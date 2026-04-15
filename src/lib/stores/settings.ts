import { writable } from 'svelte/store';
import { setLang as applyLang, type Lang } from '$lib/i18n';
import { getConfigStore } from './configStore';

export type Theme = 'system' | 'dark' | 'light';
export type Layout = 'sidebar' | 'single' | 'grouped';
export type SpeedtestEngine = 'ookla' | 'iperf3';
export type Palette = 'indigo' | 'cyan' | 'emerald' | 'rose' | 'amber' | 'slate';
export type { Lang };

interface SettingsState {
  theme: Theme;
  palette: Palette;
  dashboardRefreshSec: number;
  layout: Layout;
  lang: Lang;
  autoPortScanProfileId: string | null;
  speedtestEngine: SpeedtestEngine;
  iperfServer: string;
}

const DEFAULT: SettingsState = {
  theme: 'system',
  palette: 'indigo',
  dashboardRefreshSec: 0,
  layout: 'sidebar',
  lang: 'en',
  autoPortScanProfileId: null,
  speedtestEngine: 'ookla',
  iperfServer: '',
};

function createSettingsStore() {
  const { subscribe, set, update } = writable<SettingsState>({ ...DEFAULT });

  async function init() {
    const store = await getConfigStore();
    const theme = (await store.get<Theme>('theme')) ?? 'system';
    const dashboardRefreshSec = (await store.get<number>('dashboardRefreshSec')) ?? 0;
    const layout = (await store.get<Layout>('layout')) ?? 'sidebar';
    const lang = (await store.get<Lang>('lang')) ?? 'en';
    const autoPortScanProfileId = (await store.get<string | null>('autoPortScanProfileId')) ?? null;
    const speedtestEngine = (await store.get<SpeedtestEngine>('speedtestEngine')) ?? 'ookla';
    const iperfServer = (await store.get<string>('iperfServer')) ?? '';
    const palette = (await store.get<Palette>('palette')) ?? 'indigo';
    set({ theme, dashboardRefreshSec, layout, lang, autoPortScanProfileId, speedtestEngine, iperfServer, palette });
    applyTheme(theme);
    applyLang(lang);
    applyPalette(palette);
  }

  async function setSpeedtestEngine(engine: SpeedtestEngine) {
    const store = await getConfigStore();
    await store.set('speedtestEngine', engine);
    await store.save();
    update(s => ({ ...s, speedtestEngine: engine }));
  }

  async function setIperfServer(server: string) {
    const store = await getConfigStore();
    await store.set('iperfServer', server);
    await store.save();
    update(s => ({ ...s, iperfServer: server }));
  }

  async function setAutoPortScanProfile(id: string | null) {
    const store = await getConfigStore();
    await store.set('autoPortScanProfileId', id);
    await store.save();
    update(s => ({ ...s, autoPortScanProfileId: id }));
  }

  async function setTheme(theme: Theme) {
    const store = await getConfigStore();
    await store.set('theme', theme);
    await store.save();
    update(s => ({ ...s, theme }));
    applyTheme(theme);
  }

  async function setDashboardRefresh(sec: number) {
    const store = await getConfigStore();
    await store.set('dashboardRefreshSec', sec);
    await store.save();
    update(s => ({ ...s, dashboardRefreshSec: sec }));
  }

  async function setLayout(layout: Layout) {
    const store = await getConfigStore();
    await store.set('layout', layout);
    await store.save();
    update(s => ({ ...s, layout }));
  }

  async function setLanguage(lang: Lang) {
    const store = await getConfigStore();
    await store.set('lang', lang);
    await store.save();
    update(s => ({ ...s, lang }));
    applyLang(lang);
  }

  async function setPalette(palette: Palette) {
    const store = await getConfigStore();
    await store.set('palette', palette);
    await store.save();
    update(s => ({ ...s, palette }));
    applyPalette(palette);
  }

  return { subscribe, init, setTheme, setDashboardRefresh, setLayout, setLanguage, setAutoPortScanProfile, setSpeedtestEngine, setIperfServer, setPalette };
}

export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  root.removeAttribute('data-theme');
  if (theme !== 'system') root.setAttribute('data-theme', theme);
}

export function applyPalette(palette: Palette) {
  document.documentElement.setAttribute('data-palette', palette);
}

export const settings = createSettingsStore();
