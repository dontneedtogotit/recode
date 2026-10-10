import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ACCENT_MAP, ThemeProvider, useTheme } from './theme';

function ThemeProbe() {
  const theme = useTheme();
  return <>
    <output data-testid="theme">{theme.accent}/{theme.themeMode}/{theme.effectiveTheme}/{theme.density}</output>
    <button onClick={() => theme.setAccent('sky')}>Sky</button>
    <button onClick={() => theme.setDensity('compact')}>Compact</button>
    <button onClick={() => theme.setThemeMode('light')}>Light</button>
    <button onClick={() => theme.setThemeMode('system')}>System</button>
  </>;
}

function mockSystemTheme(dark: boolean) {
  const listeners = new Set<() => void>();
  const query = {
    matches: dark,
    addEventListener: vi.fn((_event: string, listener: () => void) => listeners.add(listener)),
    removeEventListener: vi.fn((_event: string, listener: () => void) => listeners.delete(listener)),
  };
  vi.stubGlobal('matchMedia', vi.fn(() => query));
  return {
    query,
    listeners,
    change(matches: boolean) {
      act(() => {
        query.matches = matches;
        listeners.forEach((listener) => listener());
      });
    },
  };
}

describe('theme accents', () => {
  it('defines an RGB channel token for every accent', () => {
    for (const [name, colors] of Object.entries(ACCENT_MAP)) {
      expect(name).toMatch(/^(violet|emerald|sky|amber|rose)$/);
      expect(colors.rgb).toMatch(/^\d{1,3}, \d{1,3}, \d{1,3}$/);
      expect(colors.accent).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it('exports a provider for the application theme boundary', () => {
    expect(ThemeProvider).toBeTypeOf('function');
  });
});

describe('theme preferences', () => {
  it('restores preferences and renders the matching CSS tokens', () => {
    mockSystemTheme(true);
    localStorage.setItem('recode.accent', 'emerald');
    localStorage.setItem('recode.themeMode', 'light');
    localStorage.setItem('recode.density', 'compact');
    const { container } = render(<ThemeProvider><ThemeProbe /></ThemeProvider>);
    expect(screen.getByTestId('theme').textContent).toBe('emerald/light/light/compact');
    const root = container.firstElementChild as HTMLElement;
    expect(root.style.getPropertyValue('--accent')).toBe(ACCENT_MAP.emerald.accent);
    expect(root.style.getPropertyValue('--bg-base')).toBe('#f8fafc');
    expect(root.style.getPropertyValue('--ui-scale')).toBe('0.85');
  });

  it.each(['invalid', '__proto__', 'constructor', ''])('rejects invalid preferences: %s', (value) => {
    mockSystemTheme(true);
    for (const key of ['accent', 'themeMode', 'density']) localStorage.setItem(`recode.${key}`, value);
    render(<ThemeProvider><ThemeProbe /></ThemeProvider>);
    expect(screen.getByTestId('theme').textContent).toBe('violet/system/dark/comfortable');
  });

  it('persists independent preference changes', () => {
    mockSystemTheme(true);
    render(<ThemeProvider><ThemeProbe /></ThemeProvider>);
    fireEvent.click(screen.getByText('Sky'));
    fireEvent.click(screen.getByText('Compact'));
    fireEvent.click(screen.getByText('Light'));
    expect(localStorage.getItem('recode.accent')).toBe('sky');
    expect(localStorage.getItem('recode.density')).toBe('compact');
    expect(localStorage.getItem('recode.themeMode')).toBe('light');
    expect(screen.getByTestId('theme').textContent).toBe('sky/light/light/compact');
  });

  it.each(['getItem', 'setItem'] as const)('remains interactive when storage %s throws', (method) => {
    mockSystemTheme(true);
    vi.spyOn(localStorage, method).mockImplementation(() => { throw new Error('storage blocked'); });
    render(<ThemeProvider><ThemeProbe /></ThemeProvider>);
    fireEvent.click(screen.getByText('Sky'));
    fireEvent.click(screen.getByText('Light'));
    expect(screen.getByTestId('theme').textContent).toBe('sky/light/light/comfortable');
  });

  it('survives a blocked localStorage getter', () => {
    mockSystemTheme(true);
    vi.spyOn(window, 'localStorage', 'get').mockImplementation(() => { throw new Error('blocked'); });
    render(<ThemeProvider><ThemeProbe /></ThemeProvider>);
    expect(screen.getByTestId('theme').textContent).toBe('violet/system/dark/comfortable');
  });

  it('tracks system changes only in system mode and disposes subscriptions', () => {
    const system = mockSystemTheme(false);
    const { unmount } = render(<ThemeProvider><ThemeProbe /></ThemeProvider>);
    expect(screen.getByTestId('theme').textContent).toBe('violet/system/light/comfortable');
    system.change(true);
    expect(screen.getByTestId('theme').textContent).toBe('violet/system/dark/comfortable');
    fireEvent.click(screen.getByText('Light'));
    expect(system.listeners.size).toBe(0);
    system.change(false);
    system.change(true);
    expect(screen.getByTestId('theme').textContent).toBe('violet/light/light/comfortable');
    fireEvent.click(screen.getByText('System'));
    expect(system.listeners.size).toBe(1);
    expect(screen.getByTestId('theme').textContent).toBe('violet/system/dark/comfortable');
    unmount();
    expect(system.listeners.size).toBe(0);
    expect(system.query.removeEventListener).toHaveBeenCalledTimes(2);
  });

  it('uses a stable dark fallback without matchMedia', () => {
    vi.stubGlobal('matchMedia', undefined);
    render(<ThemeProvider><ThemeProbe /></ThemeProvider>);
    expect(screen.getByTestId('theme').textContent).toBe('violet/system/dark/comfortable');
    fireEvent.click(screen.getByText('Light'));
    expect(screen.getByTestId('theme').textContent).toBe('violet/light/light/comfortable');
  });

  it('can render without browser globals', () => {
    vi.stubGlobal('window', undefined);
    expect(renderToStaticMarkup(<ThemeProvider><ThemeProbe /></ThemeProvider>)).toContain('violet/system/dark/comfortable');
  });
});
