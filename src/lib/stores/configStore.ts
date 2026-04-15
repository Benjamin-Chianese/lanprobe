import { invoke } from '@tauri-apps/api/core';

// Mini key/value API plugé sur `cmd_config_get` / `cmd_config_set`, qui
// rempace l'ancien stockage tauri-plugin-store côté desktop. Avantage :
// le même fichier `app_config.json` est géré par le backend Rust, et le
// serveur embarqué peut diffuser sur le bus un event `config:update`
// dès qu'un client change quelque chose — ce qui permet au client web
// et au desktop de rester en phase en temps réel.
//
// Côté web, `cmd_config_set` est volontairement bloqué par le dispatch
// HTTP : les modifs sont pilotées depuis le desktop, les clients web
// restent en lecture seule.

type Value = unknown;

let cache: Record<string, Value> = {};
let loaded = false;

async function ensureLoaded(): Promise<void> {
  if (loaded) return;
  try {
    const v = await invoke<Record<string, Value>>('cmd_config_get');
    cache = (v && typeof v === 'object' ? v : {}) as Record<string, Value>;
  } catch {
    cache = {};
  }
  loaded = true;
}

// Appelé par le listener `config:update` dans +layout.svelte quand le
// backend pousse une nouvelle version du blob. On remplace le cache
// en place et laisse les stores consommateurs se réhydrater (init()).
export function applyRemoteConfig(next: unknown): void {
  if (next && typeof next === 'object') {
    cache = next as Record<string, Value>;
    loaded = true;
  }
}

export interface ConfigStoreApi {
  get<T = unknown>(key: string): Promise<T | undefined>;
  set<T = unknown>(key: string, value: T): Promise<void>;
  save(): Promise<void>;
}

export async function getConfigStore(): Promise<ConfigStoreApi> {
  await ensureLoaded();
  return {
    async get<T>(key: string): Promise<T | undefined> {
      return cache[key] as T | undefined;
    },
    async set<T>(key: string, value: T): Promise<void> {
      cache[key] = value;
      try {
        await invoke('cmd_config_set', { value: cache });
      } catch (e) {
        // Côté web, les écritures sont bloquées — on garde la valeur
        // en cache pour la session mais elle ne sera pas persistée.
        console.warn('config.set failed', e);
      }
    },
    async save(): Promise<void> {
      // No-op : la persistance se fait dans `set` via cmd_config_set.
    },
  };
}
