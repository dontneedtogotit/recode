import { useCallback, useEffect, useRef, useState } from 'react';
import { getGatewayStatus, runGatewayTask } from './useGateway';

export type GatewayRunState = {
  provider: string;
  taskType: string;
  prompt: string;
  response: Record<string, unknown> | null;
  status: string | null;
  loading: boolean;
  rawResponse: string;
};

export type GatewayRunActions = {
  setProvider: (value: string) => void;
  setTaskType: (value: string) => void;
  setPrompt: (value: string) => void;
  run: () => Promise<void>;
};

export type GatewayRunResult = GatewayRunState & GatewayRunActions;

const initialState: GatewayRunState = {
  provider: 'openai-compatible',
  taskType: 'chat',
  prompt: '',
  response: null,
  status: null,
  loading: false,
  rawResponse: '',
};

export function useGatewayRun(): GatewayRunResult {
  const [state, setState] = useState<GatewayRunState>(initialState);
  const isMountedRef = useRef(true);
  const loadingRef = useRef(false);

  useEffect(() => {
    isMountedRef.current = true;
    let cancelled = false;
    getGatewayStatus().then((result) => {
      if (!cancelled && isMountedRef.current) {
        setState((current) => ({ ...current, status: result.state }));
      }
    }).catch((error) => {
      if (!cancelled && isMountedRef.current) {
        const message = error instanceof Error ? error.message : String(error);
        setState((current) => ({ ...current, status: `Gateway status unavailable: ${message}` }));
      }
    });
    return () => {
      cancelled = true;
      isMountedRef.current = false;
    };
  }, []);

  const setProvider = useCallback((value: string) => {
    setState((current) => ({ ...current, provider: value }));
  }, []);

  const setTaskType = useCallback((value: string) => {
    setState((current) => ({ ...current, taskType: value }));
  }, []);

  const setPrompt = useCallback((value: string) => {
    setState((current) => ({ ...current, prompt: value }));
  }, []);

  const run = useCallback(async () => {
    const trimmed = state.prompt.trim();
    if (!trimmed || loadingRef.current || !isMountedRef.current) return;

    loadingRef.current = true;
    setState((current) => ({ ...current, loading: true, response: null, rawResponse: '' }));
    try {
      const result = await runGatewayTask({
        provider: state.provider,
        payload: { messages: [{ role: 'user', content: trimmed }] },
        task_type: state.taskType || undefined,
      });

      if (!isMountedRef.current) return;

      const rawBody = result?.body;
      const body: Record<string, unknown> = (rawBody === null || rawBody === undefined)
        ? { error: 'empty response' }
        : (typeof rawBody === 'object' && rawBody !== null && !Array.isArray(rawBody) ? rawBody as Record<string, unknown> : { body: rawBody });

      setState((current) => ({
        ...current,
        response: body,
        rawResponse: typeof result.raw === 'string' ? result.raw : '',
        loading: false,
      }));
    } catch (error) {
      if (!isMountedRef.current) return;
      setState((current) => ({
        ...current,
        response: { error: String(error) } as Record<string, unknown>,
        loading: false,
      }));
    } finally {
      loadingRef.current = false;
      if (isMountedRef.current) {
        setState((current) => ({ ...current, loading: false }));
      }
    }
  }, [state.provider, state.prompt, state.taskType]);

  return {
    ...state,
    setProvider,
    setTaskType,
    setPrompt,
    run,
  };
}
