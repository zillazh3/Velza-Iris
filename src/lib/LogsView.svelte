<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { Check, Copy, Pause, Play, Trash2 } from "@lucide/svelte";
  import { api } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";
  import IrisCheckbox from "$lib/components/ui/IrisCheckbox.svelte";
  import { formatClock, LogStore, rowsToText, type LogFilter, type LogKind, type LogRow } from "$lib/logs";

  const POLL_MS = 100;
  /** Filas que se pintan: el resto sigue en memoria (búsqueda, copiar) pero no cuesta DOM. */
  const VIEW_CAP = 500;

  const store = new LogStore();
  let cursor = 0;

  let rows = $state.raw<LogRow[]>([]);
  let total = $state(0);
  let filter = $state<LogFilter>("all");
  let search = $state("");
  let collapse = $state(true);
  let paused = $state(false);
  let copied = $state(false);
  let rateIn = $state(0);
  let rateOut = $state(0);
  let skipped = $state(0);
  let listEl = $state<HTMLDivElement | null>(null);
  let stick = true;

  const filters: { id: LogFilter; key: string }[] = [
    { id: "all", key: "logs.filter.all" },
    { id: "oscIn", key: "logs.filter.oscIn" },
    { id: "oscOut", key: "logs.filter.oscOut" },
    { id: "engine", key: "logs.filter.engine" },
    { id: "error", key: "logs.filter.error" },
  ];
  const badge: Record<LogKind, string> = { oscIn: "IN", oscOut: "OUT", engine: "ENGINE", error: "ERROR" };

  async function refresh() {
    const all = store.rows(filter, search);
    total = all.length;
    rows = all.length > VIEW_CAP ? all.slice(-VIEW_CAP) : all;
    if (stick) {
      await tick();
      if (listEl) listEl.scrollTop = listEl.scrollHeight;
    }
  }

  let busy = false;
  async function poll() {
    if (busy) return;
    busy = true;
    try {
      const b = await api.logsSince(cursor);
      cursor = b.nextCursor;
      rateIn = b.rateIn;
      rateOut = b.rateOut;
      skipped += b.dropped;
      if (b.entries.length) {
        store.ingest(b.entries, collapse);
        // En pausa se sigue consumiendo el cursor (sin huecos al reanudar), pero no se repinta.
        if (!paused) await refresh();
      }
    } catch {
      /* backend aún iniciando */
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void poll();
    const id = setInterval(poll, POLL_MS);
    return () => clearInterval(id);
  });

  // Cambiar filtro, búsqueda o reanudar repinta con lo que ya hay en memoria.
  $effect(() => {
    void filter;
    void search;
    if (!paused) untrack(() => void refresh());
  });

  function onScroll() {
    if (listEl) stick = listEl.scrollHeight - listEl.scrollTop - listEl.clientHeight < 24;
  }

  function clear() {
    store.clear();
    skipped = 0;
    void refresh();
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(rowsToText(store.rows(filter, search)));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* portapapeles no disponible */
    }
  }
</script>

<section class="logs" aria-label={t("nav.logs")}>
  <header class="bar">
    <div class="chips" role="group" aria-label={t("logs.filter.label")}>
      {#each filters as f (f.id)}
        <button type="button" class="chip" class:active={filter === f.id} aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>
          {t(f.key)}
        </button>
      {/each}
    </div>

    <input class="search" type="search" placeholder={t("logs.search")} aria-label={t("logs.search")} bind:value={search} />

    <IrisCheckbox size="sm" bind:checked={collapse} label={t("logs.collapse")} title={t("logs.collapseHelp")} />

    <div class="actions">
      <span class="rate" title={t("logs.rateHelp")}>{t("logs.rate", { in: rateIn, out: rateOut })}</span>
      <button type="button" class="ghost sm" onclick={() => (paused = !paused)} aria-pressed={paused}>
        {#if paused}<Play size={14} /> {t("logs.resume")}{:else}<Pause size={14} /> {t("logs.pause")}{/if}
      </button>
      <button type="button" class="ghost sm" onclick={clear}><Trash2 size={14} /> {t("logs.clear")}</button>
      <button type="button" class="ghost sm" onclick={copy}>
        {#if copied}<Check size={14} /> {t("logs.copied")}{:else}<Copy size={14} /> {t("logs.copy")}{/if}
      </button>
    </div>
  </header>

  <div class="list" bind:this={listEl} onscroll={onScroll} role="log" aria-live="off">
    {#each rows as r (r.id)}
      <div class="row {r.kind}">
        <span class="time">{formatClock(r.tMs)}</span>
        <span class="badge">{badge[r.kind]}</span>
        <span class="addr" title={r.addr}>{r.addr}</span>
        <span class="payload" title={r.payload}>{r.payload}</span>
        {#if r.count > 1}<span class="count">×{r.count}</span>{/if}
      </div>
    {:else}
      <p class="empty">{t("logs.empty")}</p>
    {/each}
  </div>

  <footer class="foot">
    {#if paused}<span class="paused">{t("logs.pausedNote")}</span>{/if}
    {#if total > VIEW_CAP}<span>{t("logs.showingLast", { n: VIEW_CAP, total })}</span>{/if}
    {#if skipped > 0}<span>{t("logs.skipped", { n: skipped })}</span>{/if}
  </footer>
</section>

<style>
  .logs {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: calc(100vh - 150px);
    min-height: 320px;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
  }
  .chips {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-md);
    background: var(--surface);
  }
  .chip {
    padding: 4px 10px;
    font-size: 0.78rem;
    background: transparent;
    color: var(--text-2);
    border-radius: var(--radius-sm);
  }
  .chip:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .chip.active {
    background: var(--accent-subtle);
    color: var(--text);
  }
  .search {
    flex: 1;
    min-width: 160px;
    max-width: 320px;
    padding: 6px 10px;
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
  }
  .search:focus-visible {
    outline: 2px solid var(--accent);
  }
  .actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .rate {
    margin-right: 6px;
    font-family: var(--font-mono);
    font-size: 0.76rem;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  .list {
    flex: 1;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--bg-viewport);
    padding: 6px 0;
    font-family: var(--font-mono);
    font-size: 0.78rem;
  }
  .row {
    display: grid;
    grid-template-columns: 94px 58px minmax(140px, 260px) 1fr auto;
    align-items: baseline;
    gap: 10px;
    padding: 2px 14px;
    /* Virtualización nativa: el navegador salta el layout de las filas fuera de pantalla */
    content-visibility: auto;
    contain-intrinsic-size: auto 20px;
  }
  .row:hover {
    background: var(--surface);
  }
  .row.error {
    background: color-mix(in srgb, var(--danger) 6%, transparent);
  }
  .time {
    color: var(--text-muted);
  }
  .badge {
    justify-self: start;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--text-2);
    background: color-mix(in srgb, var(--text-2) 12%, transparent);
  }
  .oscIn .badge {
    color: var(--axis-z);
    background: color-mix(in srgb, var(--axis-z) 12%, transparent);
  }
  .oscOut .badge {
    color: var(--accent-hover);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .error .badge {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .addr {
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .payload {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--surface-raised);
    color: var(--text-2);
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
  }
  .empty {
    margin: 24px;
    text-align: center;
    color: var(--text-muted);
    font-family: inherit;
  }
  .foot {
    display: flex;
    gap: 16px;
    min-height: 1.2em;
    font-size: 0.76rem;
    color: var(--text-muted);
  }
  .paused {
    color: var(--warn);
  }
</style>
