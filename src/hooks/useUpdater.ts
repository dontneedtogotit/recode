import {
  checkUpdates as checkTypedUpdates,
  verifyUpdateSignature as verifyTypedUpdateSignature,
  type UpdateCheckResult,
} from '../lib/ipc';

export type { UpdateCheckResult } from '../lib/ipc';

export async function checkUpdates(endpoint: string, currentVersion: string): Promise<UpdateCheckResult> {
  try {
    return await checkTypedUpdates(endpoint, currentVersion);
  } catch {
    return { available: false };
  }
}

export async function verifyUpdateSignature(
  artifactPath: string,
  signaturePath: string,
  publicKeyPath: string,
): Promise<boolean> {
  try {
    return await verifyTypedUpdateSignature(artifactPath, signaturePath, publicKeyPath);
  } catch {
    return false;
  }
}
