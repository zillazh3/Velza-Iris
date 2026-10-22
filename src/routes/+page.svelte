<script lang="ts">
  import { onMount } from "svelte";
  import { Camera, Crosshair, Languages, Plug } from "@lucide/svelte";
  import { api, approxMm, type Preset, type Status } from "$lib/api";
  import { i18n, t, type Key } from "$lib/i18n/index.svelte";

  let status = $state<Status>({ connected: false, open: false, anchored: false });
  let presets = $state<Preset[]>([]);
  let message = $state<{ text: string; ok: boolean } | null>(null);

  const categories = ["retrato", "cuerpo", "ambiente", "movimiento", "video", "ref_sheet"] as const;
  const groups = $derived(
    categories.map((c) => ({ c, items: presets.filter((p) => p.category === c) })).filter((g) => g.items.length),
  );

  // Los mensajes de error vienen del backend tal cual (en español).
  async function run(action: () => Promise<unknown>, okText?: string) {
    try {
      await action();
      message = okText ? { text: okText, ok: true } : null;
    } catch (e) {
      message = { text: String(e), ok: false };
    }
  }

  let polling = false;
  async function tick() {
    if (polling) return;
    polling = true;
    try {
      if (!status.connected) await api.connect().catch(() => {});
      status = await api.status();
    } finally {
      polling = false;
    }
  }

  const ready = (p: Preset) => status.connected && status.open && (p.frame === null || status.anchored);

  onMount(() => {
    run(async () => {
      const lib = await api.listPresets();
      presets = lib.presets;
      if (lib.warnings.length) throw lib.warnings.join("\n");
    });
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  });
</script>

<main>
  <header>
    <div>
      <h1>{t("app.title")}</h1>
      <p class="tagline">{t("app.tagline")}</p>
    </div>
    <button class="ghost" onclick={() => i18n.toggle()}><Languages size={16} /> {t("lang.switch")}</button>
  </header>

  <section class="chips" aria-live="polite">
    <span class="chip" class:on={status.connected}>
      <Plug size={16} /> {t(status.connected ? "status.connected" : "status.disconnected")}
    </span>
    <span class="chip" class:on={status.open}>
      <Camera size={16} /> {t(status.open ? "status.open" : "status.closed")}
    </span>
    <span class="chip" class:on={status.anchored}>
      <Crosshair size={16} /> {t(status.anchored ? "status.anchored" : "status.unanchored")}
    </span>
  </section>

  {#if !status.connected}
    <p class="banner">{t("banner.disconnected")}</p>
  {:else if !status.open}
    <p class="banner">{t("banner.closed")}</p>
  {/if}

  {#if message}
    <p class="msg" class:ok={message.ok} role="status">{message.text}</p>
  {/if}

  <section class="card anchor">
    <div>
      <h2>{t("calibrate.title")}</h2>
      <p>{t("calibrate.help")}</p>
    </div>
    <button
      class="primary"
      disabled={!status.connected || !status.open}
      onclick={() => run(() => api.calibrate().then(tick), t("calibrate.done"))}
    >
      <Crosshair size={16} /> {t("calibrate.button")}
    </button>
  </section>

  <h2 class="section">{t("presets.title")}</h2>
  {#each groups as g (g.c)}
    <h3>{t(`cat.${g.c}` as Key)}</h3>
    <div class="grid">
      {#each g.items as p (p.name)}
        <article class="card preset">
          <h4>{p.name}</h4>
          <p class="meta">
            {#if p.look.zoom != null}<span>{t("presets.mm", { mm: approxMm(p.look.zoom) })}</span>{/if}
            {#if p.look.aperture != null}<span>{t("presets.aperture", { value: p.look.aperture })}</span>{/if}
            {#if p.look.focus}<span>{t(`focus.${p.look.focus}` as Key)}</span>{/if}
            {#if p.modifier}
              <span class="tag">{t("presets.modifier")}</span>
              {#if p.look.exposure != null}<span>{t("presets.ev", { value: (p.look.exposure > 0 ? "+" : "") + p.look.exposure })}</span>{/if}
            {/if}
          </p>
          <button
            class="primary"
            disabled={!ready(p)}
            title={p.frame && !status.anchored ? t("presets.needAnchor") : undefined}
            onclick={() => run(() => api.applyPreset(p.name))}
          >
            {t("presets.apply")}
          </button>
        </article>
      {/each}
    </div>
  {/each}

  <footer>{t("footer.privacy")}</footer>
</main>

<style>
  main {
    max-width: 960px;
    margin: 0 auto;
    padding: 28px 24px 40px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
  }
  h1 {
    font-family: "Fraunces Variable", Georgia, serif;
    font-weight: 500;
    font-size: 2.6rem;
    letter-spacing: -0.01em;
    margin: 0;
    background: linear-gradient(120deg, var(--text), var(--lavender));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .tagline {
    margin: 4px 0 0;
    color: var(--text-2);
  }
  h2.section {
    margin: 32px 0 4px;
    font-size: 1.15rem;
  }
  h3 {
    margin: 20px 0 10px;
    font-size: 0.8rem;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--lavender);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 20px 0 12px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 0.85rem;
    border: 1px solid transparent;
    transition: all 200ms var(--ease);
  }
  .chip.on {
    color: var(--text);
    border-color: var(--accent);
    box-shadow: 0 0 18px rgba(61, 90, 254, 0.25);
  }
  .banner,
  .msg {
    margin: 10px 0;
    padding: 12px 16px;
    border-radius: 12px;
    background: var(--velvet);
    white-space: pre-line;
  }
  .msg {
    border: 1px solid #c85a7a;
  }
  .msg.ok {
    border-color: var(--lavender);
  }
  .card {
    background: var(--surface);
    border-radius: var(--radius);
    padding: 18px;
    border: 1px solid rgba(184, 169, 255, 0.08);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25);
    transition:
      border-color 200ms var(--ease),
      transform 200ms var(--ease);
  }
  .card.preset:hover {
    border-color: var(--lavender);
    transform: translateY(-2px);
  }
  .anchor {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
  }
  .anchor h2 {
    margin: 0 0 4px;
    font-size: 1.05rem;
  }
  .anchor p {
    margin: 0;
    color: var(--text-2);
    max-width: 56ch;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 14px;
  }
  h4 {
    margin: 0 0 6px;
    font-size: 1rem;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    margin: 0 0 14px;
    color: var(--text-2);
    font-size: 0.85rem;
  }
  .tag {
    color: var(--lavender);
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    border: 0;
    border-radius: 12px;
    padding: 9px 16px;
    transition:
      background 180ms var(--ease),
      opacity 180ms var(--ease);
  }
  button.primary {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  button.primary:hover:not(:disabled) {
    background: var(--lavender);
    color: var(--bg);
  }
  button.ghost {
    background: transparent;
    color: var(--text-2);
    border: 1px solid rgba(184, 169, 255, 0.2);
  }
  button.ghost:hover {
    color: var(--text);
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  button:focus-visible {
    outline: 2px solid var(--lavender);
    outline-offset: 2px;
  }
  footer {
    margin-top: 40px;
    color: var(--text-2);
    font-size: 0.8rem;
    text-align: center;
  }
</style>
