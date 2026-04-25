import { writable } from 'svelte/store';
import { getConfigStore } from './configStore';

export interface InfluxV1Config {
  database: string;
  username: string;
  password: string;
}

export interface InfluxV2Config {
  org: string;
  bucket: string;
  token: string;
}

export interface InfluxDbConfig {
  enabled: boolean;
  version: 'v1' | 'v2';
  url: string;
  instance_label: string;
  v1: InfluxV1Config;
  v2: InfluxV2Config;
}

const DEFAULT_INFLUX: InfluxDbConfig = {
  enabled: false,
  version: 'v2',
  url: '',
  instance_label: '',
  v1: { database: 'lanprobe', username: '', password: '' },
  v2: { org: '', bucket: 'lanprobe', token: '' },
};

function createInfluxStore() {
  const { subscribe, set, update } = writable<InfluxDbConfig>({ ...DEFAULT_INFLUX, v1: { ...DEFAULT_INFLUX.v1 }, v2: { ...DEFAULT_INFLUX.v2 } });

  async function init() {
    const store = await getConfigStore();
    const saved = await store.get<InfluxDbConfig>('influxdb');
    if (saved) {
      set({
        ...DEFAULT_INFLUX,
        ...saved,
        v1: { ...DEFAULT_INFLUX.v1, ...(saved.v1 ?? {}) },
        v2: { ...DEFAULT_INFLUX.v2, ...(saved.v2 ?? {}) },
      });
    }
  }

  async function save(cfg: InfluxDbConfig) {
    const store = await getConfigStore();
    await store.set('influxdb', cfg);
    await store.save();
    set({ ...cfg, v1: { ...cfg.v1 }, v2: { ...cfg.v2 } });
  }

  return { subscribe, init, save };
}

export const influxDb = createInfluxStore();
