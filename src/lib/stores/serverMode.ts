import { getConfigStore } from './configStore';

export interface ServerModeConfig {
  enabled: boolean;
  host: string;
  port: number;
}

const STORE_KEY = 'server_mode';

const DEFAULTS: ServerModeConfig = {
  enabled: false,
  host: '0.0.0.0',
  port: 8443,
};

// En mode web (UI servie par lanprobe-server), tauri-plugin-store n'est
// pas dispo (c'est notre shim HTTP qui sert l'UI). Dans ce cas la
// persistance n'a pas de sens : on reste toujours sur les defaults.
function isWeb(): boolean {
  return typeof window !== 'undefined' && (window as any).__LANPROBE_WEB__ === true;
}

export async function loadServerMode(): Promise<ServerModeConfig> {
  if (isWeb()) return DEFAULTS;
  try {
    const store = await getConfigStore();
    const saved = await store.get<ServerModeConfig>(STORE_KEY);
    return saved ? { ...DEFAULTS, ...saved } : DEFAULTS;
  } catch {
    return DEFAULTS;
  }
}

export async function saveServerMode(cfg: ServerModeConfig): Promise<void> {
  if (isWeb()) return;
  const store = await getConfigStore();
  await store.set(STORE_KEY, cfg);
  await store.save();
}
