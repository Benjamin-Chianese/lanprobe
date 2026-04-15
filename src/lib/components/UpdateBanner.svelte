<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';

  interface UpdateInfo {
    current_version: string;
    latest_version: string | null;
    has_update: boolean;
    asset_url: string | null;
    asset_name: string | null;
    platform_supported: boolean;
    release_notes_url: string | null;
  }

  let info = $state<UpdateInfo | null>(null);
  let dismissed = $state(false);
  let installing = $state(false);
  let error = $state<string | null>(null);

  const DISMISS_KEY = 'lanprobe.update.dismissed';

  onMount(async () => {
    try {
      const u = await invoke<UpdateInfo>('cmd_check_update');
      if (u?.has_update && u.latest_version) {
        const last = localStorage.getItem(DISMISS_KEY);
        if (last === u.latest_version) dismissed = true;
        info = u;
      }
    } catch {
      // silent
    }
  });

  function dismiss() {
    dismissed = true;
    if (info?.latest_version) {
      try { localStorage.setItem(DISMISS_KEY, info.latest_version); } catch {}
    }
  }

  async function install() {
    if (!info?.asset_url || !info?.asset_name) return;
    installing = true;
    error = null;
    try {
      await invoke<string>('cmd_apply_update', {
        url: info.asset_url,
        assetName: info.asset_name,
      });
      // L'installeur a été lancé — on ne quitte pas l'app nous-mêmes,
      // il fermera le process quand l'utilisateur confirmera.
    } catch (e) {
      error = String(e);
      installing = false;
    }
  }
</script>

{#if info && !dismissed}
  <div class="banner">
    <span class="icon" title={$_('banners.update_icon_title')}>↑</span>
    <span class="text">
      {$_('banners.update_available_prefix')} <strong>v{info.latest_version}</strong>
      {#if error} — <span class="err">{$_('banners.update_install_error')}: {error}</span>{/if}
    </span>
    {#if info.asset_url}
      <button class="install" onclick={install} disabled={installing}>
        {installing ? $_('banners.update_installing') : $_('banners.update_install')}
      </button>
    {:else if info.release_notes_url}
      <a class="link" href={info.release_notes_url} target="_blank" rel="noopener noreferrer">
        {$_('banners.update_view')}
      </a>
      <span class="hint">{$_('banners.update_unsupported')}</span>
    {/if}
    <button class="dismiss" onclick={dismiss} title={$_('banners.dismiss')} disabled={installing}>✕</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    background: var(--ep-accent);
    color: #fff;
    font-size: 13px;
    flex-shrink: 0;
  }
  .icon { font-size: 14px; }
  .text { flex: 1; }
  .err { opacity: 0.9; }
  .hint { font-size: 11px; opacity: 0.8; }
  .link { color: #fff; font-weight: 700; text-decoration: underline; cursor: pointer; white-space: nowrap; }
  .install {
    background: rgba(255,255,255,0.18);
    border: 1px solid rgba(255,255,255,0.4);
    color: #fff;
    font-weight: 600;
    padding: 4px 12px;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
  }
  .install:hover:not(:disabled) { background: rgba(255,255,255,0.28); }
  .install:disabled { opacity: 0.7; cursor: wait; }
  .dismiss { background: transparent; border: none; color: rgba(255,255,255,0.8); cursor: pointer; font-size: 14px; padding: 2px 6px; border-radius: 4px; }
  .dismiss:hover:not(:disabled) { background: rgba(255,255,255,0.2); }
</style>
