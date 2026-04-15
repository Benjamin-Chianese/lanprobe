<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';

  let hasPerms = $state(true); // optimiste, vérifié au mount
  let installing = $state(false);
  let done = $state(false);
  let error = $state('');

  onMount(async () => {
    hasPerms = await invoke<boolean>('cmd_check_permissions');
  });

  async function setup() {
    installing = true;
    error = '';
    try {
      await invoke('cmd_install_permissions');
      done = true;
      hasPerms = true;
    } catch (e) {
      // "Opération annulée" = l'utilisateur a fermé le prompt, pas une erreur critique
      if (String(e).includes('annulée') || String(e).includes('cancelled')) {
        done = true; // Cache la bannière sans marquer comme configuré
      } else {
        error = String(e);
      }
    }
    installing = false;
  }
</script>

{#if !hasPerms && !done}
  <div class="banner">
    <div class="banner-content">
      <span class="icon">🔐</span>
      <div class="text">
        <strong>{$_('banners.setup_title')}</strong>
        <span>{$_('banners.setup_desc')}</span>
      </div>
      {#if error}
        <span class="error">{error}</span>
      {/if}
      <button onclick={setup} disabled={installing}>
        {installing ? $_('banners.setup_installing') : $_('banners.setup_install')}
      </button>
      <button class="dismiss" onclick={() => { done = true; }} title={$_('banners.dismiss')}>✕</button>
    </div>
  </div>
{/if}

<style>
  .banner {
    background: color-mix(in srgb, var(--ep-accent) 12%, var(--ep-bg-secondary));
    border-bottom: 1px solid color-mix(in srgb, var(--ep-accent) 30%, var(--ep-border));
    padding: 10px 20px;
    flex-shrink: 0;
  }
  .banner-content {
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: 900px;
  }
  .icon { font-size: 18px; flex-shrink: 0; }
  .text { flex: 1; display: flex; gap: 8px; align-items: baseline; font-size: 13px; flex-wrap: wrap; }
  .text strong { font-weight: 700; white-space: nowrap; }
  .text span { color: var(--ep-text-secondary); }
  .error { color: var(--ep-danger); font-size: 12px; }
  button {
    padding: 6px 14px;
    border-radius: 6px;
    border: none;
    background: var(--ep-accent);
    color: #fff;
    cursor: pointer;
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
    flex-shrink: 0;
  }
  button:disabled { opacity: 0.6; cursor: not-allowed; }
  button.dismiss {
    background: transparent;
    color: var(--ep-text-muted);
    padding: 4px 8px;
    font-size: 14px;
  }
</style>
