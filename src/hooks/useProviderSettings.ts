import {
  deleteProviderSettings as deleteTypedProviderSettings,
  listProviderSettings as listTypedProviderSettings,
  saveProviderSettings as saveTypedProviderSettings,
  type ProviderSettings,
} from '../lib/ipc';

export type { ProviderSettings } from '../lib/ipc';

export async function listProviders(): Promise<ProviderSettings[]> {
  return listTypedProviderSettings();
}

export async function saveProvider(provider: ProviderSettings): Promise<void> {
  await saveTypedProviderSettings(provider);
}

export async function deleteProvider(id: string): Promise<void> {
  await deleteTypedProviderSettings(id);
}
