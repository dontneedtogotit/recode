import { useEffect, useRef, useState } from 'react';
import { getGatewayStatus } from '../lib/ipc';

type CheckState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'reported'; state: string }
  | { status: 'error'; message: string };

/** Passive native configuration inspection: never dispatches a provider request. */
export function GatewayConfigurationCheck() {
  const [check, setCheck] = useState<CheckState>({ status: 'idle' });
  const inFlight = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const inspect = async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setCheck({ status: 'checking' });
    try {
      const result = await getGatewayStatus();
      if (mounted.current) setCheck({ status: 'reported', state: result.state });
    } catch (error) {
      if (mounted.current) setCheck({ status: 'error', message: error instanceof Error ? error.message : 'Configuration check unavailable' });
    } finally {
      inFlight.current = false;
    }
  };

  const state = check.status === 'reported' ? check.state.toLowerCase() : '';
  const description = state === 'unconfigured'
    ? 'Gateway is not configured. Set the generation endpoint and model in the app environment, then restart the desktop app.'
    : state === 'closed'
      ? 'Gateway configuration is present. Provider connectivity, credentials and model availability have not been tested.'
      : state === 'open' || state === 'halfopen' || state === 'half_open'
        ? 'Gateway reports a restricted circuit state. Inspect the gateway before attempting generation.'
        : 'The runtime did not report a recognized configuration state. Readiness remains unknown.';

  return (
    <div className="stack-sm gateway-configuration-check">
      <button type="button" className="secondary compact-button" disabled={check.status === 'checking'} onClick={() => void inspect()}>
        {check.status === 'checking' ? 'Checking configuration…' : 'Check gateway configuration'}
      </button>
      <span className="text-xs text-muted">Read-only runtime check. No model request or workspace write.</span>
      {check.status === 'reported' && <p role="status" className="text-xs text-secondary">{description}</p>}
      {check.status === 'error' && <p role="alert" className="text-xs text-danger">{check.message} Readiness remains unknown.</p>}
    </div>
  );
}
