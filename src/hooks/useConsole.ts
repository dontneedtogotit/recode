import { useState, useEffect } from 'react';
import type { ConsoleEntry } from '../types/frontend';

export function useConsole() {
  const [entries, setEntries] = useState<ConsoleEntry[]>([]);

  const append = (entry: ConsoleEntry) => {
    setEntries((current) => [...current, entry]);
  };

  const clear = () => {
    setEntries([]);
  };

  useEffect(() => {
    clear();
  }, []);

  return { entries, append, clear };
}
