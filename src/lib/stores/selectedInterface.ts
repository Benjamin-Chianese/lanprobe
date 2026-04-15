import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Interface active choisie dans le Dashboard. Source unique de vérité pour
// toutes les opérations qui ciblent "l'interface courante" : appliquer un
// profil, ping, scan, speedtest — c'est cette valeur qui décide.
//
// Côté desktop, persistée dans localStorage et poussée au backend dès le
// boot. Côté web, la sélection est pilotée par le desktop (cmd_set_...
// est bloqué par le serveur) : on s'abonne à `interface:selected` pour
// rester miroir, et on hydrate l'état au démarrage depuis le backend.

const STORAGE_KEY = 'lanprobe.dashboard.interface';

function isWeb(): boolean {
  return typeof window !== 'undefined' && (window as any).__LANPROBE_WEB__ === true;
}

function readInitial(): string {
  try { return localStorage.getItem(STORAGE_KEY) ?? ''; } catch { return ''; }
}

function createStore() {
  const initial = readInitial();
  const { subscribe, set } = writable<string>(initial);
  let current = initial;
  let initPromise: Promise<void> | null = null;

  if (!isWeb() && initial) {
    invoke('cmd_set_selected_interface', { name: initial }).catch(() => {});
  }

  // Hydratation explicite via init() : le layout awaite cette promesse
  // avant de rendre les pages, ce qui évite qu'un composant comme
  // Profiles voit $selectedInterface='' au mount (désync web <-> desktop).
  async function init(): Promise<void> {
    if (initPromise) return initPromise;
    initPromise = (async () => {
      try {
        const backendName = await invoke<string | null>('cmd_get_selected_interface');
        if (backendName && backendName !== current) {
          current = backendName;
          set(backendName);
          try { localStorage.setItem(STORAGE_KEY, backendName); } catch {}
        }
      } catch {}
      try {
        await listen<{ name: string | null }>('interface:selected', ({ payload }) => {
          const next = payload?.name ?? '';
          if (next === current) return;
          current = next;
          set(next);
          try { localStorage.setItem(STORAGE_KEY, next); } catch {}
        });
      } catch {}
    })();
    return initPromise;
  }

  async function select(name: string) {
    const changed = name !== current;
    current = name;
    set(name);
    try { localStorage.setItem(STORAGE_KEY, name); } catch {}
    try { await invoke('cmd_set_selected_interface', { name: name || null }); } catch {}
    // Changer d'interface invalide l'historique du monitoring internet :
    // l'ancienne fenêtre d'uptime reflétait une autre NIC. On reset côté
    // backend pour repartir d'une base propre — évite d'afficher un mix
    // d'échantillons venant de deux chemins réseau différents.
    if (changed) {
      try { await invoke('cmd_reset_internet_monitor'); } catch {}
    }
  }

  return { subscribe, select, init };
}

export const selectedInterface = createStore();
