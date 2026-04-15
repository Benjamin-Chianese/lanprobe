import { writable, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { portscanProfiles, BUILTIN_PROFILES } from './portscanProfiles';

export interface PortResult {
  port: number;
  service: string;
  proto: 'tcp' | 'udp';
  open: boolean;
}

export interface ScanEntry {
  ip: string;
  scanning: boolean;
  tcpResults: PortResult[];
  udpResults: PortResult[];
  error: string | null;
  expanded: boolean;
  profileId: string | null;
  profileName: string | null;
  scannedAt: number | null;
}

interface BackendPortScanEntry {
  ip: string;
  tcp: PortResult[];
  udp: PortResult[];
  timestamp: number;
  profile_id: string | null;
  in_progress: boolean;
}

function createPortScanStore() {
  const { subscribe, update } = writable<Map<string, ScanEntry>>(new Map());

  let initPromise: Promise<void> | null = null;

  function resolveProfileName(id: string | null): string | null {
    if (!id) return null;
    const local = get(portscanProfiles).find(p => p.id === id);
    if (local) return local.name;
    const builtin = BUILTIN_PROFILES.find(p => p.id === id);
    return builtin?.name ?? null;
  }

  function mergeBackendEntry(b: BackendPortScanEntry) {
    update(map => {
      const existing = map.get(b.ip);
      const profileId = b.profile_id ?? existing?.profileId ?? null;
      // On résout le nom de profil localement à partir de l'ID transmis
      // par le backend : les profils custom vivent dans le store frontend
      // mirroré via config:update, donc le PC qui n'a pas lancé le scan
      // peut quand même afficher le bon tag.
      const profileName = existing?.profileName ?? resolveProfileName(profileId);
      map.set(b.ip, {
        ip: b.ip,
        scanning: b.in_progress,
        tcpResults: b.tcp ?? existing?.tcpResults ?? [],
        udpResults: b.udp ?? existing?.udpResults ?? [],
        error: null,
        expanded: existing?.expanded ?? false,
        profileId,
        profileName,
        scannedAt: b.timestamp ? b.timestamp * 1000 : existing?.scannedAt ?? null,
      });
      return new Map(map);
    });
  }

  async function init(): Promise<void> {
    if (initPromise) return initPromise;
    initPromise = (async () => {
      try {
        const snap = await invoke<BackendPortScanEntry[]>('cmd_get_portscan_snapshot');
        if (Array.isArray(snap)) for (const e of snap) mergeBackendEntry(e);
      } catch {}
      try {
        await listen<BackendPortScanEntry>('portscan:update', ({ payload }) => {
          if (payload && payload.ip) mergeBackendEntry(payload);
        });
        await listen<{ ip: string }>('portscan:removed', ({ payload }) => {
          if (payload?.ip) update(map => { map.delete(payload.ip); return new Map(map); });
        });
      } catch {}
    })();
    return initPromise;
  }

  // Ajoute une IP au store (si pas déjà présente) et kicke un scan en
  // background. Appelable depuis n'importe où — notamment le clic-droit
  // dans Discovery — sans forcer de navigation.
  async function add(
    ip: string,
    tcpPorts?: number[],
    udpPorts?: number[],
    profileId?: string | null,
    profileName?: string | null,
  ) {
    ip = ip.trim();
    if (!ip) return;
    update(map => {
      const base = map.get(ip);
      map.set(ip, {
        ip,
        scanning: true,
        tcpResults: base?.tcpResults ?? [],
        udpResults: base?.udpResults ?? [],
        error: null,
        // Accordéon fermé par défaut : quand l'auto-scan Discovery pousse
        // 50 hôtes d'un coup, on ne veut pas tout déplier — l'utilisateur
        // ouvre ce qu'il veut voir.
        expanded: base?.expanded ?? false,
        profileId: profileId ?? null,
        profileName: profileName ?? null,
        scannedAt: base?.scannedAt ?? null,
      });
      return new Map(map);
    });

    try {
      const [tcp, udp] = await Promise.all([
        invoke<PortResult[]>('cmd_scan_ports', { ip, ports: tcpPorts ?? null, profileId: profileId ?? null }),
        invoke<PortResult[]>('cmd_scan_udp_ports', { ip, ports: udpPorts ?? null }),
      ]);
      update(map => {
        const e = map.get(ip);
        if (!e) return map;
        e.tcpResults = tcp;
        e.udpResults = udp;
        e.scanning = false;
        e.scannedAt = Date.now();
        map.set(ip, e);
        return new Map(map);
      });
    } catch (err) {
      update(map => {
        const e = map.get(ip);
        if (!e) return map;
        e.error = String(err);
        e.scanning = false;
        map.set(ip, e);
        return new Map(map);
      });
    }
  }

  function remove(ip: string) {
    update(map => { map.delete(ip); return new Map(map); });
    invoke('cmd_clear_portscan_entry', { ip }).catch(() => {});
  }

  function toggle(ip: string) {
    update(map => {
      const e = map.get(ip);
      if (!e) return map;
      e.expanded = !e.expanded;
      map.set(ip, e);
      return new Map(map);
    });
  }

  return { subscribe, init, add, remove, toggle };
}

export const portscan = createPortScanStore();
