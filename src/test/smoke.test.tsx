import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ThemeProvider, useTheme } from '../lib/theme';
import { act } from '@testing-library/react';

describe('test environment', () => {
  it.each([1, 2])('isolates browser storage and DOM (pass %i)', () => {
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
    expect(screen.queryByRole('button', { name: 'Temporary' })).not.toBeInTheDocument();
    localStorage.setItem('previous-test', 'value');
    sessionStorage.setItem('previous-test', 'value');
    render(<button>Temporary</button>);
  });

  it('does not advertise a native bridge in a browser-only test', () => {
    expect(Reflect.get(globalThis, '__TAURI__')).toBeUndefined();
  });

  it('implements storage overwrite, removal, enumeration and missing keys', () => {
    localStorage.setItem('first', 'one');
    localStorage.setItem('second', 'two');
    localStorage.setItem('first', 'updated');
    expect(localStorage.length).toBe(2);
    expect(localStorage.key(0)).toBe('first');
    expect(localStorage.key(1)).toBe('second');
    expect(localStorage.key(2)).toBeNull();
    expect(localStorage.getItem('first')).toBe('updated');
    localStorage.removeItem('first');
    expect(localStorage.getItem('first')).toBeNull();
    expect(localStorage.key(0)).toBe('second');
    localStorage.clear();
    expect(localStorage.length).toBe(0);
  });
});

// Smoke test for ThemeProvider
describe('ThemeProvider', () => {
  it.each(['violet', 'emerald', 'sky', 'amber', 'rose'] as const)(
    'restores the persisted %s accent', (accent) => {
      localStorage.setItem('recode.accent', accent);
      const Consumer = () => <output aria-label="Accent">{useTheme().accent}</output>;
      render(<ThemeProvider><Consumer /></ThemeProvider>);
      expect(screen.getByLabelText('Accent')).toHaveTextContent(accent);
    },
  );

  it.each(['invalid', 'constructor', '__proto__'])('rejects invalid stored accent %s', (accent) => {
    localStorage.setItem('recode.accent', accent);
    const Consumer = () => <output aria-label="Accent">{useTheme().accent}</output>;
    render(<ThemeProvider><Consumer /></ThemeProvider>);
    expect(screen.getByLabelText('Accent')).toHaveTextContent('violet');
    expect(localStorage.getItem('recode.accent')).toBe('violet');
  });

  it('provides default accent and allows changing it', () => {
    let result: ReturnType<typeof useTheme> | undefined;
    const TestComponent = () => {
      result = useTheme();
      return null;
    };

    render(
      <ThemeProvider>
        <TestComponent />
      </ThemeProvider>
    );

    expect(result?.accent).toBe('violet');
    expect(typeof result?.setAccent).toBe('function');

    act(() => {
      result?.setAccent('emerald');
    });

    expect(result?.accent).toBe('emerald');
  });

  it('persists accent to localStorage', () => {
    const setItemSpy = vi.spyOn(window.localStorage, 'setItem');

    const TestComponent = () => {
      const { setAccent } = useTheme();
      return <button onClick={() => setAccent('sky')}>Change</button>;
    };

    render(
      <ThemeProvider>
        <TestComponent />
      </ThemeProvider>
    );

    fireEvent.click(screen.getByText('Change'));

    expect(setItemSpy).toHaveBeenCalledWith('recode.accent', 'sky');
    expect(localStorage.getItem('recode.accent')).toBe('sky');
  });
});

// Smoke test for agentHistory
import { loadAgentHistory, saveAgentHistory, pushAgentTaskRecord, createAgentHistoryState } from '../lib/agentHistory';

describe('agentHistory', () => {
  it.each(['{broken', '{}', 'null', '42'])('recovers from corrupt or non-array history: %s', (raw) => {
    localStorage.setItem('recode.agentHistory', raw);
    expect(loadAgentHistory()).toEqual(createAgentHistoryState());
  });

  it('recovers from storage read failures', () => {
    vi.spyOn(localStorage, 'getItem').mockImplementation(() => {
      throw new DOMException('Storage blocked', 'SecurityError');
    });
    expect(loadAgentHistory()).toEqual(createAgentHistoryState());
  });

  it('does not throw when storage is full', () => {
    vi.spyOn(localStorage, 'setItem').mockImplementation(() => {
      throw new DOMException('Storage full', 'QuotaExceededError');
    });
    expect(() => saveAgentHistory(createAgentHistoryState())).not.toThrow();
  });

  it('adds records without mutating previous state', () => {
    const previous = createAgentHistoryState();
    const record = { id: 'task', agentId: 'planner', input: 'goal', success: false, timestamp: 123 };
    const next = pushAgentTaskRecord(previous, record);
    expect(previous.records).toEqual([]);
    expect(next.records).toEqual([record]);
    expect(next.records).not.toBe(previous.records);
  });

  it('creates empty state', () => {
    const state = createAgentHistoryState();
    expect(state.records).toEqual([]);
  });

  it('pushes records and limits to MAX_HISTORY', () => {
    let state = createAgentHistoryState();
    for (let i = 0; i < 55; i++) {
      state = pushAgentTaskRecord(state, {
        id: `task-${i}`,
        agentId: 'test',
        input: `input-${i}`,
        success: true,
        timestamp: i,
      });
    }
    expect(state.records.length).toBe(50);
    expect(state.records[0].id).toBe('task-54');
    expect(state.records.map((record) => record.id)).toEqual(
      Array.from({ length: 50 }, (_, index) => `task-${54 - index}`),
    );
  });

  it('saves and loads from localStorage', () => {
    const state = createAgentHistoryState();
    const record = {
      id: 'test-1',
      agentId: 'planner',
      input: 'test input',
      success: true,
      timestamp: 123456,
    };
    const newState = pushAgentTaskRecord(state, record);
    saveAgentHistory(newState);

    const loaded = loadAgentHistory();
    expect(loaded.records).toHaveLength(1);
    expect(loaded.records[0].id).toBe('test-1');
    expect(loaded).toEqual(newState);
  });
});

// Smoke test for ipc normalize functions (re-exported for testing)
import {
  normalizeAgentTaskResult as normalizeAgentTaskResultInternal,
  normalizePipelineEvent as normalizePipelineEventInternal,
  normalizeLearningPatterns as normalizeLearningPatternsInternal,
  normalizeAgentRuntimeSettings as normalizeAgentRuntimeSettingsInternal,
} from '../lib/ipc';

describe('ipc normalization', () => {
  it('normalizes agent task result with snake_case task_id', () => {
    const result = normalizeAgentTaskResultInternal({ task_id: 'abc-123', agent_id: 'planner', success: true, output: 'done' }, 'fallback');
    expect(result.taskId).toBe('abc-123');
    expect(result.agentId).toBe('planner');
    expect(result.success).toBe(true);
  });

  it('normalizes agent task result with camelCase taskId', () => {
    const result = normalizeAgentTaskResultInternal({ taskId: 'camel-case', agentId: 'planner', success: true }, 'fallback');
    expect(result.taskId).toBe('camel-case');
  });

  it('normalizes pipeline event with snake_case fields', () => {
    const event = normalizePipelineEventInternal({
      run_id: 'run-1',
      topic: 'step',
      severity: 'info',
      message: 'hello',
      ts: 123456,
    });
    expect(event).not.toBeNull();
    expect(event?.runId).toBe('run-1');
    expect(event?.topic).toBe('step');
  });

  it('returns null for invalid pipeline event', () => {
    expect(normalizePipelineEventInternal({})).toBeNull();
    expect(normalizePipelineEventInternal(null)).toBeNull();
  });

  it('normalizes learning patterns', () => {
    const patterns = normalizeLearningPatternsInternal([
      { id: 'p1', agent_role: 'planner', task_type: 'code', content: 'pattern', score: 0.8, sample_count: 5, updated_at: 123 },
    ], 'success');
    expect(patterns).toHaveLength(1);
    expect(patterns[0].kind).toBe('success');
    expect(patterns[0].score).toBe(0.8);
  });

  it('normalizes agent runtime settings with snake_case keys', () => {
    const settings = normalizeAgentRuntimeSettingsInternal({
      max_concurrency: 3,
      default_timeout_ms: 5000,
      retry_limit: 2,
    });
    expect(settings.maxConcurrency).toBe(3);
    expect(settings.defaultTimeoutMs).toBe(5000);
    expect(settings.retryLimit).toBe(2);
  });

  it('uses defaults for missing/invalid settings', () => {
    const settings = normalizeAgentRuntimeSettingsInternal({
      max_concurrency: 'invalid',
      default_timeout_ms: -1,
    });
    expect(settings.maxConcurrency).toBe(2);
    expect(settings.defaultTimeoutMs).toBe(60000);
  });
});