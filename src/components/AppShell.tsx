import type { ReactNode } from 'react';
import { useState, useEffect, useRef, useCallback } from 'react';
import type { RightPanelTab, ConsoleEntry, RunState } from '../types/frontend';
import { NavIcons } from './NavIcons';
import { EmptyState } from './EmptyState';
import { Code2, Menu, PanelRight, Search, Terminal } from 'lucide-react';

export type View = 'project' | 'agent' | 'runner' | 'diagnostics' | 'plans' | 'ai' | 'learning' | 'history' | 'telemetry' | 'settings' | 'updates';

const VIEW_LABELS: Record<View, string> = {
  project: 'Project',
  agent: 'Agent chat',
  runner: 'Runner',
  diagnostics: 'Diagnostics',
  plans: 'Plans',
  ai: 'AI gateway',
  learning: 'Learning',
  history: 'Run history',
  telemetry: 'Telemetry',
  settings: 'Settings',
  updates: 'Updates',
};

function statusClass(status: RunState['status']): string {
  return status === 'idle' ? '' : status === 'failed' ? 'err' : status === 'running' ? 'warn' : 'ok';
}

type ShellProps = {
  view: View;
  onNavigate: (view: View) => void;
  onOpenCommandPalette?: () => void;
  onToggleInspector?: () => void;
  onToggleConsole?: () => void;
  runState: RunState;
  rightPanel: {
    tab: RightPanelTab;
    onChangeTab: (tab: RightPanelTab) => void;
    onClose: () => void;
    runState: RunState;
    consoleEntries: ConsoleEntry[];
    onRunAgentTask: (agentId: string, input: string) => void;
    onToggleConsole: () => void;
  } | null;
  console: { entries: ConsoleEntry[]; runState: RunState; onRunAgentTask: (agentId: string, input: string) => void; onClose: () => void } | null;
  accentPicker?: ReactNode;
  children: ReactNode;
};

export const NAV_ITEMS: { id: View; label: string; shortcut: string; icon: keyof typeof NavIcons; group?: string }[] = [
  { id: 'project', label: 'Project', shortcut: '1', icon: 'project', group: 'work' },
  { id: 'agent', label: 'Agent', shortcut: '2', icon: 'agent', group: 'work' },
  { id: 'runner', label: 'Runner', shortcut: '3', icon: 'runner', group: 'work' },
  { id: 'diagnostics', label: 'Diagnostics', shortcut: '4', icon: 'diagnostics', group: 'inspect' },
  { id: 'plans', label: 'Plans', shortcut: '5', icon: 'plans', group: 'work' },
  { id: 'telemetry', label: 'Telemetry', shortcut: '6', icon: 'telemetry', group: 'inspect' },
  { id: 'settings', label: 'Settings', shortcut: '7', icon: 'settings', group: 'system' },
  { id: 'updates', label: 'Updates', shortcut: '8', icon: 'updates', group: 'system' },
  { id: 'history', label: 'History', shortcut: '9', icon: 'history', group: 'inspect' },
  { id: 'ai', label: 'AI gateway', shortcut: '0', icon: 'ai', group: 'system' },
  { id: 'learning', label: 'Learning', shortcut: 'L', icon: 'learning', group: 'inspect' },
];

const VIEW_BY_SHORTCUT = new Map(NAV_ITEMS.map((item) => [item.shortcut.toLowerCase(), item.id]));

const NAV_GROUPS = [
  { id: 'work', label: 'Build' },
  { id: 'inspect', label: 'Observe' },
  { id: 'system', label: 'Configure' },
] as const;

function RunStatusLabel({ status }: { status: RunState['status'] }) {
  const map = {
    idle: 'Ready',
    running: 'Running',
    completed: 'Completed',
    failed: 'Failed',
  } as const;
  return <span className="run-status-label">{map[status]}</span>;
}

export default function AppShell({ view, onNavigate, onOpenCommandPalette, onToggleInspector, onToggleConsole, runState, rightPanel, console: consoleState, accentPicker, children }: ShellProps) {
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const menuRef = useRef<HTMLButtonElement>(null);
  const navRef = useRef<(HTMLButtonElement | null)[]>([]);
  const focusedIndexRef = useRef(-1);
  const flatNavItems = NAV_GROUPS.flatMap((group) => NAV_ITEMS.filter((item) => item.group === group.id));

  const focusNavItem = useCallback((index: number) => {
    if (index >= 0 && index < navRef.current.length) {
      navRef.current[index]?.focus();
      focusedIndexRef.current = index;
    }
  }, []);

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      const isMeta = event.ctrlKey || event.metaKey;
      if (event.defaultPrevented || isMeta || event.altKey) return;
      if (event.key === 'Escape' && sidebarOpen) {
        setSidebarOpen(false);
        menuRef.current?.focus();
        return;
      }
      if (event.target instanceof HTMLElement && event.target.closest('input, textarea, select, [contenteditable="true"], [role="dialog"]')) {
        return;
      }
      const target = VIEW_BY_SHORTCUT.get(event.key.toLowerCase());
      if (target) {
        event.preventDefault();
        onNavigate(target);
        return;
      }
      if (event.target instanceof HTMLElement && event.target.closest('.nav') && navRef.current.length > 0) {
        if (event.key === 'ArrowDown') {
          event.preventDefault();
          const next = Math.min(focusedIndexRef.current + 1, navRef.current.length - 1);
          focusNavItem(next);
        } else if (event.key === 'ArrowUp') {
          event.preventDefault();
          const next = Math.max(focusedIndexRef.current - 1, 0);
          focusNavItem(next);
        } else if (event.key === 'Home') {
          event.preventDefault();
          focusNavItem(0);
        } else if (event.key === 'End') {
          event.preventDefault();
          focusNavItem(navRef.current.length - 1);
        }
      }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onNavigate, sidebarOpen, focusNavItem]);

  const handleNavClick = (item: typeof NAV_ITEMS[0]) => {
    onNavigate(item.id);
    setSidebarOpen(false);
  };

  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace-content">Skip to content</a>
      <button
        ref={menuRef}
        type="button"
        className="sidebar-toggle"
        onClick={() => setSidebarOpen((current) => !current)}
        aria-label="Toggle navigation"
        aria-expanded={sidebarOpen}
      >
        <Menu size={18} aria-hidden="true" />
        <span className="sidebar-toggle-label">{sidebarOpen ? 'Close' : 'Menu'}</span>
      </button>

      {sidebarOpen && (
        <div
          className="sidebar-backdrop"
          onClick={() => setSidebarOpen(false)}
          aria-hidden="true"
        />
      )}

      <aside className={`sidebar ${sidebarOpen ? 'open' : ''}`} aria-label="Primary">
        <div>
          <div className="brand">
            <span className="brand-mark"><Code2 size={20} aria-hidden="true" /></span>
            <div><span>recode</span><span className="brand-caption">Development workbench</span></div>
          </div>
          <nav className="nav" aria-label="Main">
            {NAV_GROUPS.map((group, groupIndex) => (
              <div key={group.id} className="stack-sm">
                {groupIndex > 0 && <div className="nav-separator" aria-hidden="true" />}
                <div className="nav-group-label">{group.label}</div>
                {NAV_ITEMS.filter((item) => item.group === group.id).map((item) => {
                  const globalIndex = flatNavItems.findIndex((i) => i.id === item.id);
                  return (
                    <button
                      ref={(el) => { navRef.current[globalIndex] = el ?? null; }}
                      onFocus={() => { focusedIndexRef.current = globalIndex; }}
                      key={item.id}
                      type="button"
                      onClick={() => handleNavClick(item)}
                      className={`nav-item ${view === item.id ? 'active' : ''}`}
                      title={`${item.label} (${item.shortcut})`}
                      aria-current={view === item.id ? 'page' : undefined}
                    >
                      <span className="nav-item-icon" aria-hidden="true">
                        {NavIcons[item.icon]({ 'aria-hidden': 'true' })}
                      </span>
                      <span className="nav-item-label">{item.label}</span>
                      <span className="nav-item-shortcut">{item.shortcut}</span>
                    </button>
                  );
                })}
              </div>
            ))}
          </nav>
        </div>

        <div className="sidebar-footer">
          <div className="run-status">
            <span className={`run-status-dot ${runState.status}`} />
            <RunStatusLabel status={runState.status} />
          </div>
          {accentPicker && <details className="appearance-settings"><summary>Appearance</summary><div className="appearance-body">{accentPicker}</div></details>}
        </div>
      </aside>

      <div className="main-stage">
        <header className="topbar" aria-label="Application header">
          <div className="topbar-heading">
            <span className="topbar-eyebrow">{NAV_GROUPS.find((group) => group.id === NAV_ITEMS.find((item) => item.id === view)?.group)?.label}</span>
            <span className="topbar-title">{VIEW_LABELS[view]}</span>
          </div>
          <div className="topbar-actions">
            <div className={`topbar-status badge ${statusClass(runState.status)}`}>
              <span className={`run-status-dot ${runState.status}`} />
              {runState.status === 'running' ? 'Run active' : runState.status === 'completed' ? 'Run complete' : runState.status === 'failed' ? 'Run failed' : 'Ready'}
            </div>
            <button type="button" onClick={onOpenCommandPalette} className="topbar-command" aria-label="Open command palette" disabled={!onOpenCommandPalette}>
              <Search size={15} aria-hidden="true" />
              <span>Search commands</span>
              <kbd>Ctrl/⌘ K</kbd>
            </button>
            {onToggleConsole && <button type="button" className="ghost shell-tool" onClick={onToggleConsole} aria-label="Toggle console" title="Toggle console (`)" aria-pressed={Boolean(consoleState)}><Terminal size={17} aria-hidden="true" /></button>}
            {onToggleInspector && <button type="button" className="ghost shell-tool" onClick={onToggleInspector} aria-label="Toggle inspector" title="Toggle inspector" aria-pressed={Boolean(rightPanel)}><PanelRight size={17} aria-hidden="true" /></button>}
          </div>
        </header>
        <main className="main" id="workspace-content" tabIndex={-1}>
          {children}
        </main>

        {rightPanel && (
          <aside className="right-panel" aria-label="Inspector">
            <div className="right-panel-header">
              <div className="right-panel-tabs">
                {(['details', 'chat', 'history', 'telemetry'] as const).map((tab) => (
                  <button
                    key={tab}
                    type="button"
                    onClick={() => rightPanel.onChangeTab(tab)}
                    className={`right-panel-tab ${rightPanel.tab === tab ? 'active' : ''}`}
                    aria-pressed={rightPanel.tab === tab}
                  >
                    {tab === 'details' ? 'Details' : tab === 'chat' ? 'Chat' : tab === 'history' ? 'History' : 'Telemetry'}
                  </button>
                ))}
              </div>
              <button type="button" onClick={rightPanel.onClose} className="right-panel-close">
                Close
              </button>
            </div>
            <div className="right-panel-body">
              {rightPanel.tab === 'details' && (
                <div className="stack-sm">
                  <div className="font-bold">Run state</div>
                  <div className="status-pill">
                    <span className={`run-status-dot ${rightPanel.runState.status}`} />
                    <span className="capitalize">{rightPanel.runState.status}</span>
                  </div>
                  <div className="field">
                    <span className="placeholder-body">Goal</span>
                    <div className="placeholder-title" style={{ marginTop: 2 }}>{rightPanel.runState.goal ?? '—'}</div>
                  </div>
                  {rightPanel.runState.status === 'completed' && (
                    <div className="badge ok">Plan {rightPanel.runState.planId}</div>
                  )}
                  {rightPanel.runState.status === 'failed' && (
                    <div className="badge err">{rightPanel.runState.error}</div>
                  )}
                </div>
              )}
              {rightPanel.tab === 'chat' && (
                <div className="stack-sm">
                  <div className="font-bold">Agent chat</div>
                  <div className="inspector-empty">
                    <div className="placeholder-title">Open the full chat workspace</div>
                    <div className="placeholder-body">Use the Agent view for conversation history, capabilities, and task controls.</div>
                  </div>
                  <button type="button" onClick={() => onNavigate('agent')} className="secondary">Open Agent chat</button>
                </div>
              )}
              {rightPanel.tab === 'history' && (
                <div className="stack-sm">
                  <div className="font-bold">Recent console</div>
                  {rightPanel.consoleEntries.length === 0 && (
                    <div className="placeholder">
                      <div className="placeholder-title">No history yet</div>
                      <div className="placeholder-body">Run a pipeline or agent task to populate recent events.</div>
                    </div>
                  )}
                  <div
                    role="log"
                    aria-live="polite"
                    aria-label="Recent console history"
                    style={{ display: 'flex', flexDirection: 'column', gap: 4 }}
                  >
                    {rightPanel.consoleEntries.slice(-20).reverse().map((entry) => (
                      <div key={entry.id} className={`console-entry console-entry-${entry.level}`}>
                        <span className="console-entry-time">{new Date(entry.ts).toLocaleTimeString()}</span>
                        <span className="console-entry-source">[{entry.source}]</span>
                        <span>{entry.message}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              {rightPanel.tab === 'telemetry' && (
                <div className="stack-sm">
                  <div className="font-bold">Telemetry snapshot</div>
                  <div className="inspector-empty">
                    <div className="placeholder-title">Use Telemetry for benchmarks</div>
                    <div className="placeholder-body">Run an IPC benchmark and inspect runtime metrics in the dedicated Telemetry view.</div>
                  </div>
                  <div className="metric-grid">
                    <div className="metric-card">
                      <div className="metric-label">Console entries</div>
                      <div className="metric-value">{rightPanel.consoleEntries.length}</div>
                    </div>
                    <div className="metric-card">
                      <div className="metric-label">Run state</div>
                      <div className="metric-value capitalize">{rightPanel.runState.status}</div>
                    </div>
                  </div>
                  <button type="button" onClick={() => onNavigate('telemetry')} className="secondary">Open Telemetry</button>
                </div>
              )}
            </div>
          </aside>
        )}

        {consoleState && (
          <section className="console-drawer" aria-label="Console">
            <div className="console-header">
              <div className="console-title">Console</div>
              <button type="button" onClick={consoleState.onClose} className="console-close">
                Close
              </button>
            </div>
            <div className="console-body" role="log" aria-live="polite" aria-label="Console output">
              {consoleState.entries.length > 0 ? consoleState.entries.map((entry) => (
                <div key={entry.id} className={`console-entry console-entry-${entry.level}`}>
                  <span className="console-entry-time">{new Date(entry.ts).toLocaleTimeString()}</span>
                  <span className="console-entry-source">[{entry.source}]</span>
                  <span>{entry.message}</span>
                </div>
              )) : (
                <EmptyState
                  title="No output yet"
                  description="Start a pipeline or run an agent task to see live output here."
                />
              )}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}