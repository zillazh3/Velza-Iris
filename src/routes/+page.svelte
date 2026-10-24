<script lang="ts">
  import { onMount } from "svelte";
  import {
    Activity,
    Aperture,
    BookOpen,
    Camera,
    ChevronsLeft,
    ChevronsRight,
    Clapperboard,
    Copy,
    Crosshair,
    Film,
    Globe,
    Grid3x3,
    Home,
    Languages,
    LayoutGrid,
    Mountain,
    Orbit,
    PersonStanding,
    Plug,
    RotateCcw,
    Settings,
    Sparkles,
    Trash2,
    Upload,
    UserRound,
  } from "@lucide/svelte";
  import { api, type Preset, type SequenceProject, type Status } from "$lib/api";
  import { i18n, t, type Key } from "$lib/i18n/index.svelte";
  import MotionPanel from "$lib/MotionPanel.svelte";
  import StudioWorkspace from "$lib/studio/StudioWorkspace.svelte";

  type MainView = "home" | "framings" | "studio" | "library" | "community" | "settings";
  type Category = Preset["category"];

  let activeView = $state<MainView>("studio");
  let status = $state<Status>({
    connected: false,
    open: false,
    anchored: false,
    anchor: null,
    camera: null,
  });
  let presets = $state<Preset[]>([]);
  let sequences = $state<SequenceProject[]>([]);
  let activeProject = $state<SequenceProject>({
    schemaVersion: 2,
    id: "seed-arco-heroico",
    name: "Arco Heroico Cinematográfico",
    duration: 10,
    durationUnit: "seconds",
    durationInputMode: "valueUnit",
    space: "anchor",
    anchorSnapshot: { eye: [0, 1.65, 0], yaw: 0 },
    trajectoryScale: 1,
    target: null,
    cinematic: {
      profile: "cinematic",
      inertiaTau: 0.18,
      maxLinearSpeed: 4.5,
      maxLinearAccel: 8.0,
      maxAngularSpeed: 180.0,
      organicVariation: 0.0,
      organicSeed: 42,
    },
    calibration: {
      enabled: true,
      autoCorrect: false,
      posOffset: [0, 0, 0],
      yawOffsetDeg: 0,
      pitchOffsetDeg: 0,
      compositionX: 0,
      compositionY: 0,
      leadRoom: 0,
    },
    keyframes: [
      {
        id: "kf-a",
        label: "Punto A",
        time: 0,
        rawPose: { pos: [-1.2, 1.3, 1.8], rot: [-11, 146, 0] },
        localPose: { pos: [-1.2, -0.35, 1.8], rot: [-11, 146, 0] },
        capturedAtMs: 0,
        transition: "smooth",
        knotMode: "stop",
        look: { zoom: 38, exposure: null, focalDistance: null, aperture: null },
      },
      {
        id: "kf-b",
        label: "Punto B",
        time: 5,
        rawPose: { pos: [0, 1.7, 1.35], rot: [2, 180, 0] },
        localPose: { pos: [0, 0.05, 1.35], rot: [2, 180, 0] },
        capturedAtMs: 0,
        transition: "smooth",
        knotMode: "passThrough",
        look: { zoom: 54, exposure: null, focalDistance: null, aperture: null },
      },
      {
        id: "kf-c",
        label: "Punto C",
        time: 10,
        rawPose: { pos: [1.2, 1.9, 1.9], rot: [7.5, -148, 0] },
        localPose: { pos: [1.2, 0.25, 1.9], rot: [7.5, -148, 0] },
        capturedAtMs: 0,
        transition: "smooth",
        knotMode: "stop",
        look: { zoom: 45, exposure: null, focalDistance: null, aperture: null },
      },
    ],
    effects: [
      {
        id: "ef-1",
        kind: "cameraBreathing",
        track: 0,
        startTime: 0,
        duration: 10,
        intensity: 0.8,
        speed: 1,
        paramA: null,
        paramB: null,
        easing: "smooth",
        enabled: true,
      },
    ],
    rawRecording: [],
  });

  let message = $state<{ text: string; ok: boolean } | null>(null);
  let selectedCategory = $state<"all" | Category>("all");
  let motionCount = $state(0);
  let collapsed = $state(false);
  let narrow = $state(false);
  let importJsonText = $state("");
  const rail = $derived(collapsed || narrow);

  const navItems: { id: MainView; key: Key; icon: typeof Home }[] = [
    { id: "home", key: "nav.home", icon: Home },
    { id: "framings", key: "nav.framings", icon: Camera },
    { id: "studio", key: "nav.studio", icon: Film },
    { id: "library", key: "nav.library", icon: BookOpen },
    { id: "community", key: "nav.community", icon: Globe },
    { id: "settings", key: "nav.settings", icon: Settings },
  ];

  const categories: Category[] = ["retrato", "cuerpo", "ambiente", "movimiento", "video", "ref_sheet"];
  const catIcons = {
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
      .filter((c) => selectedCategory === "all" || c === selectedCategory)
      .map((c) => ({ c, items: presets.filter((p) => p.category === c) }))
      .filter((g) => g.items.length),
  );

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

  async function copySequenceJson(seq: SequenceProject) {
    const raw = JSON.stringify(seq, null, 2);
    await navigator.clipboard.writeText(raw);
    message = { text: `JSON de «${seq.name}» copiado al portapapeles.`, ok: true };
  }

  async function copyDollyJson(seq: SequenceProject, bakeEffects: boolean) {
    await run(async () => {
      const raw = await api.exportVrcDolly(seq, bakeEffects);
      await navigator.clipboard.writeText(raw);
    }, `Trayectoria Camera Dolly de «${seq.name}» copiada al portapapeles.`);
  }

  async function handleImportSequence() {
    if (!importJsonText.trim()) return;
    await run(async () => {
      const imported = await api.importSequence(importJsonText);
      const lib = await api.listSequences();
      sequences = lib.sequences;
      activeProject = structuredClone(imported);
      importJsonText = "";
      activeView = "studio";
    }, "Secuencia importada y abierta en Iris Studio.");
  }

  async function handleImportDolly() {
    if (!importJsonText.trim()) return;
    await run(async () => {
      const imported = await api.importVrcDolly(importJsonText);
      const lib = await api.listSequences();
      sequences = lib.sequences;
      activeProject = structuredClone(imported);
      importJsonText = "";
      activeView = "studio";
    }, "Trayectoria Camera Dolly importada y lista para reproducirse en Iris Studio sin VRC+.");
  }

  async function handleDeleteSequence(id: string) {
    await run(async () => {
      await api.deleteSequence(id);
      const lib = await api.listSequences();
      sequences = lib.sequences;
      if (activeProject.id === id && sequences.length > 0) {
        activeProject = structuredClone(sequences[0]);
      }
    }, "Secuencia eliminada.");
  }

  onMount(() => {
    run(async () => {
      const [presetLib, seqLib] = await Promise.all([api.listPresets(), api.listSequences()]);
      presets = presetLib.presets;
      sequences = seqLib.sequences;
      if (seqLib.sequences.length > 0) {
        activeProject = structuredClone(seqLib.sequences[0]);
      }
      const allWarnings = [...presetLib.warnings, ...seqLib.warnings];
      if (allWarnings.length) throw allWarnings.join("\n");
    });
    api.listMotions().then((m) => (motionCount = m.length + 1)).catch(() => {});
    tick();
    const id = setInterval(tick, 750);
    const mq = window.matchMedia("(max-width: 860px)");
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
  <!-- BARRA LATERAL PRINCIPAL (6 SECCIONES + COLAPSABLE) -->
  <nav class="sidebar" aria-label={t("app.title")}>
    <div class="brand-wrap">
      <h1 class="brand">
        <Aperture size={22} />
        <span class="text">{t("app.title")}</span>
      </h1>
    </div>

    <ul class="nav-list">
      {#each navItems as item (item.id)}
        {@const Icon = item.icon}
        <li>
          <button
            type="button"
            class="item"
            aria-current={activeView === item.id ? "page" : undefined}
            title={t(item.key)}
            onclick={() => (activeView = item.id)}
          >
            <Icon size={18} />
            <span class="text">{t(item.key)}</span>
            {#if item.id === "studio"}
              <span class="pill-new text">Studio</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    <div class="sidebar-footer">
      <button type="button" class="item lang" title={t("lang.switch")} onclick={() => i18n.toggle()}>
        <Languages size={18} /> <span class="text">{t("lang.switch")}</span>
      </button>

      {#if !narrow}
        {@const Chevron = rail ? ChevronsRight : ChevronsLeft}
        <button
          type="button"
          class="item"
          aria-expanded={!rail}
          title={t(rail ? "nav.expand" : "nav.collapse")}
          onclick={() => (collapsed = !collapsed)}
        >
          <Chevron size={18} /> <span class="text">{t("nav.collapse")}</span>
        </button>
      {/if}
    </div>
  </nav>

  <!-- CONTENIDO PRINCIPAL -->
  <main class:wide={activeView === "studio"}>
    <!-- Cabecera global compacta con indicadores de Conexión, Cámara, Lectura de Pose y Calibración -->
    <header class="global-status-bar">
      <div class="chips" aria-live="polite">
        <span class="chip" class:on={status.connected} title="Estado de conexión OSCQuery con VRChat">
          <Plug size={15} /> {t(status.connected ? "status.connected" : "status.disconnected")}
        </span>
        <span class="chip" class:on={status.open} title="Estado de apertura de la cámara en VRChat">
          <Camera size={15} /> {t(status.open ? "status.open" : "status.closed")}
        </span>
        <span
          class="chip"
          class:on={Boolean(status.camera?.poseFresh)}
          title="Indica si VRChat ya emitió la posición exacta actual de tu cámara"
        >
          <Activity size={15} />
          {t(status.camera?.poseFresh ? "status.poseFresh" : "status.poseStale")}
        </span>
        <span class="chip" class:on={status.anchored} title="Origen relativo para presets y secuencias portables">
          <Crosshair size={15} /> {t(status.anchored ? "status.anchored" : "status.unanchored")}
        </span>
      </div>

      <div class="quick-top-actions">
        <button
          type="button"
          class="primary sm"
          disabled={!status.connected || !status.open}
          title={t("calibrate.help")}
          onclick={() => run(() => api.calibrate().then(tick), t("calibrate.done"))}
        >
          <Crosshair size={15} /> {t("calibrate.button")}
        </button>
        <button
          type="button"
          class="ghost sm"
          disabled={!status.connected}
          title={t("reset.help")}
          onclick={() => run(() => api.resetDefaults(), t("reset.done"))}
        >
          <RotateCcw size={15} /> {t("reset.button")}
        </button>
      </div>
    </header>

    {#if !status.connected}
      <p class="banner">{t("banner.disconnected")}</p>
    {:else if !status.open}
      <p class="banner">{t("banner.closed")}</p>
    {/if}

    {#if message}
      <p class="msg" class:ok={message.ok} role="status">{message.text}</p>
    {/if}

    <!-- =====================================================================
         VISTA 1: INICIO (HOME)
         ===================================================================== -->
    {#if activeView === "home"}
      <section class="hero card">
        <div>
          <span class="eyebrow"><Sparkles size={14} /> VELZA IRIS</span>
          <h2>{t("home.welcome")}</h2>
          <p class="sub">{t("home.subtitle")}</p>
        </div>
        <div class="hero-actions">
          <button type="button" class="primary" onclick={() => (activeView = "studio")}>
            <Film size={16} /> {t("home.openStudio")}
          </button>
          <button type="button" class="ghost" onclick={() => (activeView = "framings")}>
            <Camera size={16} /> {t("home.openFramings")}
          </button>
        </div>
      </section>

      <div class="steps-grid">
        <article class="card step-card">
          <h4>{t("home.step1Title")}</h4>
          <p>{t("home.step1Desc")}</p>
        </article>
        <article class="card step-card">
          <h4>{t("home.step2Title")}</h4>
          <p>{t("home.step2Desc")}</p>
        </article>
        <article class="card step-card">
          <h4>{t("home.step3Title")}</h4>
          <p>{t("home.step3Desc")}</p>
        </article>
      </div>

      <div class="home-cards-two">
        <article class="card feature-card">
          <h3><Film size={18} /> {t("home.quickStudioTitle")}</h3>
          <p>{t("home.quickStudioDesc")}</p>
          <button type="button" class="primary sm" onclick={() => (activeView = "studio")}>
            {t("home.openStudio")}
          </button>
        </article>

        <article class="card feature-card">
          <h3><Camera size={18} /> {t("home.quickFramingsTitle")}</h3>
          <p>{t("home.quickFramingsDesc")}</p>
          <button type="button" class="ghost sm" onclick={() => (activeView = "framings")}>
            {t("home.openFramings")}
          </button>
        </article>
      </div>

    <!-- =====================================================================
         VISTA 2: ENCUADRES (CATÁLOGO DE RETRATO, CUERPO, AMBIENTE, MOVIMIENTO, VIDEO, REF SHEET)
         ===================================================================== -->
    {:else if activeView === "framings"}
      <section class="cat-filter-bar" aria-label={t("presets.title")}>
        {#each ["all", ...categories] as const as c (c)}
          {@const Icon = catIcons[c]}
          <button
            type="button"
            class="cat-pill"
            class:active={selectedCategory === c}
            onclick={() => (selectedCategory = c)}
          >
            <Icon size={15} />
            <span>{label(c)}</span>
            <small>{count(c)}</small>
          </button>
        {/each}
      </section>

      {#each groups as g (g.c)}
        {#if selectedCategory === "all"}<h3>{label(g.c)}</h3>{/if}
        <div class="grid">
          {#each g.items as p (p.name)}
            <article class="card preset">
              <div class="preset-top">
                <h4>{p.name}</h4>
                {#if p.modifier}
                  <span class="tag-badge">{t("presets.modifier")}</span>
                {/if}
              </div>
              <p class="meta">
                {#if p.look.zoom != null}<span>{t("presets.mm", { mm: p.look.zoom })}</span>{/if}
                {#if p.look.aperture != null}<span>{t("presets.aperture", { value: p.look.aperture })}</span>{/if}
                {#if p.look.focus}<span>{t(`focus.${p.look.focus}` as Key)}</span>{/if}
                {#if p.modifier && p.look.exposure != null}
                  <span>{t("presets.ev", { value: (p.look.exposure > 0 ? "+" : "") + p.look.exposure })}</span>
                {/if}
              </p>
              <button
                type="button"
                class="primary sm"
                disabled={!ready(p)}
                title={p.frame && !status.anchored ? t("presets.needAnchor") : "Aplicar encuadre a la cámara de VRChat"}
                onclick={() => run(() => api.applyPreset(p.name))}
              >
                {t("presets.apply")}
              </button>
            </article>
          {/each}
        </div>
      {/each}

      {#if groups.length === 0 && selectedCategory !== "movimiento"}
        <p class="empty-state">{t("presets.empty")}</p>
      {/if}

      {#if selectedCategory === "all" || selectedCategory === "movimiento"}
        <MotionPanel ready={status.connected && status.open && status.anchored} {run} />
      {/if}

    <!-- =====================================================================
         VISTA 3: IRIS STUDIO (MODO CAPTURA Y MODO EDITOR COMPARTIDOS)
         ===================================================================== -->
    {:else if activeView === "studio"}
      <StudioWorkspace {status} bind:sequences bind:activeProject {run} />

    <!-- =====================================================================
         VISTA 4: BIBLIOTECA (SECUENCIAS PORTABLES Y PRESETS)
         ===================================================================== -->
    {:else if activeView === "library"}
      <section class="lib-section">
        <div class="sec-head">
          <h2>{t("library.sequences")}</h2>
          <span class="sub">Archivos guardados en `VelzaIris-data/sequences/`</span>
        </div>

        <div class="grid">
          {#each sequences as seq (seq.id)}
            <article class="card seq-card">
              <h4>{seq.name}</h4>
              <p class="meta">
                <span>
                  {seq.durationUnit === "minutes"
                    ? `${Math.round((seq.duration / 60) * 1000) / 1000} min (${seq.duration} s)`
                    : `${seq.duration} s`}
                </span>
                <span>{seq.keyframes.length} puntos</span>
                <span>{seq.effects.length} efectos</span>
                <span class="tag">
                  {seq.space === "anchor" ? "Portable (Ancla)" : "Espacio de Mundo"}
                </span>
              </p>
              <div class="card-actions">
                <button
                  type="button"
                  class="primary sm"
                  onclick={() => {
                    activeProject = structuredClone(seq);
                    activeView = "studio";
                  }}
                >
                  <Film size={14} /> {t("library.openInStudio")}
                </button>
                <button type="button" class="ghost sm" onclick={() => copySequenceJson(seq)}>
                  <Copy size={14} /> {t("library.exportJson")}
                </button>
                <button
                  type="button"
                  class="ghost sm"
                  title="Exporta esta secuencia al formato JSON de Camera Dolly de VRChat (con efectos horneados)"
                  onclick={() => copyDollyJson(seq, true)}
                >
                  <Copy size={14} /> Dolly JSON
                </button>
                {#if !seq.id.startsWith("seed-")}
                  <button
                    type="button"
                    class="ghost sm"
                    onclick={() => handleDeleteSequence(seq.id)}
                    title={t("library.delete")}
                  >
                    <Trash2 size={14} />
                  </button>
                {/if}
              </div>
            </article>
          {/each}
        </div>

        <div class="card import-box">
          <h4><Upload size={15} /> {t("library.importJson")} / VRChat Camera Dolly</h4>
          <textarea
            rows="3"
            placeholder="Pega aquí el JSON de una secuencia de Iris Studio o un archivo .json de VRChat Camera Dolly (CameraPaths)…"
            bind:value={importJsonText}
          ></textarea>
          <div class="card-actions">
            <button
              type="button"
              class="primary sm"
              disabled={!importJsonText.trim()}
              onclick={handleImportSequence}
            >
              <Upload size={14} /> {t("library.importJson")}
            </button>
            <button
              type="button"
              class="ghost sm"
              disabled={!importJsonText.trim()}
              title="Importa una trayectoria Camera Dolly oficial de VRChat para reproducirla en Velza Iris sin VRC+"
              onclick={handleImportDolly}
            >
              <Upload size={14} /> Importar Camera Dolly (Sin VRC+)
            </button>
          </div>
        </div>
      </section>

    <!-- =====================================================================
         VISTA 5: COMUNIDAD (ECOSISTEMA PORTABLE Y FUTURO ASSET)
         ===================================================================== -->
    {:else if activeView === "community"}
      <section class="card community-card">
        <span class="eyebrow"><Globe size={14} /> ECOSISTEMA VELZA IRIS</span>
        <h2>{t("community.title")}</h2>
        <p class="sub">{t("community.subtitle")}</p>
        <p>{t("community.desc")}</p>

        <div class="steps-grid">
          <article class="card step-card">
            <h4>1. Presets Relativos (`Anchor Space`)</h4>
            <p>
              Las secuencias guardan desplazamientos relativos a tu ancla calibrada (`world_to_local`),
              permitiendo reutilizar el mismo movimiento en otros mundos tras pulsar «Calibrar ancla».
            </p>
          </article>
          <article class="card step-card">
            <h4>2. Sin requisito de VRC+</h4>
            <p>
              Cualquier usuario puede reproducir secuencias compartidas mediante el motor directo de
              60 Hz sobre `/usercamera/Pose`.
            </p>
          </article>
          <article class="card step-card">
            <h4>3. Preparado para Asset Complementario</h4>
            <p>
              El esquema `SequenceProject v1` incluye metadatos de espacio, escala y `TargetProvider`
              listos para el futuro prefab de visualización espacial en VRChat.
            </p>
          </article>
        </div>
      </section>

    <!-- =====================================================================
         VISTA 6: AJUSTES Y DIAGNÓSTICO OSC
         ===================================================================== -->
    {:else if activeView === "settings"}
      <div class="settings-stack">
        <section class="card anchor-row">
          <div>
            <h3>{t("settings.language")}</h3>
            <p class="sub">Alterna todos los textos, ayudas y nombres de efectos entre Español e Inglés.</p>
          </div>
          <button type="button" class="primary sm" onclick={() => i18n.toggle()}>
            <Languages size={15} /> {t("lang.switch")}
          </button>
        </section>

        <section class="card">
          <h3>{t("settings.oscTitle")}</h3>
          <div class="diag-grid">
            <div>
              <span class="diag-k">Estado OSCQuery:</span>
              <strong>{status.connected ? "Conectado (UDP 9000 / Escucha 9001)" : "Sin conexión"}</strong>
            </div>
            <div>
              <span class="diag-k">Paquetes `/usercamera/Pose` recibidos:</span>
              <strong>{status.camera?.poseSeq ?? 0}</strong>
            </div>
            <div>
              <span class="diag-k">Frescura de Pose (`poseFresh`):</span>
              <strong>{status.camera?.poseFresh ? "Sí (lectura verificada)" : "No (esperando movimiento)"}</strong>
            </div>
            <div>
              <span class="diag-k">Última Pose cruda recibida:</span>
              <code>
                {status.camera?.pose
                  ? `XYZ [${status.camera.pose.pos.map((v) => v.toFixed(3)).join(", ")}] · PYR [${status.camera.pose.rot.map((v) => v.toFixed(1)).join("°, ")}°]`
                  : "Ninguna"}
              </code>
            </div>
          </div>
        </section>

        <section class="card">
          <h3>{t("settings.companionTitle")}</h3>
          <p class="sub">
            Iris Studio separa estrictamente las capacidades verificadas hoy por OSC directo de las
            extensiones experimentales basadas en consentimiento:
          </p>
          <ul class="cap-list">
            <li>
              <strong>Viable y verificada hoy:</strong> Captura y reproducción 6DoF a 60 Hz por
              `/usercamera/Pose`, óptica (`/usercamera/Zoom`, `Exposure`, `FocalDistance`, `Aperture`),
              espacio relativo por Ancla Calibrada y `TargetProvider` de punto estático calibrado.
            </li>
            <li>
              <strong>Experimental (Fase 9–10):</strong> Prefab de Avatar OSC y puente Udon HTTP local
              (`VRCStringDownloader`) para visualización de líneas en mundo y seguimiento voluntario.
            </li>
          </ul>
        </section>
      </div>
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
    padding: 20px 12px;
    background: rgba(26, 18, 48, 0.82);
    border-right: 1px solid rgba(184, 169, 255, 0.12);
    backdrop-filter: blur(10px);
    z-index: 10;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 6px;
    font-family: "Fraunces Variable", Georgia, serif;
    font-weight: 500;
    font-size: 1.3rem;
    color: var(--lavender);
  }
  .nav-list {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }
  .sidebar-footer {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 9px 12px;
    background: transparent;
    color: var(--text-2);
    border: 1px solid transparent;
    text-align: left;
    border-radius: 11px;
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
  .item .text {
    flex: 1;
  }
  .pill-new {
    flex: 0 !important;
    font-size: 0.66rem;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(61, 90, 254, 0.35);
    color: var(--lavender);
    border: 1px solid rgba(184, 169, 255, 0.25);
  }

  main {
    min-width: 0;
    max-width: 1060px;
    width: 100%;
    margin: 0 auto;
    padding: 18px 22px 36px;
  }
  main.wide {
    max-width: 1440px;
  }

  .global-status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    margin-bottom: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 11px;
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 0.8rem;
    border: 1px solid rgba(184, 169, 255, 0.1);
  }
  .chip.on {
    color: var(--text);
    border-color: var(--accent);
    box-shadow: 0 0 14px rgba(61, 90, 254, 0.22);
  }
  .quick-top-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  button.sm {
    padding: 6px 12px;
    font-size: 0.82rem;
    border-radius: 10px;
  }

  .banner,
  .msg {
    margin: 0 0 12px;
    padding: 10px 14px;
    border-radius: 12px;
    background: var(--velvet);
    font-size: 0.86rem;
    white-space: pre-line;
  }
  .msg {
    border: 1px solid #c85a7a;
  }
  .msg.ok {
    border-color: var(--lavender);
  }

  /* Inicio */
  .hero {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 18px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }
  .eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.74rem;
    letter-spacing: 0.08em;
    color: var(--lavender);
    font-weight: 600;
  }
  .hero h2 {
    margin: 4px 0 6px;
    font-size: 1.35rem;
  }
  .sub {
    margin: 0;
    color: var(--text-2);
    font-size: 0.88rem;
    max-width: 65ch;
  }
  .hero-actions {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .steps-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
    gap: 12px;
    margin: 14px 0;
  }
  .step-card p {
    margin: 0;
    font-size: 0.84rem;
    color: var(--text-2);
  }
  .home-cards-two {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(290px, 1fr));
    gap: 14px;
  }
  .feature-card h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 6px;
    font-size: 1rem;
    color: var(--text);
    text-transform: none;
    letter-spacing: 0;
  }
  .feature-card p {
    margin: 0 0 12px;
    font-size: 0.86rem;
    color: var(--text-2);
  }

  /* Encuadres */
  .cat-filter-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 14px;
  }
  .cat-pill {
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    border: 1px solid rgba(184, 169, 255, 0.12);
    font-size: 0.82rem;
  }
  .cat-pill.active {
    background: var(--velvet);
    color: var(--text);
    border-color: var(--accent);
  }
  .cat-pill small {
    opacity: 0.7;
  }
  h3 {
    margin: 18px 0 8px;
    font-size: 0.78rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--lavender);
  }
  .card.preset {
    padding: 14px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
  }
  .card.preset:hover {
    border-color: var(--lavender);
    transform: translateY(-1px);
  }
  .preset-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }
  .tag-badge {
    font-size: 0.7rem;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(184, 169, 255, 0.14);
    color: var(--lavender);
  }

  /* Biblioteca, Comunidad y Ajustes */
  .sec-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .sec-head h2 {
    margin: 0;
    font-size: 1.15rem;
  }
  .card-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .import-box {
    margin-top: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .import-box h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }
  textarea {
    width: 100%;
    background: #0e0a1a;
    color: var(--text);
    border: 1px solid rgba(184, 169, 255, 0.2);
    border-radius: 10px;
    padding: 10px;
    font-family: monospace;
    font-size: 0.8rem;
  }
  .settings-stack {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .anchor-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .diag-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
    margin-top: 10px;
    font-size: 0.84rem;
  }
  .diag-k {
    display: block;
    color: var(--text-2);
    font-size: 0.76rem;
  }
  .cap-list {
    margin: 10px 0 0;
    padding-left: 18px;
    color: var(--text-2);
    font-size: 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  footer {
    margin-top: 32px;
    color: var(--text-2);
    font-size: 0.78rem;
    text-align: center;
  }

  .rail .sidebar {
    padding: 20px 8px;
    align-items: center;
  }
  .rail .text {
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
