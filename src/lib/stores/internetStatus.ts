import { writable } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export type InternetState = 'online' | 'limited' | 'offline';

export interface InternetTick {
  state: InternetState;
  icmp_ok: boolean;
  icmp_ms: number | null;
  http_ok: boolean;
  http_ms: number | null;
  dns_ok: boolean;
  dns_ms: number | null;
  dns_target: string;
  icmp_target: string;
  http_target: string;
  timestamp: number;
  uptime_pct: number;
  samples: number;
}

function createInternetStatus() {
  const { subscribe, set } = writable<InternetTick | null>(null);
  let initPromise: Promise<void> | null = null;
  let unlisten: UnlistenFn | null = null;

  async function init() {
    if (initPromise) return initPromise;
    initPromise = (async () => {
      // Snapshot immédiat au mount pour ne pas attendre le premier tick (5s).
      try {
        const snap = await invoke<InternetTick | null>('cmd_get_internet_status');
        if (snap) set(snap);
      } catch (e) {
        console.warn('[internetStatus] snapshot failed', e);
      }
      unlisten = await listen<InternetTick>('internet:tick', ({ payload }) => set(payload));
    })();
    return initPromise;
  }

  return {
    subscribe,
    init,
    _teardown: () => { unlisten?.(); unlisten = null; initPromise = null; },
  };
}

export const internetStatus = createInternetStatus();
