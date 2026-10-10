import {
  getGatewayStatus as getTypedGatewayStatus,
  runGatewayTask as runTypedGatewayTask,
  type GatewayRequest,
  type GatewayResponse,
} from '../lib/ipc';

export type { GatewayRequest, GatewayResponse } from '../lib/ipc';
export type GatewayStatus = { state: string };

export async function getGatewayStatus(): Promise<GatewayStatus> {
  return getTypedGatewayStatus();
}

export async function runGatewayTask(input: GatewayRequest): Promise<GatewayResponse> {
  return runTypedGatewayTask(input);
}
