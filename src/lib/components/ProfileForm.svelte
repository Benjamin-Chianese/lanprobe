<script lang="ts">
  import type { Profile } from '../stores/profiles';
  import { api } from '../tauri';
  import { _ } from 'svelte-i18n';
  import { selectedInterface } from '../stores/selectedInterface';

  const { profile, onSave, onCancel } = $props<{
    profile?: Partial<Profile>;
    onSave: (p: Profile) => void;
    onCancel: () => void;
  }>();

  // Le profil s'applique toujours à l'interface active du Dashboard.
  // On ne stocke plus d'interface dans le profil — il est portable.
  let form = $state<Profile>({
    id: profile?.id ?? crypto.randomUUID(),
    name: profile?.name ?? '',
    ip: profile?.ip ?? '',
    subnet: profile?.subnet ?? '255.255.255.0',
    gateway: profile?.gateway ?? '',
    dns_primary: profile?.dns_primary ?? '8.8.8.8',
    dns_secondary: profile?.dns_secondary ?? '8.8.4.4',
    monitor_ips: profile?.monitor_ips ?? [],
  });

  // Édition textuelle : une IP par ligne. Parsé au save pour retomber
  // sur un string[] propre (trim + filter vide).
  let monitorIpsText = $state((profile?.monitor_ips ?? []).join('\n'));

  function save() {
    form.monitor_ips = monitorIpsText
      .split('\n')
      .map((s: string) => s.trim())
      .filter((s: string) => s.length > 0);
    onSave(form);
  }

  async function autoDetect() {
    if (!$selectedInterface) return;
    const details = await api.getInterfaceDetails($selectedInterface);
    if (details.ip) form.ip = details.ip;
    if (details.subnet) form.subnet = details.subnet;
    if (details.gateway) form.gateway = details.gateway;
    if (details.dns.length > 0) form.dns_primary = details.dns[0];
    if (details.dns.length > 1) form.dns_secondary = details.dns[1];
  }
</script>

<div class="overlay">
  <div class="card">
    <h2>{profile?.id ? $_('profiles.form.edit_title') : $_('profiles.form.new_title')}</h2>
    <div class="hint">
      {$_('profiles.form.applied_to')}
      {#if $selectedInterface}<span class="iface-name">({$selectedInterface})</span>{/if}
    </div>
    <label>{$_('profiles.form.name')}<input bind:value={form.name} placeholder={$_('profiles.form.name_placeholder')} /></label>
    <label>{$_('profiles.form.ip')}<input bind:value={form.ip} placeholder={$_('profiles.form.ip_placeholder')} /></label>
    <label>{$_('profiles.form.mask')}<input bind:value={form.subnet} placeholder={$_('profiles.form.mask_placeholder')} /></label>
    <label>{$_('profiles.form.gateway')} <span class="opt">{$_('profiles.form.gateway_optional')}</span><input bind:value={form.gateway} placeholder={$_('profiles.form.gateway_placeholder')} /></label>
    <label>{$_('profiles.form.dns_primary')} <span class="opt">{$_('profiles.form.dns_optional')}</span><input bind:value={form.dns_primary} placeholder={$_('profiles.form.dns_primary_placeholder')} /></label>
    <label>{$_('profiles.form.dns_secondary')} <span class="opt">{$_('profiles.form.dns_optional')}</span><input bind:value={form.dns_secondary} placeholder={$_('profiles.form.dns_secondary_placeholder')} /></label>
    <label>
      {$_('profiles.form.monitor_ips')}
      <span class="opt">{$_('profiles.form.monitor_ips_hint')}</span>
      <textarea
        bind:value={monitorIpsText}
        placeholder={$_('profiles.form.monitor_ips_placeholder')}
        rows="3"
      ></textarea>
    </label>
    <div class="actions">
      <button type="button" class="detect-btn" onclick={autoDetect} title={$_('profiles.form.detect_title')}>{$_('profiles.form.detect')}</button>
      <span class="spacer"></span>
      <button onclick={onCancel}>{$_('profiles.form.cancel')}</button>
      <button class="primary" onclick={save}>{$_('profiles.form.save')}</button>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.5); display: flex; align-items: center; justify-content: center; z-index: 100; }
  .card { background: var(--ep-bg-secondary); border: 1px solid var(--ep-border); border-radius: 12px; padding: 24px; width: 380px; display: flex; flex-direction: column; gap: 12px; }
  h2 { font-size: 16px; font-weight: 700; margin-bottom: 4px; }
  .hint { font-size: 12px; color: var(--ep-text-muted); padding: 8px 10px; background: var(--ep-bg-tertiary); border: 1px solid var(--ep-border); border-radius: 6px; }
  .iface-name { font-family: var(--ep-font-mono); color: var(--ep-accent); font-weight: 600; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--ep-text-secondary); }
  .opt { color: var(--ep-text-muted); font-weight: 400; font-size: 11px; }
  input, textarea { background: var(--ep-bg-tertiary); border: 1px solid var(--ep-border); border-radius: 6px; padding: 7px 10px; color: var(--ep-text-primary); font-family: var(--ep-font-mono); font-size: 13px; }
  textarea { resize: vertical; min-height: 52px; }
  .actions { display: flex; gap: 8px; align-items: center; margin-top: 8px; }
  .spacer { flex: 1; }
  .detect-btn { padding: 7px 12px; border-radius: 6px; border: 1px solid var(--ep-accent); background: transparent; color: var(--ep-accent); cursor: pointer; font-size: 12px; font-weight: 600; }
  .detect-btn:hover { background: var(--ep-accent); color: #fff; }
  button { padding: 7px 16px; border-radius: 6px; border: 1px solid var(--ep-border); background: var(--ep-bg-tertiary); color: var(--ep-text-primary); cursor: pointer; font-size: 13px; font-weight: 600; }
  button.primary { background: var(--ep-accent); border-color: var(--ep-accent); color: #fff; }
</style>
