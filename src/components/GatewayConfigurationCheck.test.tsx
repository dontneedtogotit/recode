import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { GatewayConfigurationCheck } from './GatewayConfigurationCheck';

afterEach(() => { delete (globalThis as { __TAURI__?: unknown }).__TAURI__; });

describe('passive gateway configuration check', () => {
  it('only invokes the passive status command after an explicit click', async () => {
    const invoke = vi.fn().mockResolvedValue('Closed');
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    render(<GatewayConfigurationCheck />);
    expect(invoke).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Check gateway configuration' }));
    expect(await screen.findByRole('status')).toHaveTextContent('have not been tested');
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('get_gateway_status');
  });

  it('reports missing configuration without suggesting a successful connection', async () => {
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke: vi.fn().mockResolvedValue('Unconfigured') };
    render(<GatewayConfigurationCheck />);
    fireEvent.click(screen.getByRole('button', { name: 'Check gateway configuration' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Gateway is not configured');
  });

  it('rejects malformed native status instead of treating it as readiness', async () => {
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke: vi.fn().mockResolvedValue({ healthy: true }) };
    render(<GatewayConfigurationCheck />);
    fireEvent.click(screen.getByRole('button', { name: 'Check gateway configuration' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Invalid gateway configuration status');
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });
});
