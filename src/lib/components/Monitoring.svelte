<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { _ } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import { monitoring, type HostMonitor, type PingEntry } from '../stores/monitoring';
  import { internetStatus } from '../stores/internetStatus';
  import { settings } from '../stores/settings';
  import { selectedInterface } from '../stores/selectedInterface';
  import ExcelJS from 'exceljs';

  const isSingle = $derived($settings.layout === 'single');

  let newIp = $state('');
  let adding = $state(false);

  // ── SLA Export ──────────────────────────────────────────────────────────
  let showExport = $state(false);
  let exportSelected = $state<Set<string>>(new Set());
  let exportIncludeInternet = $state(true);
  let exporting = $state(false);

  function openExport() {
    exportSelected = new Set([...$monitoring.keys()]);
    exportIncludeInternet = true;
    showExport = true;
  }

  function toggleExportIp(ip: string) {
    const s = new Set(exportSelected);
    if (s.has(ip)) s.delete(ip); else s.add(ip);
    exportSelected = s;
  }

  function exportSelectAll() {
    exportSelected = new Set([...$monitoring.keys()]);
    exportIncludeInternet = true;
  }

  function exportDeselectAll() {
    exportSelected = new Set();
    exportIncludeInternet = false;
  }

  type Sample = { alive: boolean; latency_ms: number | null; timestamp: number };

  interface Outage {
    start: number;   // unix seconds
    end: number | null;
    samples_lost: number;
  }

  function computeOutages(samples: Sample[]): Outage[] {
    const out: Outage[] = [];
    let start: number | null = null;
    let lost = 0;
    for (const s of samples) {
      if (!s.alive) {
        if (start === null) { start = s.timestamp; lost = 0; }
        lost++;
      } else if (start !== null) {
        out.push({ start, end: s.timestamp, samples_lost: lost });
        start = null; lost = 0;
      }
    }
    if (start !== null) out.push({ start, end: null, samples_lost: lost });
    return out;
  }

  function localDt(ts: number): string {
    return new Intl.DateTimeFormat(undefined, {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false
    }).format(new Date(ts * 1000));
  }

  const CHART_COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#8b5cf6', '#06b6d4', '#f97316'];

  function renderChartPng(sources: { label: string; samples: Sample[] }[], nowTs: number): string {
    const W = 900, H = 420;
    const PAD = { top: 50, right: 30, bottom: 55, left: 65 };
    const PW = W - PAD.left - PAD.right;
    const PH = H - PAD.top - PAD.bottom;

    const canvas = document.createElement('canvas');
    canvas.width = W; canvas.height = H;
    const ctx = canvas.getContext('2d')!;

    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, W, H);

    const allSamples = sources.flatMap(s => s.samples);
    if (allSamples.length === 0) {
      ctx.fillStyle = '#6b7280';
      ctx.font = '14px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('Aucune donnée', W / 2, H / 2);
      return canvas.toDataURL('image/png').split(',')[1];
    }

    const minTs = Math.min(...allSamples.map(s => s.timestamp));
    const maxTs = Math.max(...allSamples.map(s => s.timestamp), minTs + 1);
    const tsRange = maxTs - minTs;
    const allLats = allSamples.filter(s => s.latency_ms != null).map(s => s.latency_ms as number);
    const maxLat = allLats.length ? Math.max(...allLats) * 1.15 : 100;

    const toX = (ts: number) => PAD.left + ((ts - minTs) / tsRange) * PW;
    const toY = (ms: number) => PAD.top + PH - (ms / maxLat) * PH;

    // grid horizontales
    ctx.lineWidth = 1;
    for (let i = 0; i <= 5; i++) {
      const ms = (maxLat / 5) * i;
      const y = toY(ms);
      ctx.strokeStyle = i === 0 ? '#9ca3af' : '#e5e7eb';
      ctx.beginPath(); ctx.moveTo(PAD.left, y); ctx.lineTo(PAD.left + PW, y); ctx.stroke();
      ctx.fillStyle = '#6b7280'; ctx.font = '11px sans-serif'; ctx.textAlign = 'right';
      ctx.fillText(Math.round(ms) + ' ms', PAD.left - 8, y + 4);
    }

    // grid verticales + labels X
    for (let i = 0; i <= 6; i++) {
      const ts = minTs + tsRange * (i / 6);
      const x = toX(ts);
      ctx.strokeStyle = '#e5e7eb'; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(x, PAD.top); ctx.lineTo(x, PAD.top + PH); ctx.stroke();
      ctx.fillStyle = '#6b7280'; ctx.font = '11px sans-serif'; ctx.textAlign = 'center';
      const lbl = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
        .format(new Date(ts * 1000));
      ctx.fillText(lbl, x, PAD.top + PH + 20);
    }

    // zones coupures
    for (const { samples } of sources) {
      for (const o of computeOutages(samples)) {
        const x1 = toX(o.start);
        const x2 = toX(o.end ?? nowTs);
        ctx.fillStyle = 'rgba(239,68,68,0.12)';
        ctx.fillRect(x1, PAD.top, Math.max(x2 - x1, 2), PH);
      }
    }

    // courbes latence
    for (let si = 0; si < sources.length; si++) {
      const color = CHART_COLORS[si % CHART_COLORS.length];
      const pts = sources[si].samples.filter(s => s.alive && s.latency_ms != null);
      if (pts.length < 2) continue;
      ctx.strokeStyle = color; ctx.lineWidth = 1.5; ctx.lineJoin = 'round';
      ctx.beginPath();
      pts.forEach((s, i) => {
        const x = toX(s.timestamp), y = toY(s.latency_ms as number);
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      });
      ctx.stroke();
    }

    // bordure zone graphique
    ctx.strokeStyle = '#d1d5db'; ctx.lineWidth = 1;
    ctx.strokeRect(PAD.left, PAD.top, PW, PH);

    // titre
    ctx.fillStyle = '#111827'; ctx.font = 'bold 13px sans-serif'; ctx.textAlign = 'center';
    ctx.fillText('Latence (ms) — ' + localDt(minTs) + ' → ' + localDt(maxTs), W / 2, 22);

    // légende
    let lx = PAD.left;
    for (let si = 0; si < sources.length; si++) {
      const color = CHART_COLORS[si % CHART_COLORS.length];
      ctx.fillStyle = color; ctx.fillRect(lx, H - 20, 14, 10);
      ctx.fillStyle = '#374151'; ctx.font = '11px sans-serif'; ctx.textAlign = 'left';
      ctx.fillText(sources[si].label, lx + 18, H - 11);
      lx += 20 + ctx.measureText(sources[si].label).width + 18;
    }
    // légende zone rouge
    ctx.fillStyle = 'rgba(239,68,68,0.35)'; ctx.fillRect(lx, H - 20, 14, 10);
    ctx.fillStyle = '#374151'; ctx.font = '11px sans-serif'; ctx.textAlign = 'left';
    ctx.fillText('Coupure', lx + 18, H - 11);

    return canvas.toDataURL('image/png').split(',')[1];
  }

  function statsForSource(samples: Sample[], nowTs: number) {
    const total = samples.length;
    const failed = samples.filter(s => !s.alive).length;
    const up = total === 0 ? 100 : ((total - failed) / total) * 100;
    const lats = samples.filter(s => s.alive && s.latency_ms != null)
      .map(s => s.latency_ms as number).sort((a, b) => a - b);
    const avg = lats.length ? lats.reduce((a, b) => a + b, 0) / lats.length : null;
    const min = lats.length ? lats[0] : null;
    const max = lats.length ? lats[lats.length - 1] : null;
    const p95 = lats.length ? lats[Math.min(lats.length - 1, Math.ceil(lats.length * 0.95) - 1)] : null;
    const outages = computeOutages(samples);
    const longest = outages.reduce((acc, o) => {
      const dur = o.end != null ? o.end - o.start : nowTs - o.start;
      return Math.max(acc, dur);
    }, 0);
    const firstTs = samples.length ? localDt(samples[0].timestamp) : '—';
    return { total, failed, up, avg, min, max, p95, outages, longest, firstTs };
  }

  function addChartSheet(wb: ExcelJS.Workbook, sheetName: string, sources: { label: string; samples: Sample[] }[], nowTs: number) {
    const ws = wb.addWorksheet(sheetName);
    const pngB64 = renderChartPng(sources, nowTs);
    const imgId = wb.addImage({ base64: pngB64, extension: 'png' });
    ws.addImage(imgId, { tl: { col: 0, row: 0 } as any, ext: { width: 900, height: 420 } });
  }

  async function doExportXlsx() {
    if (exporting) return;
    exporting = true;
    try {
      const snap = await invoke<Record<string, Sample[]>>('cmd_get_monitoring_snapshot');
      const now = new Date();
      const nowTs = Math.floor(now.getTime() / 1000);
      const slug = now.toISOString().replace(/[:.]/g, '-').slice(0, 19);
      const iface = $selectedInterface || '—';
      const t = (k: string) => get(_)(k) as string;

      // extraSeries : séries supplémentaires affichées dans le graphique et le
      // tableau de la feuille détaillée, mais pas utilisées pour les stats SLA.
      type SourceData = { label: string; samples: Sample[]; extraSeries?: { label: string; samples: Sample[] }[] };
      const sources: SourceData[] = [];

      if (exportIncludeInternet) {
        const total = $internetStatus?.samples ?? internetHistory.length;
        const upPct = $internetStatus?.uptime_pct ?? 100;
        const failedCount = Math.round(total * (1 - upPct / 100));
        const fakeSamples: Sample[] = internetHistory.map((ms, i) => ({
          alive: true, latency_ms: ms,
          timestamp: nowTs - (internetHistory.length - i)
        }));
        if (failedCount > 0 && fakeSamples.length > 0) {
          const step = Math.max(1, Math.floor(fakeSamples.length / failedCount));
          for (let i = 0; i < fakeSamples.length; i += step) {
            fakeSamples[i] = { ...fakeSamples[i], alive: false, latency_ms: null };
          }
        }
        // DNS comme série supplémentaire — fusionné avec Ping dans une seule feuille
        const dnsSamples: Sample[] = internetDnsHistory.map((dns, i) => ({
          alive: dns != null,
          latency_ms: dns,
          timestamp: nowTs - (internetDnsHistory.length - i)
        }));
        const extra = dnsSamples.some(s => s.alive)
          ? [{ label: 'DNS', samples: dnsSamples }]
          : undefined;
        sources.push({ label: t('monitoring.internet'), samples: fakeSamples, extraSeries: extra });
      }
      for (const ip of exportSelected) {
        sources.push({ label: ip, samples: snap?.[ip] ?? [] });
      }

      const wb = new ExcelJS.Workbook();

      // ── Feuille Résumé global ─────────────────────────────────────────────
      const ws1 = wb.addWorksheet(t('monitoring.xlsx_sheet_summary'));
      ws1.addRow(['LanProbe — ' + t('monitoring.export_modal_title')]);
      ws1.addRow([t('monitoring.xlsx_exported'), localDt(nowTs)]);
      ws1.addRow([t('monitoring.xlsx_interface'), iface]);
      ws1.addRow([]);
      ws1.addRow([
        t('monitoring.xlsx_col_source'),
        t('monitoring.xlsx_col_since'),
        t('monitoring.xlsx_col_uptime'),
        t('monitoring.xlsx_col_avg'),
        t('monitoring.xlsx_col_min'),
        t('monitoring.xlsx_col_max'),
        t('monitoring.xlsx_col_p95'),
        t('monitoring.xlsx_col_samples'),
        t('monitoring.xlsx_col_failures'),
        t('monitoring.xlsx_col_outages'),
        t('monitoring.xlsx_col_worst'),
      ]);
      for (const { label, samples } of sources) {
        const s = statsForSource(samples, nowTs);
        ws1.addRow([label, s.firstTs, +s.up.toFixed(2),
          s.avg != null ? +s.avg.toFixed(1) : '',
          s.min ?? '', s.max ?? '', s.p95 ?? '',
          s.total, s.failed, s.outages.length, s.longest]);
      }
      ws1.columns = [18, 24, 10, 10, 10, 10, 10, 13, 8, 10, 16].map(w => ({ width: w }));

      // ── Graphique global ──────────────────────────────────────────────────
      // Toutes séries (primaires + DNS fusionné) dans le même graphique
      const allChartSeries = sources.flatMap(src => [
        { label: src.label, samples: src.samples },
        ...(src.extraSeries ?? []),
      ]);
      addChartSheet(wb, t('monitoring.xlsx_sheet_chart'), allChartSeries, nowTs);

      // ── Une feuille par source ────────────────────────────────────────────
      for (let si = 0; si < sources.length; si++) {
        const { label, samples, extraSeries } = sources[si];
        const sheetName = label.slice(0, 31);
        const ws = wb.addWorksheet(sheetName);
        const s = statsForSource(samples, nowTs);

        // Métadonnées (stats basées sur la série primaire = Ping pour Internet)
        ws.addRow([label + ' — ' + t('monitoring.export_modal_title')]);
        ws.addRow([t('monitoring.xlsx_col_since'), s.firstTs]);
        ws.addRow([t('monitoring.xlsx_col_uptime'), +s.up.toFixed(2) + ' %']);
        ws.addRow([t('monitoring.xlsx_col_avg'), s.avg != null ? +s.avg.toFixed(1) + ' ms' : '—']);
        ws.addRow([t('monitoring.xlsx_col_min') + ' / ' + t('monitoring.xlsx_col_max'),
          (s.min ?? '—') + ' / ' + (s.max ?? '—') + ' ms']);
        ws.addRow([t('monitoring.xlsx_col_p95'), (s.p95 ?? '—') + ' ms']);
        ws.addRow([t('monitoring.xlsx_col_outages'), s.outages.length]);
        ws.addRow([t('monitoring.xlsx_col_worst'), s.longest + ' s']);
        ws.addRow([]);

        // Incidents
        ws.addRow([t('monitoring.xlsx_sheet_incidents')]);
        ws.addRow([
          t('monitoring.xlsx_col_inc_start'),
          t('monitoring.xlsx_col_inc_end'),
          t('monitoring.xlsx_col_inc_dur'),
          t('monitoring.xlsx_col_inc_lost'),
          t('monitoring.xlsx_col_status'),
        ]);
        if (s.outages.length === 0) {
          ws.addRow([t('monitoring.xlsx_no_outage')]);
        } else {
          for (const o of s.outages) {
            const end = o.end ?? nowTs;
            ws.addRow([
              localDt(o.start),
              o.end != null ? localDt(o.end) : t('monitoring.xlsx_ongoing'),
              end - o.start,
              o.samples_lost,
              o.end != null ? t('monitoring.xlsx_restored') : t('monitoring.xlsx_ongoing'),
            ]);
          }
        }
        ws.addRow([]);

        // Série temporelle — colonnes Ping + DNS fusionnées si extraSeries présents
        ws.addRow([t('monitoring.xlsx_sheet_series')]);
        if (extraSeries?.length) {
          // Internet : colonnes Ping ms + DNS ms alignées sur le même timestamp
          ws.addRow([t('monitoring.xlsx_col_time'), t('monitoring.xlsx_col_state'), 'Ping (ms)', 'DNS (ms)']);
          const dnsSamples = extraSeries[0].samples;
          const maxLen = Math.max(samples.length, dnsSamples.length);
          for (let i = 0; i < maxLen; i++) {
            const ping = samples[i];
            const dns = dnsSamples[i];
            const ts = ping?.timestamp ?? dns?.timestamp;
            ws.addRow([
              ts != null ? localDt(ts) : '',
              ping ? (ping.alive ? 'OK' : 'KO') : '',
              ping?.latency_ms ?? '',
              dns?.latency_ms ?? '',
            ]);
          }
          ws.columns = [24, 10, 12, 12].map(w => ({ width: w }));
        } else {
          ws.addRow([t('monitoring.xlsx_col_time'), t('monitoring.xlsx_col_state'), t('monitoring.xlsx_col_latency')]);
          for (const sample of samples) {
            ws.addRow([localDt(sample.timestamp), sample.alive ? 'OK' : 'KO', sample.latency_ms ?? '']);
          }
          ws.columns = [24, 20, 14].map(w => ({ width: w }));
        }

        // Graphique — toutes les séries (Ping + DNS si présent) dans le même chart
        // Placé col F (col 5) pour ne pas couvrir les données (max col E pour incidents)
        const chartSeries = [{ label, samples }, ...(extraSeries ?? [])];
        const pngB64 = renderChartPng(chartSeries, nowTs);
        const imgId = wb.addImage({ base64: pngB64, extension: 'png' });
        ws.addImage(imgId, { tl: { col: 5, row: 0 } as any, ext: { width: 680, height: 300 } });
      }

      // Téléchargement
      const buffer = await wb.xlsx.writeBuffer();
      const blob = new Blob([buffer], { type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url; a.download = `lanprobe-sla-${slug}.xlsx`;
      document.body.appendChild(a); a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      showExport = false;
    } finally {
      exporting = false;
    }
  }

  onMount(() => { internetStatus.init(); });

  async function addHost() {
    if (adding) return;
    const ip = newIp.trim();
    if (!ip || $monitoring.has(ip)) return;
    adding = true;
    try {
      monitoring.addHost(ip);
      newIp = '';
      try { await invoke('cmd_start_ping', { ip }); }
      catch (e) { console.error('[Monitoring] cmd_start_ping failed for', ip, e); }
    } finally {
      adding = false;
    }
  }

  async function removeHost(ip: string) {
    await invoke('cmd_stop_ping', { ip });
    monitoring.removeHost(ip);
  }

  interface SlaView {
    ip: string;
    uptime_pct: number;
    avg: number | null;
    min: number | null;
    max: number | null;
    p95: number | null;
    total: number;
    failed: number;
    current_ms: number | null;
    alive: boolean | null;
  }

  function computeSla(host: HostMonitor): SlaView {
    const samples = host.history;
    const total = samples.length;
    const failed = samples.filter(s => !s.alive).length;
    const uptime_pct = total === 0 ? 100 : ((total - failed) / total) * 100;
    const lats = samples
      .filter(s => s.alive && s.latency_ms != null)
      .map(s => s.latency_ms as number)
      .sort((a, b) => a - b);
    const avg = lats.length ? lats.reduce((a, b) => a + b, 0) / lats.length : null;
    const min = lats.length ? lats[0] : null;
    const max = lats.length ? lats[lats.length - 1] : null;
    const p95 = lats.length ? lats[Math.min(lats.length - 1, Math.ceil(lats.length * 0.95) - 1)] : null;
    return {
      ip: host.ip,
      uptime_pct,
      avg, min, max, p95,
      total, failed,
      current_ms: host.current?.latency_ms ?? null,
      alive: host.current?.alive ?? null,
    };
  }

  const hostSlas = $derived([...$monitoring.values()].map(computeSla));

  // Historique local des ticks internet pour calculer avg/p95/min/max —
  // le store ne garde pas la série, il n'expose que le tick courant.
  // `untrack` est CRITIQUE : sans ça l'effet lit + écrit internetHistory
  // et Svelte 5 le détecte comme boucle (effect_update_depth_exceeded),
  // ce qui gelait complètement l'UI sur macOS (navigation impossible).
  let internetHistory = $state<number[]>([]);
  let internetDnsHistory = $state<(number | null)[]>([]);
  $effect(() => {
    const ms = $internetStatus?.icmp_ms;
    const dns = $internetStatus?.dns_ms ?? null;
    if (ms == null) return;
    untrack(() => {
      if (internetHistory.length >= 300) internetHistory.shift();
      internetHistory.push(ms);
      if (internetDnsHistory.length >= 300) internetDnsHistory.shift();
      internetDnsHistory.push(dns);
    });
  });
  const internetStats = $derived.by(() => {
    const lats = [...internetHistory].sort((a, b) => a - b);
    if (lats.length === 0) return { avg: null, p95: null, min: null, max: null } as { avg: number|null; p95: number|null; min: number|null; max: number|null };
    const avg = lats.reduce((a, b) => a + b, 0) / lats.length;
    return {
      avg,
      min: lats[0],
      max: lats[lats.length - 1],
      p95: lats[Math.min(lats.length - 1, Math.ceil(lats.length * 0.95) - 1)],
    };
  });
  function fmtMs(v: number | null, digits = 1): string {
    return v != null ? `${v.toFixed(digits)}ms` : '—';
  }

  function sparkline(history: PingEntry[], color: string): string {
    if (history.length < 2) return '';
    const W = 220, H = 32;
    const vals = history.map(p => p.latency_ms ?? 0);
    const max = Math.max(...vals, 1);
    const denom = Math.max(1, history.length - 1);
    // Trace seulement les segments entre samples "alive" consécutifs — un gap
    // où le host était down ne doit pas être relié par une ligne droite qui
    // masquerait la panne visuellement.
    let path = '';
    let drawing = false;
    history.forEach((p, i) => {
      const x = (i / denom) * W;
      if (p.alive && p.latency_ms != null) {
        const y = H - (p.latency_ms / max) * H;
        path += (drawing ? ' L' : 'M') + x.toFixed(1) + ',' + y.toFixed(1);
        drawing = true;
      } else {
        drawing = false;
      }
    });
    const failDots = history
      .map((p, i) => !p.alive ? `<circle cx="${((i / denom) * W).toFixed(1)}" cy="${H - 2}" r="2" fill="var(--ep-danger)"/>` : '')
      .join('');
    return `<svg width="100%" height="${H}" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none"><path d="${path}" fill="none" stroke="${color}" stroke-width="1.5"/>${failDots}</svg>`;
  }

  function uptimeColor(pct: number) {
    if (pct >= 99) return 'var(--ep-success)';
    if (pct >= 95) return '#f59e0b';
    return 'var(--ep-danger)';
  }

  function internetSpark(): string {
    const t = $internetStatus;
    if (!t || t.icmp_ms == null) return '';
    return `<svg width="100%" height="32" viewBox="0 0 220 32" preserveAspectRatio="none"><line x1="0" y1="16" x2="220" y2="16" stroke="var(--ep-success)" stroke-width="1.5"/></svg>`;
  }
</script>

<div class="page" class:compact={isSingle}>
  <div class="header">
    <h1>{$_('monitoring.title')}</h1>
    <div class="add-row">
      <input bind:value={newIp} placeholder={$_('monitoring.placeholder')} aria-label={$_('monitoring.placeholder')} onkeydown={(e) => e.key === 'Enter' && addHost()} />
      <button class="primary" onclick={addHost} disabled={adding}>{$_('monitoring.add')}</button>
      <button onclick={openExport}>{$_('monitoring.export')}</button>
    </div>
  </div>

  {#if showExport}
    <div class="modal-backdrop" role="presentation" onclick={() => showExport = false}>
      <div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
        <h2>{$_('monitoring.export_modal_title')}</h2>
        <div class="export-actions">
          <button class="sm" onclick={exportSelectAll}>{$_('monitoring.export_select_all')}</button>
          <button class="sm" onclick={exportDeselectAll}>{$_('monitoring.export_deselect_all')}</button>
        </div>
        <div class="export-list">
          <label class="export-item">
            <input type="checkbox" checked={exportIncludeInternet} onchange={() => exportIncludeInternet = !exportIncludeInternet} />
            <span class="export-ip">🌐 Internet</span>
            <span class="export-hint">{$_('monitoring.pinned')}</span>
          </label>
          {#each [...$monitoring.keys()] as ip}
            <label class="export-item">
              <input type="checkbox" checked={exportSelected.has(ip)} onchange={() => toggleExportIp(ip)} />
              <span class="export-ip">🖥 {ip}</span>
            </label>
          {/each}
          {#if $monitoring.size === 0}
            <p class="export-empty">{$_('monitoring.empty')}</p>
          {/if}
        </div>
        {#if !exportIncludeInternet && exportSelected.size === 0}
          <p class="export-warn">{$_('monitoring.export_empty')}</p>
        {/if}
        <div class="modal-footer">
          <button onclick={() => showExport = false}>{$_('monitoring.export_cancel')}</button>
          <button class="primary"
            disabled={exporting || (!exportIncludeInternet && exportSelected.size === 0)}
            onclick={doExportXlsx}>
            {exporting ? $_('monitoring.export_exporting') : $_('monitoring.export_xlsx')}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <div class="host-card pinned state-{$internetStatus?.state ?? 'offline'}">
    <div class="head">
      <span class="icon">🌐</span>
      <span class="name">{$_('monitoring.internet')}</span>
      <span class="pin-badge">{$_('monitoring.pinned')}</span>
    </div>
    <div class="state-pill">
      <span class="dot"></span>
      {$_(`monitoring.${$internetStatus?.state ?? 'offline'}`)}
    </div>
    <div class="uptime-block">
      <div class="uptime" style="color: {uptimeColor($internetStatus?.uptime_pct ?? 0)}">
        {($internetStatus?.uptime_pct ?? 0).toFixed(2)}%
      </div>
      <div class="uptime-sub">
        {$_('monitoring.uptime_samples', { values: { n: $internetStatus?.samples ?? 0 } })}
      </div>
    </div>
    <div class="actions"></div>
    <div class="row3">
      <span class="spark">{@html internetSpark()}</span>
      <div class="latency-live" style="color: var(--ep-success)">
        {$internetStatus?.icmp_ms != null ? `${$internetStatus.icmp_ms}ms` : '—'}
      </div>
      <div class="stats">
        <span title={$_('monitoring.stats.avg')}><span class="k">{$_('monitoring.stats.avg')}</span><span class="v">{fmtMs(internetStats.avg)}</span></span>
        <span title={$_('monitoring.stats.p95')}><span class="k">{$_('monitoring.stats.p95')}</span><span class="v">{fmtMs(internetStats.p95, 0)}</span></span>
        <span title={$_('monitoring.stats.min')}><span class="k">{$_('monitoring.stats.min')}</span><span class="v">{fmtMs(internetStats.min, 0)}</span></span>
        <span title={$_('monitoring.stats.max')}><span class="k">{$_('monitoring.stats.max')}</span><span class="v">{fmtMs(internetStats.max, 0)}</span></span>
      </div>
    </div>
  </div>

  {#each hostSlas as s (s.ip)}
    <div class="host-card state-{s.alive === null ? 'pending' : s.alive ? 'alive' : 'down'}">
      <div class="head">
        <span class="icon">🖥</span>
        <span class="name">{s.ip}</span>
      </div>
      <div class="state-pill">
        <span class="dot"></span>
        {s.alive === null ? '…' : s.alive ? $_('monitoring.alive') : $_('monitoring.down')}
      </div>
      <div class="uptime-block">
        <div class="uptime" style="color: {uptimeColor(s.uptime_pct)}">
          {s.uptime_pct.toFixed(2)}%
        </div>
        <div class="uptime-sub">
          {s.failed > 0
            ? $_('monitoring.uptime_samples_failed', { values: { n: s.total, f: s.failed } })
            : $_('monitoring.uptime_samples', { values: { n: s.total } })}
        </div>
      </div>
      <div class="actions">
        <button class="icon-btn" title={$_('monitoring.remove')} aria-label={$_('monitoring.remove')} onclick={() => removeHost(s.ip)}>✕</button>
      </div>
      <div class="row3">
        <span class="spark">{@html sparkline($monitoring.get(s.ip)?.history ?? [],
          s.alive === false ? 'var(--ep-danger)' : 'var(--ep-success)')}</span>
        <div class="latency-live" style="color: {s.alive === false ? 'var(--ep-danger)' : 'var(--ep-success)'}">
          {s.alive === false ? '—' : (s.current_ms != null ? `${s.current_ms}ms` : '…')}
        </div>
        <div class="stats">
          <span title={$_('monitoring.stats.avg')}><span class="k">{$_('monitoring.stats.avg')}</span><span class="v">{s.avg != null ? s.avg.toFixed(1) + 'ms' : '—'}</span></span>
          <span title={$_('monitoring.stats.p95')}><span class="k">{$_('monitoring.stats.p95')}</span><span class="v">{s.p95 != null ? s.p95 + 'ms' : '—'}</span></span>
          <span title={$_('monitoring.stats.min')}><span class="k">{$_('monitoring.stats.min')}</span><span class="v">{s.min != null ? s.min + 'ms' : '—'}</span></span>
          <span title={$_('monitoring.stats.max')}><span class="k">{$_('monitoring.stats.max')}</span><span class="v">{s.max != null ? s.max + 'ms' : '—'}</span></span>
        </div>
      </div>
    </div>
  {:else}
    <p class="empty">{$_('monitoring.empty')}</p>
  {/each}

</div>

<style>
  .page { padding: 24px; }
  .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; flex-wrap: wrap; gap: 12px; }
  h1 { font-size: 20px; font-weight: 700; }
  .add-row { display: flex; gap: 8px; }
  input { background: var(--ep-bg-tertiary); border: 1px solid var(--ep-border); border-radius: 6px; padding: 7px 12px; color: var(--ep-text-primary); font-family: var(--ep-font-mono); font-size: 13px; width: 200px; }
  button { padding: 7px 14px; border-radius: 6px; border: 1px solid var(--ep-border); background: var(--ep-bg-tertiary); color: var(--ep-text-primary); cursor: pointer; font-size: 13px; font-weight: 600; }
  button.primary { background: var(--ep-accent); border-color: var(--ep-accent); color: #fff; }
  .icon-btn { background: transparent; border: 1px solid var(--ep-border); color: var(--ep-text-muted); padding: 4px 8px; border-radius: 5px; font-size: 11px; }

  .host-card {
    background: var(--ep-glass-bg);
    border: 1px solid var(--ep-glass-border);
    border-left: 3px solid var(--ep-glass-border);
    border-radius: var(--ep-radius-lg);
    padding: 18px 20px;
    margin-bottom: 10px;
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 12px 24px;
    transition: border-color 0.15s;
  }
  .host-card:hover { border-color: var(--ep-glass-border-strong); }
  .host-card.pinned { border-left-color: #06b6d4; background: color-mix(in srgb, #06b6d4 5%, var(--ep-glass-bg)); }
  .host-card.state-alive   { border-left-color: var(--ep-success); }
  .host-card.state-down    { border-left-color: var(--ep-danger); }
  .host-card.state-online  { border-left-color: var(--ep-success); }
  .host-card.state-limited { border-left-color: #f59e0b; }
  .host-card.state-offline { border-left-color: var(--ep-danger); }

  .head { display: flex; align-items: center; gap: 10px; font-size: 14px; font-weight: 700; grid-column: 1; }
  .head .icon { font-size: 16px; }
  .pin-badge { font-size: 9px; text-transform: uppercase; letter-spacing: .5px; background: color-mix(in srgb, #06b6d4 20%, transparent); color: #06b6d4; padding: 2px 6px; border-radius: 4px; font-weight: 700; }

  .state-pill { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; grid-column: 2; justify-self: end; }
  .state-pill .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ep-text-muted); }
  .host-card.state-alive .state-pill,
  .host-card.state-online .state-pill { color: var(--ep-success); }
  .host-card.state-alive .state-pill .dot,
  .host-card.state-online .state-pill .dot { background: var(--ep-success); box-shadow: 0 0 8px var(--ep-success); }
  .host-card.state-down .state-pill,
  .host-card.state-offline .state-pill { color: var(--ep-danger); }
  .host-card.state-down .state-pill .dot,
  .host-card.state-offline .state-pill .dot { background: var(--ep-danger); box-shadow: 0 0 8px var(--ep-danger); }
  .host-card.state-limited .state-pill { color: #f59e0b; }
  .host-card.state-limited .state-pill .dot { background: #f59e0b; box-shadow: 0 0 8px #f59e0b; }

  .uptime-block { grid-column: 1; }
  .uptime { font-size: 32px; font-weight: 800; line-height: 1; }
  .uptime-sub { font-size: 10px; color: var(--ep-text-muted); margin-top: 4px; text-transform: uppercase; letter-spacing: .5px; }
  .actions { grid-column: 2; display: flex; gap: 6px; justify-self: end; }

  .row3 { grid-column: 1 / span 2; display: flex; align-items: center; gap: 16px; padding-top: 10px; border-top: 1px solid var(--ep-glass-border); }
  .spark { flex: 1; min-width: 0; }
  .latency-live { font-size: 16px; font-weight: 700; min-width: 60px; text-align: right; font-family: var(--ep-font-mono); }
  .stats { display: flex; gap: 14px; font-size: 11px; color: var(--ep-text-secondary); }
  .stats .k { color: var(--ep-text-muted); margin-right: 3px; text-transform: uppercase; font-size: 9px; letter-spacing: .5px; }
  .stats .v { color: var(--ep-text-primary); font-weight: 600; font-family: var(--ep-font-mono); }

  .empty { color: var(--ep-text-muted); font-size: 14px; text-align: center; padding: 30px; }

  /* ── Export modal ─────────────────────────────────────────────────── */
  .modal-backdrop {
    position: fixed; inset: 0; background: rgba(0,0,0,.5);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
  }
  .modal {
    background: var(--ep-bg-secondary); border: 1px solid var(--ep-glass-border-strong);
    border-radius: var(--ep-radius-lg); padding: 24px; min-width: 320px; max-width: 420px; width: 90%;
  }
  .modal h2 { font-size: 16px; font-weight: 700; margin-bottom: 14px; }
  .export-actions { display: flex; gap: 8px; margin-bottom: 12px; }
  .export-list { display: flex; flex-direction: column; gap: 4px; max-height: 240px; overflow-y: auto; margin-bottom: 12px; }
  .export-item {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px; border-radius: 7px; cursor: pointer;
    background: var(--ep-glass-bg); border: 1px solid var(--ep-glass-border);
    font-size: 13px;
    transition: border-color 0.12s;
  }
  .export-item:hover { border-color: var(--ep-accent); }
  .export-item input[type="checkbox"] { accent-color: var(--ep-accent); width: 15px; height: 15px; flex-shrink: 0; }
  .export-ip { flex: 1; font-family: var(--ep-font-mono); font-size: 13px; }
  .export-hint { font-size: 10px; color: var(--ep-text-muted); text-transform: uppercase; letter-spacing: .5px; }
  .export-empty { color: var(--ep-text-muted); font-size: 12px; padding: 10px 0; text-align: center; }
  .export-warn { font-size: 12px; color: #f59e0b; margin-bottom: 10px; }
  .modal-footer { display: flex; gap: 8px; justify-content: flex-end; margin-top: 4px; }
  button.sm { font-size: 11px; padding: 4px 10px; }

  /* En single-page on aplatit complètement chaque card en une ligne :
     icône + nom + état + uptime + latence + sparkline + stats, sans
     padding vertical ni border-top, pour faire tenir 5-6 hosts sans scroll.*/
  .page.compact { padding: 12px; }
  .page.compact h1 { font-size: 14px; }
  .page.compact .header { margin-bottom: 10px; }
  .page.compact .add-row input { padding: 4px 8px; font-size: 11px; width: 150px; }
  .page.compact .add-row button { padding: 4px 10px; font-size: 11px; }

  .page.compact .host-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    margin-bottom: 4px;
    border-radius: 6px;
  }
  .page.compact .head { font-size: 11px; gap: 5px; flex: 0 0 auto; min-width: 0; }
  .page.compact .head .icon { font-size: 12px; }
  .page.compact .head .name { max-width: 110px; overflow: hidden; text-overflow: ellipsis; }
  .page.compact .pin-badge { display: none; }
  .page.compact .state-pill { font-size: 10px; flex: 0 0 auto; order: 2; justify-self: start; }
  .page.compact .state-pill .dot { width: 6px; height: 6px; }

  .page.compact .uptime-block { display: flex; align-items: baseline; gap: 4px; flex: 0 0 auto; order: 3; }
  .page.compact .uptime { font-size: 13px; font-weight: 700; }
  .page.compact .uptime-sub { display: none; }

  .page.compact .row3 {
    flex: 1 1 auto;
    padding-top: 0;
    border-top: none;
    min-width: 0;
    gap: 8px;
    order: 4;
  }
  .page.compact .spark { max-width: 120px; }
  .page.compact .latency-live { font-size: 11px; min-width: 38px; }
  .page.compact .stats { gap: 8px; font-size: 10px; }
  .page.compact .stats .k { display: none; }

  .page.compact .actions { order: 5; flex: 0 0 auto; }
  .page.compact .icon-btn { padding: 2px 6px; font-size: 10px; }
</style>
