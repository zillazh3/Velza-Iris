<script lang="ts">
  import { onMount } from "svelte";
  import {
    Aperture,
    Camera,
    ChevronsLeft,
    ChevronsRight,
    Clapperboard,
    Crosshair,
    Grid3x3,
    Languages,
    LayoutGrid,
    Mountain,
    Orbit,
    PersonStanding,
    Plug,
    UserRound,
  } from "@lucide/svelte";
  import { api, approxMm, type Preset, type Status } from "$lib/api";
  import { i18n, t, type Key } from "$lib/i18n/index.svelte";
  import MotionPanel from "$lib/MotionPanel.svelte";

  type Category = Preset["category"];

  let status = $state<Status>({ connected: false, open: false, anchored: false });
  let presets = $state<Preset[]>([]);
  let message = $state<{ text: string; ok: boolean } | null>(null);
  let selected = $state<"all" | Category>("all");
  // ponytail: el estado del sidebar no se guarda entre sesiones (igual que el idioma).
  // Movimientos + turnaround (el panel los lista); cuentan en la categoría Movimiento.
  let motionCount = $state(0);
  let collapsed = $state(false);
  let narrow = $state(false);
  const rail = $derived(collapsed || narrow);

  const categories: Category[] = ["retrato", "cuerpo", "ambiente", "movimiento", "video", "ref_sheet"];
  const icons = {
    all: LayoutGrid,
    retrato: UserRound,
    cuerpo: PersonStanding,
    ambiente: Mountain,
    movimiento: Orbit,
    video: Clapperboard,
    ref_sheet: Grid3x3,
  } as const;

  const count = (c: "all" | Category) =>
    (c === "all" ? presets.length : presets.filter((p) => p.category === c).length) +
    (c === "all" || c === "movimiento" ? motionCount : 0);
  const groups = $derived(
    categories
      .filter((c) => selected === "all" || c === selected)
      .map((c) => ({ c, items: presets.filter((p) => p.category === c) }))
      .filter((g) => g.items.length),
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
  const label = (c: "all" | Category) => (c === "all" ? t("nav.all") : t(`cat.${c}` as Key));

  onMount(() => {
    run(async () => {
      const lib = await api.listPresets();
      presets = lib.presets;
      if (lib.warnings.length) throw lib.warnings.join("\n");
    });
    api.listMotions().then((m) => (motionCount = m.length + 1)).catch(() => {});
    tick();
    const id = setInterval(tick, 1000);
    // Ventana angosta: el sidebar queda compacto sin importar la preferencia.
    const mq = window.matchMedia("(max-width: 820px)");
    const onChange = () => (narrow = mq.matches);
    onChange();
    mq.addEventListener("change", onChange);
    return () => {
      clearInterval(id);
      mq.removeEventListener("change", onChange);
    };
  });
</script>

<div class="shell" class:rail>
  <nav class="sidebar" aria-label={t("presets.title")}>
    <h1 class="brand"><Aperture size={22} /> <span class="text">{t("app.title")}</span></h1>

    <ul>
      {#each ["all", ...categories] as const as c (c)}
        {@const Icon = icons[c]}
        <li>
          <button
            class="item"
            class:empty={count(c) === 0}
            aria-current={selected === c ? "page" : undefined}
            title={label(c)}
            onclick={() => (selected = c)}
          >
            <Icon size={18} />
            <span class="text">{label(c)}</span>
            <span class="count">{count(c)}</span>
          </button>
        </li>
      {/each}
    </ul>

    <button class="item lang" title={t("lang.switch")} onclick={() => i18n.toggle()}>
      <Languages size={18} /> <span class="text">{t("lang.switch")}</span>
    </button>

    {#if !narrow}
      {@const Chevron = rail ? ChevronsRight : ChevronsLeft}
      <button
        class="item"
        aria-expanded={!rail}
        title={t(rail ? "nav.expand" : "nav.collapse")}
        onclick={() => (collapsed = !collapsed)}
      >
        <Chevron size={18} /> <span class="text">{t("nav.collapse")}</span>
      </button>
    {/if}
  </nav>

  <main>
    <p class="tagline">{t("app.tagline")}</p>

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

    <section class="card anchor">
      <div>
        <h2>{t("reset.title")}</h2>
        <p>{t("reset.help")}</p>
      </div>
      <button class="ghost" disabled={!status.connected} onclick={() => run(() => api.resetDefaults(), t("reset.done"))}>
        {t("reset.button")}
      </button>
    </section>

    {#if selected !== "movimiento"}
      <h2 class="section">{selected === "all" ? t("presets.title") : label(selected)}</h2>
    {/if}
    {#each groups as g (g.c)}
      {#if selected === "all"}<h3>{label(g.c)}</h3>{/if}
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
    {#if groups.length === 0 && selected !== "movimiento"}
      <p class="empty-state">{t("presets.empty")}</p>
    {/if}

    {#if selected === "all" || selected === "movimiento"}
      <MotionPanel ready={status.connected && status.open && status.anchored} {run} />
    {/if}

    <footer>{t("footer.privacy")}</footer>
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 232px 1fr;
    min-height: 100vh;
    transition: grid-template-columns 200ms var(--ease);
  }
  .shell.rail {
    grid-template-columns: 68px 1fr;
  }
  .sidebar {
    position: sticky;
    top: 0;
    height: 100vh;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 22px 14px;
    background: rgba(26, 18, 48, 0.72);
    border-right: 1px solid rgba(184, 169, 255, 0.1);
    backdrop-filter: blur(10px);
  }
  .sidebar ul {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 6px;
    font-family: "Fraunces Variable", Georgia, serif;
    font-weight: 500;
    font-size: 1.35rem;
    color: var(--lavender);
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: transparent;
    color: var(--text-2);
    border: 1px solid transparent;
    text-align: left;
  }
  .item:hover {
    color: var(--text);
    background: rgba(184, 169, 255, 0.08);
  }
  .item[aria-current="page"] {
    color: var(--text);
    background: var(--velvet);
    border-color: var(--accent);
  }
  .item.empty:not([aria-current="page"]) {
    opacity: 0.55;
  }
  .item .text {
    flex: 1;
  }
  .count {
    font-size: 0.75rem;
    color: var(--text-2);
  }
  .lang {
    margin-top: auto;
  }
  main {
    min-width: 0;
    max-width: 960px;
    width: 100%;
    margin: 0 auto;
    padding: 28px 24px 40px;
  }
  .tagline {
    margin: 0;
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
  .empty-state {
    margin: 16px 0;
    color: var(--text-2);
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
  .anchor + .anchor {
    margin-top: 12px;
  }
  .card.preset {
    margin-top: 0;
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
  footer {
    margin-top: 40px;
    color: var(--text-2);
    font-size: 0.8rem;
    text-align: center;
  }

  /* Sidebar compacto (rail de iconos): por la flecha o por ventana angosta. */
  .rail .sidebar {
    padding: 22px 8px;
    align-items: center;
  }
  .rail .text,
  .rail .count {
    display: none;
  }
  .rail .item {
    justify-content: center;
    padding: 10px;
  }
  .rail .brand {
    margin: 0;
  }
</style>
