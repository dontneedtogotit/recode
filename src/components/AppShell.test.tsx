import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import AppShell, { NAV_ITEMS } from './AppShell';
import { Field } from './Field';

const baseProps = {
  onNavigate: vi.fn(),
  rightPanel: null,
  console: null,
  children: <div>content</div>,
};

describe('AppShell', () => {
  it('associates implicit field labels with their controls', () => {
    render(<Field label="Provider"><input defaultValue="gateway" /></Field>);
    expect(screen.getByLabelText('Provider')).toHaveValue('gateway');
  });

  it('uses the explicit run state when the inspector is closed', () => {
    render(<AppShell {...baseProps} view="project" runState={{ status: 'running', goal: 'ship' }} />);
    expect(screen.getByText('Run active')).toBeInTheDocument();
    expect(screen.getByText('Running')).toBeInTheDocument();
    expect(screen.queryByText('Direct dev path active')).not.toBeInTheDocument();
  });

  it('keeps navigation shortcut metadata sequential in DOM order', () => {
    expect(NAV_ITEMS.map((item) => item.shortcut)).toEqual(['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'L']);
  });

  it('calls the console close handler', () => {
    const onClose = vi.fn();
    render(
      <AppShell
        {...baseProps}
        view="runner"
        runState={{ status: 'idle' }}
        console={{ entries: [], runState: { status: 'idle' }, onRunAgentTask: vi.fn(), onClose }}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('exposes global inspector and console controls', () => {
    const onToggleInspector = vi.fn();
    const onToggleConsole = vi.fn();
    render(<AppShell {...baseProps} view="project" runState={{ status: 'idle' }} onToggleInspector={onToggleInspector} onToggleConsole={onToggleConsole} />);
    fireEvent.click(screen.getByRole('button', { name: 'Toggle inspector' }));
    fireEvent.click(screen.getByRole('button', { name: 'Toggle console' }));
    expect(onToggleInspector).toHaveBeenCalledOnce();
    expect(onToggleConsole).toHaveBeenCalledOnce();
  });

  it('does not intercept native button activation while the drawer is open', () => {
    const action = vi.fn();
    render(<AppShell {...baseProps} view="project" runState={{ status: 'idle' }}><button onClick={action}>Local action</button></AppShell>);
    fireEvent.click(screen.getByRole('button', { name: 'Toggle navigation' }));
    screen.getByRole('button', { name: /Project/ }).focus();
    const button = screen.getByRole('button', { name: 'Local action' });
    button.focus();
    expect(fireEvent.keyDown(button, { key: 'Enter' })).toBe(true);
  });
});
