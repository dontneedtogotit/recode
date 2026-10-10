import { createContext, useContext, useEffect, useState, type CSSProperties } from 'react';

export type Accent = 'violet' | 'emerald' | 'sky' | 'amber' | 'rose';
export type ThemeMode = 'light' | 'dark' | 'system';
export type Density = 'compact' | 'comfortable';

type ThemeContextValue = {
  accent: Accent;
  setAccent: (accent: Accent) => void;
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
  effectiveTheme: 'light' | 'dark';
  density: Density;
  setDensity: (density: Density) => void;
};

type AccentColors = {
  accent: string;
  rgb: string;
  soft: string;
  strong: string;
};

export const ACCENT_MAP: Record<Accent, AccentColors> = {
  violet: { accent: '#8b5cf6', rgb: '139, 92, 246', soft: 'rgba(139, 92, 246, 0.28)', strong: 'rgba(139, 92, 246, 0.65)' },
  emerald: { accent: '#10b981', rgb: '16, 185, 129', soft: 'rgba(16, 185, 129, 0.28)', strong: 'rgba(16, 185, 129, 0.65)' },
  sky: { accent: '#38bdf8', rgb: '56, 189, 248', soft: 'rgba(56, 189, 248, 0.28)', strong: 'rgba(56, 189, 248, 0.65)' },
  amber: { accent: '#f59e0b', rgb: '245, 158, 11', soft: 'rgba(245, 158, 11, 0.28)', strong: 'rgba(245, 158, 11, 0.65)' },
  rose: { accent: '#f43f5e', rgb: '244, 63, 94', soft: 'rgba(244, 63, 94, 0.28)', strong: 'rgba(244, 63, 94, 0.65)' },
};

// Light theme colors (inverted from dark)
const LIGHT_THEME = {
  '--bg-base': '#f8fafc',
  '--bg-surface': '#ffffff',
  '--bg-elevated': '#f1f5f9',
  '--bg-inset': '#e2e8f0',
  '--bg-base-rgb': '248, 250, 252',
  '--bg-surface-rgb': '255, 255, 255',
  '--bg-elevated-rgb': '241, 245, 249',
  '--bg-inset-rgb': '226, 232, 240',
  '--border-subtle': 'rgba(0, 0, 0, 0.07)',
  '--border-medium': 'rgba(0, 0, 0, 0.13)',
  '--border-strong': 'rgba(0, 0, 0, 0.18)',
  '--text-primary': '#0f172a',
  '--text-secondary': '#475569',
  '--text-muted': '#64748b',
  '--success': '#15803d',
  '--success-soft': 'rgba(22, 163, 74, 0.18)',
  '--warning': '#a16207',
  '--warning-soft': 'rgba(217, 119, 6, 0.18)',
  '--danger': '#dc2626',
  '--danger-soft': 'rgba(220, 38, 38, 0.18)',
  '--info': '#2563eb',
  '--info-soft': 'rgba(37, 99, 235, 0.18)',
  '--shadow-soft': '0 10px 28px rgba(0, 0, 0, 0.1)',
  '--shadow-glow': '0 0 0 1px rgb(var(--accent-rgb) / 0.35), 0 10px 28px rgba(0, 0, 0, 0.1)',
};

const DARK_THEME = {
  '--bg-base': '#0b0d12',
  '--bg-surface': '#11141b',
  '--bg-elevated': '#181c25',
  '--bg-inset': '#0e1117',
  '--bg-base-rgb': '11, 13, 18',
  '--bg-surface-rgb': '17, 20, 27',
  '--bg-elevated-rgb': '24, 28, 37',
  '--bg-inset-rgb': '14, 17, 23',
  '--border-subtle': 'rgba(255, 255, 255, 0.07)',
  '--border-medium': 'rgba(255, 255, 255, 0.13)',
  '--border-strong': 'rgba(255, 255, 255, 0.18)',
  '--text-primary': '#f1f3f7',
  '--text-secondary': '#a1a9b6',
  '--text-muted': '#7d8595',
  '--success': '#22c55e',
  '--success-soft': 'rgba(34, 197, 94, 0.18)',
  '--warning': '#f59e0b',
  '--warning-soft': 'rgba(245, 158, 11, 0.18)',
  '--danger': '#ef4444',
  '--danger-soft': 'rgba(239, 68, 68, 0.18)',
  '--info': '#60a5fa',
  '--info-soft': 'rgba(96, 165, 250, 0.18)',
  '--shadow-soft': '0 10px 28px rgba(0, 0, 0, 0.35)',
  '--shadow-glow': '0 0 0 1px rgb(var(--accent-rgb) / 0.35), 0 10px 28px rgba(0, 0, 0, 0.45)',
};

const DENSITY = {
  compact: { '--ui-scale': '0.85', '--spacing-mult': '0.8' },
  comfortable: { '--ui-scale': '1', '--spacing-mult': '1' },
};

function isAccent(value: string | null): value is Accent {
  return value !== null && Object.prototype.hasOwnProperty.call(ACCENT_MAP, value);
}

function isThemeMode(value: string | null): value is ThemeMode {
  return value !== null && ['light', 'dark', 'system'].includes(value);
}

function isDensity(value: string | null): value is Density {
  return value !== null && ['compact', 'comfortable'].includes(value);
}

export const ThemeContext = createContext<ThemeContextValue>({
  accent: 'violet',
  setAccent: () => {},
  themeMode: 'system',
  setThemeMode: () => {},
  effectiveTheme: 'dark',
  density: 'comfortable',
  setDensity: () => {},
});

export function useTheme() {
  return useContext(ThemeContext);
}

function readPreference(key: string): string | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writePreference(key: string, value: string): void {
  try {
    if (typeof window !== 'undefined') window.localStorage.setItem(key, value);
  } catch {
    // Theme changes remain usable when browser persistence is unavailable.
  }
}

function systemThemeQuery(): MediaQueryList | undefined {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-color-scheme: dark)') : undefined;
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [accent, setAccent] = useState<Accent>(() => {
    const stored = readPreference('recode.accent');
    return isAccent(stored) ? stored : 'violet';
  });

  const [themeMode, setThemeMode] = useState<ThemeMode>(() => {
    const stored = readPreference('recode.themeMode');
    return isThemeMode(stored) ? stored : 'system';
  });

  const [density, setDensity] = useState<Density>(() => {
    const stored = readPreference('recode.density');
    return isDensity(stored) ? stored : 'comfortable';
  });

  const [systemDark, setSystemDark] = useState(() => systemThemeQuery()?.matches ?? true);
  const effectiveTheme = themeMode === 'system' ? (systemDark ? 'dark' : 'light') : themeMode;

  useEffect(() => {
    writePreference('recode.accent', accent);
  }, [accent]);

  useEffect(() => {
    writePreference('recode.themeMode', themeMode);
  }, [themeMode]);

  useEffect(() => {
    writePreference('recode.density', density);
  }, [density]);

  // Compute effective theme based on mode and system preference
  useEffect(() => {
    if (themeMode !== 'system') return;
    const mediaQuery = systemThemeQuery();
    if (!mediaQuery) return;
    const handler = () => setSystemDark(mediaQuery.matches);
    handler();
    mediaQuery.addEventListener('change', handler);
    return () => mediaQuery.removeEventListener('change', handler);
  }, [themeMode]);

  const colors = ACCENT_MAP[accent];
  const themeVars = effectiveTheme === 'light' ? LIGHT_THEME : DARK_THEME;
  const densityVars = DENSITY[density];
  const style: CSSProperties & Record<`--${string}`, string> = {
    '--accent': colors.accent,
    '--accent-rgb': colors.rgb,
    '--accent-soft': colors.soft,
    '--accent-strong': colors.strong,
    ...themeVars,
    ...densityVars,
  };

  return (
    <ThemeContext.Provider value={{ accent, setAccent, themeMode, setThemeMode, effectiveTheme, density, setDensity }}>
      <div
        className="theme-root"
        data-theme={effectiveTheme}
        data-density={density}
        style={style}
      >
        {children}
      </div>
    </ThemeContext.Provider>
  );
}