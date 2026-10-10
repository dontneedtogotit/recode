import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Search } from 'lucide-react';
import type { View } from './AppShell';
import { NAV_ITEMS } from './AppShell';

type PaletteAction = 'run' | 'self-heal' | 'clear-console' | 'copy-console' | 'toggle-inspector';

type CommandPaletteProps = {
  view: View;
  onNavigate: (view: View) => void;
  onRunGoal: (goal: string) => void;
  onAction?: (action: Exclude<PaletteAction, 'run'>) => void;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
};

type PaletteItem = {
  id: string;
  label: string;
  shortcut?: string;
  kind: 'view' | 'action';
  view?: View;
  action?: PaletteAction;
  goal?: string;
};

const VIEW_ITEMS: PaletteItem[] = NAV_ITEMS.map((item) => ({
  id: `view-${item.id}`,
  label: item.label,
  shortcut: item.shortcut,
  kind: 'view',
  view: item.id,
}));

const ACTION_ITEMS: PaletteItem[] = [
  { id: 'action-self-heal', label: 'Attempt self-heal', kind: 'action', action: 'self-heal' },
  { id: 'action-clear-console', label: 'Clear console', kind: 'action', action: 'clear-console' },
  { id: 'action-copy-console', label: 'Copy console', kind: 'action', action: 'copy-console' },
  { id: 'action-toggle-inspector', label: 'Toggle inspector', kind: 'action', action: 'toggle-inspector' },
];

export default function CommandPalette({ view, onNavigate, onRunGoal, onAction, open: controlledOpen, onOpenChange }: CommandPaletteProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false);
  const open = controlledOpen ?? uncontrolledOpen;
  const setOpen = useCallback((next: boolean) => {
    if (controlledOpen === undefined) setUncontrolledOpen(next);
    onOpenChange?.(next);
  }, [controlledOpen, onOpenChange]);
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  const trimmedQuery = query.trim();
  const normalized = trimmedQuery.toLowerCase();
  const runGoal = trimmedQuery.toLowerCase().startsWith('run ') ? trimmedQuery.slice(4).trim() : '';
  const items = useMemo<PaletteItem[]>(() => {
    const result: PaletteItem[] = [];
    if (runGoal) {
      result.push({ id: 'action-run-goal', label: `Run “${runGoal}”`, kind: 'action', action: 'run', goal: runGoal });
    }
    const searchable = [...VIEW_ITEMS, ...ACTION_ITEMS];
    for (const item of searchable) {
      if (!normalized || item.label.toLowerCase().includes(normalized)) {
        result.push(item);
      }
    }
    return result;
  }, [normalized, runGoal]);

  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      const isMeta = event.ctrlKey || event.metaKey;
      if (isMeta && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setOpen(!open);
        return;
      }
      if (event.key === 'Escape' && open) {
        event.preventDefault();
        setOpen(false);
      }
    };
    window.addEventListener('keydown', handle);
    return () => window.removeEventListener('keydown', handle);
  }, [open, setOpen]);

  useEffect(() => {
    if (open) {
      previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setQuery('');
      setSelectedIndex(0);
      inputRef.current?.focus();
    } else {
      previousFocusRef.current?.focus();
      previousFocusRef.current = null;
    }
  }, [open]);

  useEffect(() => {
    setSelectedIndex((current) => Math.min(current, Math.max(items.length - 1, 0)));
  }, [items.length]);

  const activate = (item: PaletteItem | undefined) => {
    if (!item) return;
    if (item.kind === 'view' && item.view) {
      onNavigate(item.view);
    } else if (item.action === 'run' && item.goal) {
      onRunGoal(item.goal);
    } else if (item.action && item.action !== 'run') {
      onAction?.(item.action);
    }
    setOpen(false);
  };

  const handleInputKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setSelectedIndex((current) => items.length ? (current + 1) % items.length : 0);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setSelectedIndex((current) => items.length ? (current - 1 + items.length) % items.length : 0);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      activate(items[selectedIndex]);
    } else if (event.key === 'Tab') {
      event.preventDefault();
      inputRef.current?.focus();
    }
  };

  const handlePaletteKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Tab') {
      event.preventDefault();
      inputRef.current?.focus();
    }
  };

  if (!open) return null;

  return (
    <div
      className="palette-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) setOpen(false);
      }}
    >
      <div className="palette" onKeyDown={handlePaletteKeyDown}>
        <div className="palette-input">
          <Search size={16} strokeWidth={1.8} />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSelectedIndex(0);
            }}
            onKeyDown={handleInputKeyDown}
            placeholder="Type a view, action, or run command…"
            role="combobox"
            aria-label="Search commands"
            aria-expanded="true"
            aria-controls="command-palette-list"
            aria-activedescendant={items[selectedIndex] ? `palette-${items[selectedIndex].id}` : undefined}
          />
          <kbd>ESC</kbd>
        </div>
        <div className="palette-body" id="command-palette-list" role="listbox" aria-label="Commands">
          {items.map((item, index) => (
            <button
              key={item.id}
              id={`palette-${item.id}`}
              type="button"
              role="option"
              aria-selected={index === selectedIndex}
              className={`palette-item ${view === item.view ? 'active' : ''} ${index === selectedIndex ? 'selected' : ''}`}
              onMouseEnter={() => setSelectedIndex(index)}
              onClick={() => activate(item)}
            >
              <span>{item.label}</span>
              {item.shortcut && <kbd>{item.shortcut}</kbd>}
            </button>
          ))}
          {items.length === 0 && <div className="palette-empty">No matches.</div>}
        </div>
        <div className="palette-footer">
          <span><kbd>↑↓</kbd> to move</span>
          <span><kbd>↵</kbd> to select</span>
          <span><kbd>esc</kbd> to close</span>
        </div>
      </div>
    </div>
  );
}

export type { PaletteAction };
