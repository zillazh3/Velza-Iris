<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowLeftRight, OctagonX, Pause, Play, Repeat, RotateCw, Square } from "@lucide/svelte";
  import { api, type Motion, type MotionStatus, type TurnaroundProgress } from "$lib/api";
  import { t, localizeError, localizeMotionName, type Key } from "$lib/i18n/index.svelte";

  let { ready, run }: { ready: boolean; run: (action: () => Promise<unknown>, okText?: string) => Promise<void> } = $props();

  let motions = $state<Motion[]>([]);
  let status = $state<MotionStatus>({ name: null, state: "stopped", error: null });
  let turnaround = $state<TurnaroundProgress | null>(null);
  let speed = $state(1);
  let looping = $state(false);

  const active = $derived(status.name !== null);
  const turnaroundRunning = $derived(turnaround !== null && !turnaround.finished);

  // Al cambiar de movimiento, los controles vuelven a su estado inicial (el backend arranca en 1×, sin loop forzado).
  function play(name: string) {
    speed = 1;
    looping = false;
    run(() => api.playMotion(name));
  }

  onMount(() => {
    run(async () => (motions = await api.listMotions()));
    const poll = async () => {
      try {
        [status, turnaround] = await Promise.all([api.motionStatus(), api.turnaroundStatus()]);
      } catch {
        /* sin backend todavía */
      }
    };
    poll();
    const id = setInterval(poll, 500);
    return () => clearInterval(id);
  });
</script>

<section class="panel">
  <div class="head">
    <h2>{t("motion.title")}</h2>
    <button class="primary panic" title={t("motion.panicHelp")} onclick={() => run(() => api.panicStop())}>
      <OctagonX size={16} /> {t("motion.panic")}
    </button>
  </div>

  {#if !ready}<p class="hint">{t("motion.needReady")}</p>{/if}
  {#if status.error}<p class="error" role="alert">{localizeError(status.error)}</p>{/if}

  {#if active}
    <div class="card controls" aria-live="polite">
      <div class="now">
        <strong>{status.name ? localizeMotionName(status.name) : ""}</strong>
        <span class="state" class:on={status.state === "playing"}>{t(`motion.state.${status.state}` as Key)}</span>
      </div>
      <div class="buttons">
        {#if status.state === "playing"}
          <button class="ghost" onclick={() => run(() => api.motionControl("pause"))}>
            <Pause size={16} /> {t("motion.pause")}
          </button>
        {:else}
          <button class="ghost" disabled={status.state === "stopped"} onclick={() => run(() => api.motionControl("play"))}>
            <Play size={16} /> {t("motion.resume")}
          </button>
        {/if}
        <button class="ghost" onclick={() => run(() => api.stopMotion())}><Square size={16} /> {t("motion.stop")}</button>
        <button class="ghost" title={t("motion.reverse")} onclick={() => run(() => api.motionControl("reverse"))}>
          <ArrowLeftRight size={16} /> {t("motion.reverse")}
        </button>
        <button
          class="ghost"
          aria-pressed={looping}
          class:pressed={looping}
          onclick={() => run(() => api.motionControl({ loop: (looping = !looping) }))}
        >
          <Repeat size={16} /> {t("motion.loop")}
        </button>
        <label class="speed">
          {t("motion.speed")}
          <input
            type="range"
            min="0.25"
            max="4"
            step="0.25"
            bind:value={speed}
            onchange={() => run(() => api.motionControl({ speed }))}
          />
          <span>{speed}×</span>
        </label>
      </div>
    </div>
  {/if}

  <div class="grid">
    {#each motions as m (m.name)}
      <article class="card">
        <h4>{localizeMotionName(m.name)}</h4>
        <p class="meta">
          <span>{t("motion.duration", { s: m.duration })}</span>
          <span>{t(`motion.loop.${m.loop}` as Key)}</span>
          {#if m.handheld}<span class="tag">{t("motion.handheld")}</span>{/if}
        </p>
        <button class="primary" disabled={!ready} onclick={() => play(m.name)}>
          <Play size={16} /> {t("motion.play")}
        </button>
      </article>
    {/each}
  </div>

  <article class="card turnaround">
    <div>
      <h4><RotateCw size={16} /> {t("turnaround.title")}</h4>
      <p class="help">{t("turnaround.help")}</p>
      {#if turnaround}
        {#if turnaround.finished}
          <p class="progress">{t("turnaround.done", { done: turnaround.done, total: turnaround.total })}</p>
          {#if turnaround.error}<p class="error" role="alert">{localizeError(turnaround.error)}</p>{/if}
        {:else}
          <p class="progress">{t("turnaround.progress", { done: turnaround.done + 1, total: turnaround.total })}</p>
          <progress max={turnaround.total} value={turnaround.done}></progress>
        {/if}
      {/if}
    </div>
    <div class="buttons">
      {#if turnaroundRunning}
        <button class="ghost" onclick={() => run(() => api.stopMotion())}><Square size={16} /> {t("motion.stop")}</button>
      {:else}
        <button class="primary" disabled={!ready} onclick={() => run(() => api.startTurnaround(false))}>
          {t("turnaround.eight")}
        </button>
        <button class="primary" disabled={!ready} onclick={() => run(() => api.startTurnaround(true))}>
          {t("turnaround.four")}
        </button>
      {/if}
    </div>
  </article>
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin-top: 32px;
  }
  h2 {
    margin: 0;
    font-size: 1.15rem;
  }
  .panic {
    background: #b3365a;
  }
  .panic:hover:not(:disabled) {
    background: #e0607f;
    color: #fff;
  }
  .hint {
    margin: 0;
    color: var(--text-2);
  }
  .error {
    margin: 0;
    padding: 10px 14px;
    border: 1px solid #c85a7a;
    border-radius: 12px;
    background: var(--surface-raised);
  }
  .controls {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .now {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .state {
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 0.8rem;
    border: 1px solid transparent;
  }
  .state.on {
    color: var(--text);
    background: var(--accent-subtle);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .pressed {
    color: var(--text);
    background: var(--accent-subtle);
  }
  .speed {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    font-size: 0.9rem;
  }
  input[type="range"] {
    accent-color: var(--accent);
  }
  .turnaround {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
  }
  .turnaround h4 {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .help,
  .progress {
    margin: 0 0 8px;
    color: var(--text-2);
    max-width: 56ch;
    font-size: 0.9rem;
  }
  progress {
    width: 100%;
    accent-color: var(--accent);
  }
</style>
