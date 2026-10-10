import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { vi, beforeEach, afterEach } from 'vitest';

// Keep platform doubles here, not application/service mocks. Each test gets
// fresh implementations as well as empty data, so throwing spies cannot leak.
function createStorage(): Storage {
  const store = new Map<string, string>();
  return {
    getItem: vi.fn((key: string) => store.get(String(key)) ?? null),
    setItem: vi.fn((key: string, value: string) => { store.set(String(key), String(value)); }),
    removeItem: vi.fn((key: string) => { store.delete(String(key)); }),
    clear: vi.fn(() => { store.clear(); }),
    key: vi.fn((index: number) => Array.from(store.keys())[index >>> 0] ?? null),
    get length() { return store.size; },
  };
}

beforeEach(() => {
  vi.stubGlobal('localStorage', createStorage());
  vi.stubGlobal('sessionStorage', createStorage());

  // Native integration tests must explicitly supply their bridge. A permissive
  // invoke() stub returning undefined would hide unavailable-runtime failures.
  Reflect.deleteProperty(globalThis, '__TAURI__');

  vi.stubGlobal('matchMedia', vi.fn((query: string): MediaQueryList => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(() => true),
  })));

  vi.stubGlobal('ResizeObserver', class implements ResizeObserver {
    observe = vi.fn();
    unobserve = vi.fn();
    disconnect = vi.fn();
  });
});

afterEach(() => {
  // Unmount before restoring platform APIs so effect cleanups see their mocks.
  try {
    cleanup();
  } finally {
    vi.restoreAllMocks();
    vi.clearAllMocks();
    vi.useRealTimers();
    vi.unstubAllGlobals();
    Reflect.deleteProperty(globalThis, '__TAURI__');
  }
});