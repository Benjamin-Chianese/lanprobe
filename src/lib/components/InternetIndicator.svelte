<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { _ } from 'svelte-i18n';
  import { internetStatus, type InternetState } from '../stores/internetStatus';

  const { variant = 'chip', onClick }: { variant?: 'chip' | 'block'; onClick?: () => void } = $props();

  let showPopover = $state(false);
  interface PublicIpInfo { ip: string; country: string | null; city: string | null; isp: string | null; }
  let publicIp = $state<PublicIpInfo | null>(null);

  onMount(() => {
    internetStatus.init();
    loadPublicIp();
  });

  async function loadPublicIp() {
    try { publicIp = await invoke<PublicIpInfo>('cmd_get_public_ip'); }
    catch { /* offline — leave null */ }
  }

  const stateClass = $derived<InternetState>($internetStatus?.state ?? 'offline');
  $effect(() => {
    if (stateClass === 'online' && !publicIp) loadPublicIp();
  });
  const icmpMs = $derived($internetStatus?.icmp_ms ?? null);
  const httpMs = $derived($internetStatus?.http_ms ?? null);
  const icmpOk = $derived($internetStatus?.icmp_ok ?? false);
  const httpOk = $derived($internetStatus?.http_ok ?? false);
  const uptime = $derived($internetStatus?.uptime_pct ?? 0);
  const ageSec = $derived.by(() => {
    const ts = $internetStatus?.timestamp;
    if (!ts) return null;
    return Math.max(0, Math.floor(Date.now() / 1000 - ts));
  });

  function label(): string {
    if (!$internetStatus) return '—';
    if (stateClass === 'online' && icmpMs != null) return `${icmpMs}ms`;
    if (stateClass === 'online' && httpMs != null) return `${httpMs}ms`;
    return $_(`topbar.internet.${stateClass}`);
  }
</script>

<div class="ii-wrap">
  <button
    class="ii ii-{variant} state-{stateClass}"
    onmouseenter={() => (showPopover = true)}
    onmouseleave={() => (showPopover = false)}
    onfocus={() => (showPopover = true)}
    onblur={() => (showPopover = false)}
    onkeydown={(event) => { if (event.key === 'Escape') showPopover = false; }}
    onclick={onClick}
    type="button"
    title={$_('topbar.internet.status_title')}
    aria-describedby="ii-tooltip"
  >
    <span class="dot"></span>
    <span class="icon">🌐</span>
    <span class="lbl">{label()}</span>
  </button>

  {#if showPopover && $internetStatus}
    <div class="popover pop-{variant}" role="tooltip" id="ii-tooltip">
      <div class="pop-title">{$_('topbar.internet.status_title')}</div>
      <div class="row">
        <span class="l">{$_('topbar.internet.icmp')}</span>
        <span class="t">{$internetStatus.icmp_target}</span>
        <span class="v {icmpOk ? 'ok' : stateClass === 'online' ? 'na' : 'ko'}">
          {icmpOk && icmpMs != null ? `✓ ${icmpMs}ms` : stateClass === 'online' ? '— n/a' : '✕ timeout'}
        </span>
      </div>
      <div class="row">
        <span class="l">{$_('topbar.internet.http')}</span>
        <span class="t">{$internetStatus.http_target.replace(/^https?:\/\//, '')}</span>
        <span class="v {httpOk ? 'ok' : 'ko'}">
          {httpOk && httpMs != null ? `✓ ${httpMs}ms` : '✕ timeout'}
        </span>
      </div>
      {#if publicIp}
        <div class="row">
          <span class="l">{$_('topbar.internet.public_ip')}</span>
          <span class="t">{[publicIp.city, publicIp.country].filter(Boolean).join(', ') || publicIp.isp || ''}</span>
          <span class="v ok">{publicIp.ip}</span>
        </div>
      {/if}
      <div class="foot">
        <span>{$_('topbar.internet.uptime')} {uptime.toFixed(2)}%</span>
        {#if ageSec != null}
          <span>{$_('topbar.internet.last_tick', { values: { n: ageSec } })}</span>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .ii-wrap {
    position: relative;
    display: inline-block;
  }
  .ii {
    display: inline-flex; align-items: center; gap: 7px;
    border: 1px solid var(--ep-glass-border);
    border-radius: 7px;
    background: var(--ep-bg-secondary);
    color: var(--ep-text-primary);
    font-family: var(--ep-font-mono);
    font-weight: 600;
    cursor: pointer;
  }
  .ii-chip { padding: 5px 11px; font-size: 12px; height: 32px; }
  .ii-block { padding: 8px 4px; flex-direction: column; gap: 2px; width: 44px; font-size: 10px; border-radius: 8px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; }
  .state-online  { color: var(--ep-success); background: color-mix(in srgb, var(--ep-success) 10%, transparent); border-color: color-mix(in srgb, var(--ep-success) 30%, transparent); }
  .state-online .dot { background: var(--ep-success); box-shadow: 0 0 6px var(--ep-success); }
  .state-limited { color: var(--ep-warning); background: color-mix(in srgb, var(--ep-warning) 10%, transparent); border-color: color-mix(in srgb, var(--ep-warning) 30%, transparent); }
  .state-limited .dot { background: var(--ep-warning); box-shadow: 0 0 6px var(--ep-warning); }
  .state-offline { color: var(--ep-danger); background: color-mix(in srgb, var(--ep-danger) 10%, transparent); border-color: color-mix(in srgb, var(--ep-danger) 30%, transparent); }
  .state-offline .dot { background: var(--ep-danger); box-shadow: 0 0 6px var(--ep-danger); }

  .popover {
    position: absolute;
    min-width: 280px;
    background: var(--ep-bg-secondary);
    border: 1px solid var(--ep-glass-border);
    border-radius: 10px;
    padding: 12px 14px;
    box-shadow: 0 20px 40px rgba(0,0,0,0.5);
    z-index: 100;
    text-align: left;
  }
  /* chip (topbar) : popover en bas à droite du bouton */
  .pop-chip { top: calc(100% + 8px); right: 0; }
  /* block (sidebar) : popover à droite du bouton, aligné en bas pour rester
     visible en bas de sidebar sans être coupé par le bord inférieur de la fenêtre */
  .pop-block { left: calc(100% + 10px); bottom: 0; }
  .pop-title { font-size: 10px; text-transform: uppercase; letter-spacing: 1px; color: var(--ep-text-muted); margin-bottom: 8px; font-weight: 700; }
  .row { display: flex; align-items: center; gap: 10px; padding: 6px 0; border-bottom: 1px solid var(--ep-glass-border); font-size: 12px; }
  .row:last-of-type { border-bottom: none; }
  .l { color: var(--ep-text-secondary); font-weight: 600; min-width: 42px; }
  .t { color: var(--ep-text-muted); font-size: 10px; flex: 1; }
  .v { font-weight: 700; }
  .v.ok { color: var(--ep-success); }
  .v.ko { color: var(--ep-danger); }
  .v.na { color: var(--ep-text-muted); font-weight: 400; }
  .foot { margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--ep-glass-border); font-size: 10px; color: var(--ep-text-muted); display: flex; justify-content: space-between; }
</style>
