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
    Eye,
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
    Radio,
    RotateCcw,
    Settings,
    Sparkles,
    Trash2,
    Upload,
    UserRound,
    Waves,
    Monitor,
    Moon,
    Sun,
    Palette,
    Keyboard,
    Download,
    FolderOpen,
    FileCode,
  } from "@lucide/svelte";
  import { api, type Preset, type SequenceProject, type Status, type VisualizerSnapshot } from "$lib/api";
  import { i18n, t, pointLabel, localizeError, localizePresetName, type Key } from "$lib/i18n/index.svelte";
  import { createDefaultSequenceProject } from "$lib/studio/catalog";
  import MotionPanel from "$lib/MotionPanel.svelte";
  import StudioWorkspace from "$lib/studio/StudioWorkspace.svelte";
  import IrisSelect, { type SelectOption } from "$lib/components/ui/IrisSelect.svelte";
  import KeybindingsModal from "$lib/components/ui/KeybindingsModal.svelte";
  import { themeManager, ACCENT_PRESETS, type ThemeMode, type AccentPreset } from "$lib/theme/theme.svelte";

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
  let activeProject = $state<SequenceProject>(createDefaultSequenceProject("Arco Heroico Cinematográfico"));

  let message = $state<{ text: string; ok: boolean } | null>(null);
  let visualizerSnapshot = $state<VisualizerSnapshot | null>(null);

  async function pollVisualizerStatus() {
    try {
      visualizerSnapshot = await api.visualizerStatus();
    } catch {
      /* inicializando */
    }
  }

  $effect(() => {
    if (activeView === "settings") {
      void pollVisualizerStatus();
      const interval = setInterval(pollVisualizerStatus, 1000);
      return () => clearInterval(interval);
    }
  });

  async function handleVisualizerActivate() {
    try {
      await api.visualizerActivate();
      await pollVisualizerStatus();
      message = { text: "Gizmo encendido: sigue a la cámara de VRChat sin calibrar", ok: true };
    } catch (e: any) {
      message = { text: String(e), ok: false };
    }
  }

  // Ajuste fino del ancla: fino = 1 cm / 1°, grueso = 10 cm / 10°.
  let nudgeCoarse = $state(false);
  async function handleNudge(dx: number, dy: number, dz: number, dyaw: number) {
    const k = nudgeCoarse ? 10 : 1;
    try {
      await api.visualizerNudge(dx * 0.01 * k, dy * 0.01 * k, dz * 0.01 * k, dyaw * k);
      await pollVisualizerStatus();
    } catch (e: any) {
      message = { text: String(e), ok: false };
    }
  }

  async function handleResetNudge() {
    try {
      await api.visualizerResetNudge();
      await pollVisualizerStatus();
    } catch (e: any) {
      message = { text: String(e), ok: false };
    }
  }

  /** Eco que VRChat devolvió de un parámetro iris_* frente a lo último que enviamos (misma escala normalizada). */
  function echoFor(name: string) {
    return visualizerSnapshot?.diag.avatarEcho.find((e) => e.name === name) ?? null;
  }
  const camParamNames = ["iris_cam_x", "iris_cam_y", "iris_cam_z", "iris_cam_rx", "iris_cam_ry", "iris_cam_rz"];
  function sentFor(i: number): number | null {
    const v = i < 3 ? visualizerSnapshot?.lastCamNormalizedPos : visualizerSnapshot?.lastCamNormalizedRot;
    return v ? v[i % 3] : null;
  }

  async function handleVisualizerToggleHide() {
    try {
      await api.visualizerSetHideCam(!visualizerSnapshot?.hideCam);
      await pollVisualizerStatus();
    } catch (e: any) {
      message = { text: String(e), ok: false };
    }
  }

  async function handleVisualizerResetAnchor() {
    try {
      await api.visualizerResetAnchor();
      await pollVisualizerStatus();
      message = { text: "Gizmo apagado", ok: true };
    } catch (e: any) {
      message = { text: String(e), ok: false };
    }
  }

  async function handleVisualizerSyncPath() {
    try {
      await api.visualizerSyncPath(activeProject);
      await pollVisualizerStatus();
      message = { text: "Sincronización del Path de 5 huesos encolada (Flujo B)", ok: true };
    } catch (e: any) {
      message = { text: String(e), ok: false };
    }
  }
  let selectedCategory = $state<"all" | Category>("all");
  let motionCount = $state(0);
  let collapsed = $state(false);
  let narrow = $state(false);
  let importJsonText = $state("");
  let showKeybindingsModal = $state(false);
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

  const themeModeOptions = $derived<SelectOption<ThemeMode>[]>([
    { value: "system", label: t("theme.mode.system.label"), icon: Monitor, description: t("theme.mode.system.desc") },
    { value: "dark", label: t("theme.mode.dark.label"), icon: Moon, description: t("theme.mode.dark.desc") },
    { value: "light", label: t("theme.mode.light.label"), icon: Sun, description: t("theme.mode.light.desc") },
  ]);

  const accentPresetOptions = $derived<SelectOption<AccentPreset>[]>(
    Object.values(ACCENT_PRESETS).map((preset) => ({
      value: preset.id,
      label: t(preset.labelKey),
      icon: Sparkles,
      description: preset.hex,
    }))
  );

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
      message = { text: localizeError(e), ok: false };
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

  async function toggleCamera(name: string, current: boolean | undefined | null) {
    const next = !(current ?? false);
    await run(async () => {
      await api.setCameraToggle(name, next);
      await tick();
    });
  }

  const ready = (p: Preset) => status.connected && status.open && (p.frame === null || status.anchored);
  const label = (c: "all" | Category) => (c === "all" ? t("nav.all") : t(`cat.${c}` as Key));

  async function copySequenceJson(seq: SequenceProject) {
    const raw = JSON.stringify(seq, null, 2);
    await navigator.clipboard.writeText(raw);
    message = { text: t("toast.jsonCopied", { name: seq.name }), ok: true };
  }

  async function copyDollyJson(seq: SequenceProject, bakeEffects: boolean) {
    await run(async () => {
      const raw = await api.exportVrcDolly(seq, bakeEffects);
      await navigator.clipboard.writeText(raw);
    }, t("toast.dollyCopied", { name: seq.name }));
  }

  function downloadJsonFile(filename: string, content: string) {
    const blob = new Blob([content], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  const slugFilename = (name: string, ext: string) =>
    `${name.toLowerCase().replace(/[^a-z0-9_-]/g, "_") || "sequence"}.${ext}`;

  function downloadSequenceFile(seq: SequenceProject) {
    const filename = slugFilename(seq.name, "iris.json");
    downloadJsonFile(filename, JSON.stringify(seq, null, 2));
    message = { text: t("toast.jsonCopied", { name: filename }), ok: true };
  }

  async function downloadDollyFile(seq: SequenceProject, bakeEffects: boolean) {
    await run(async () => {
      const raw = await api.exportVrcDolly(seq, bakeEffects);
      downloadJsonFile(slugFilename(seq.name, "vrc-dolly.json"), raw);
    }, t("toast.dollyCopied", { name: seq.name }));
  }

  let fileInputEl = $state<HTMLInputElement | null>(null);
  let isDraggingFile = $state(false);

  function handleFileRead(file: File) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const text = e.target?.result;
      if (typeof text === "string") {
        importJsonText = text;
        message = { text: t("library.fileLoaded", { name: file.name }), ok: true };
      }
    };
    reader.readAsText(file);
  }

  function handleFileInputChange(e: Event) {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (file) {
      handleFileRead(file);
      target.value = "";
    }
  }

  function handleFileDrop(e: DragEvent) {
    e.preventDefault();
    isDraggingFile = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) {
      handleFileRead(file);
    }
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
    }, t("toast.sequenceImported"));
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
    }, t("toast.dollyImported"));
  }

  async function handleDeleteSequence(id: string) {
    await run(async () => {
      await api.deleteSequence(id);
      const lib = await api.listSequences();
      sequences = lib.sequences;
      if (activeProject.id === id && sequences.length > 0) {
        activeProject = structuredClone(lib.sequences[0]);
      }
    }, t("toast.sequenceDeleted"));
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
      <span class="brand-version text">Velza Iris v0.1.0</span>
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
        <span class="chip" class:on={status.connected} title={t("status.tooltip.connected")}>
          <Plug size={15} /> {t(status.connected ? "status.connected" : "status.disconnected")}
        </span>
        <span class="chip" class:on={status.open} title={t("status.tooltip.camera")}>
          <Camera size={15} /> {t(status.open ? "status.open" : "status.closed")}
        </span>
        <span
          class="chip"
          class:on={Boolean(status.camera?.poseFresh)}
          title={t("status.tooltip.pose")}
        >
          <Activity size={15} />
          {t(status.camera?.poseFresh ? "status.poseFresh" : "status.poseStale")}
        </span>
        <span
          class="chip"
          class:on={status.anchored}
          title="{t('status.tooltip.anchor')}{status.anchorState ? ` · ${t(`anchor.state.${status.anchorState.state}`)}` : ''}"
        >
          <Crosshair size={15} />
          {status.anchorState && (status.anchorState.state === "acquiring" || status.anchorState.state === "syncLost" || status.anchorState.state === "recovering")
            ? t(`anchor.state.${status.anchorState.state}`)
            : t(status.anchored ? "status.anchored" : "status.unanchored")}
        </span>
        {#if status.open}
          <button
            type="button"
            class="chip chip-btn"
            class:on={Boolean(status.camera?.streaming)}
            title={t("camera.toggle.streamingHelp")}
            onclick={() => toggleCamera("Streaming", status.camera?.streaming)}
          >
            <Radio size={14} /> {t("camera.toggle.streaming")}
          </button>
          <button
            type="button"
            class="chip chip-btn"
            class:on={Boolean(status.camera?.lookAtMe)}
            title={t("camera.toggle.lookAtMeHelp")}
            onclick={() => toggleCamera("LookAtMe", status.camera?.lookAtMe)}
          >
            <Eye size={14} /> {t("camera.toggle.lookAtMe")}
          </button>
          <button
            type="button"
            class="chip chip-btn"
            class:on={Boolean(status.camera?.smoothMovement)}
            title={t("camera.toggle.smoothHelp")}
            onclick={() => toggleCamera("SmoothMovement", status.camera?.smoothMovement)}
          >
            <Waves size={14} /> {t("camera.toggle.smooth")}
          </button>
        {/if}
      </div>

      <div class="quick-top-actions">
        <button
          type="button"
          class="primary sm"
          disabled={!status.connected || !status.open}
          title={t("calibrate.help")}
          onclick={() =>
            run(
              () =>
                api
                  .calibrate()
                  .then(() => api.visualizerSyncPath(activeProject).catch(() => {}))
                  .then(tick),
              t("calibrate.done"),
            )}
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
    {:else if status.camera && status.camera.rxAvailable === false}
      <p class="banner">{t("banner.rxUnavailable")}</p>
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
                <h4>{localizePresetName(p.name)}</h4>
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
                title={p.frame && !status.anchored ? t("presets.needAnchor") : t("presets.applyTooltip")}
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
          <span class="sub">{t("library.sub")}</span>
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
                <span>{t("library.pointsCount", { count: seq.keyframes.length })}</span>
                <span>{t("library.effectsCount", { count: seq.effects.length })}</span>
                <span class="tag">
                  {seq.space === "anchor" ? t("library.spaceAnchor") : t("library.spaceWorld")}
                </span>
              </p>
              <div class="card-actions">
                <button
                  type="button"
                  class="primary sm"
                  onclick={() => {
                    activeProject = $state.snapshot(seq) as SequenceProject;
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
                  title={t("library.exportFile")}
                  onclick={() => downloadSequenceFile(seq)}
                >
                  <Download size={14} /> .iris.json
                </button>
                <button
                  type="button"
                  class="ghost sm"
                  title={t("library.dollyTooltip")}
                  onclick={() => copyDollyJson(seq, true)}
                >
                  <Copy size={14} /> Dolly JSON
                </button>
                <button
                  type="button"
                  class="ghost sm"
                  title={t("library.exportDollyFile")}
                  onclick={() => downloadDollyFile(seq, true)}
                >
                  <Download size={14} /> Dolly .json
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

        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="card import-box"
          class:is-dragging={isDraggingFile}
          ondragover={(e) => { e.preventDefault(); isDraggingFile = true; }}
          ondragleave={() => (isDraggingFile = false)}
          ondrop={handleFileDrop}
        >
          <div class="import-box-head">
            <h4><Upload size={15} /> {t("library.importJson")} / VRChat Camera Dolly</h4>
            <div class="import-head-actions">
              <input
                type="file"
                accept=".json,.iris"
                class="sr-only"
                bind:this={fileInputEl}
                onchange={handleFileInputChange}
              />
              <button
                type="button"
                class="ghost sm"
                onclick={() => fileInputEl?.click()}
                title={t("library.importFile")}
              >
                <FolderOpen size={14} /> {t("library.importFile")}
              </button>
              {#if importJsonText.trim()}
                <button
                  type="button"
                  class="ghost sm"
                  onclick={() => (importJsonText = "")}
                >
                  {t("curve.reset")}
                </button>
              {/if}
            </div>
          </div>

          <textarea
            rows={importJsonText.trim().length > 0 ? 12 : 3}
            class="import-json-textarea"
            placeholder={`${t("library.importPlaceholder")} (${t("library.dropFile")})`}
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
              title={t("library.importDollyTooltip")}
              onclick={handleImportDolly}
            >
              <Upload size={14} /> {t("library.importDollyBtn")}
            </button>
          </div>
        </div>
      </section>

    <!-- =====================================================================
         VISTA 5: COMUNIDAD (ECOSISTEMA PORTABLE Y FUTURO ASSET)
         ===================================================================== -->
    {:else if activeView === "community"}
      <section class="card community-card">
        <span class="eyebrow"><Globe size={14} /> {t("community.eyebrow")}</span>
        <h2>{t("community.title")}</h2>
        <p class="sub">{t("community.subtitle")}</p>
        <p>{t("community.desc")}</p>

        <div class="steps-grid">
          <article class="card step-card">
            <h4>{t("community.step1.title")}</h4>
            <p>{t("community.step1.desc")}</p>
          </article>
          <article class="card step-card">
            <h4>{t("community.step2.title")}</h4>
            <p>{t("community.step2.desc")}</p>
          </article>
          <article class="card step-card">
            <h4>{t("community.step3.title")}</h4>
            <p>{t("community.step3.desc")}</p>
          </article>
        </div>
      </section>

    <!-- =====================================================================
         VISTA 6: AJUSTES Y DIAGNÓSTICO OSC
         ===================================================================== -->
    {:else if activeView === "settings"}
      <div class="settings-stack">
        <!-- APARIENCIA, TEMA Y SELECTOR PRÉMIUM -->
        <section class="card theme-settings-card">
          <div class="theme-header-row">
            <div>
              <h3>{t("theme.title")}</h3>
              <p class="sub">{t("theme.subtitle")}</p>
            </div>
            <div class="theme-status-pill">
              <span class="status-indicator"></span>
              {themeManager.isDark ? t("theme.dark") : t("theme.light")} · {themeManager.mode === "system" ? t("theme.system") : t("theme.manual")}
            </div>
          </div>

          <div class="theme-grid">
            <div class="theme-field">
              <div class="theme-field-header">
                <span class="theme-field-title">{t("theme.mode.label")}</span>
                <span class="theme-field-sub">{t("theme.mode.sub")}</span>
              </div>
              <IrisSelect
                items={themeModeOptions}
                value={themeManager.mode}
                size="md"
                onchange={(m) => themeManager.setMode(m as ThemeMode)}
              />
            </div>

            <div class="theme-field">
              <div class="theme-field-header">
                <span class="theme-field-title">{t("theme.accent.label")}</span>
                <span class="theme-field-sub">{t("theme.accent.sub")}</span>
              </div>
              <IrisSelect
                items={accentPresetOptions}
                value={themeManager.accent}
                size="md"
                onchange={(a) => themeManager.setAccent(a as AccentPreset)}
              />
            </div>
          </div>

          <!-- Muestrario de paletas rápidas con swatches interactivos -->
          <div class="swatches-container">
            <span class="swatches-label">{t("theme.accent.quick")}</span>
            <div class="swatches-row">
              {#each Object.values(ACCENT_PRESETS) as preset (preset.id)}
                <button
                  type="button"
                  class="swatch-chip"
                  class:active={themeManager.accent === preset.id}
                  style="--accent-swatch: {preset.hex}"
                  title="{t(preset.labelKey)} ({preset.hex})"
                  onclick={() => themeManager.setAccent(preset.id)}
                >
                  <span class="swatch-dot"></span>
                  <span class="swatch-text">{t(preset.labelKey)}</span>
                </button>
              {/each}
            </div>
          </div>
        </section>

        <section class="card anchor-row">
          <div>
            <h3>{t("settings.language")}</h3>
            <p class="sub">{t("settings.langHelp")}</p>
          </div>
          <button type="button" class="primary sm" onclick={() => i18n.toggle()}>
            <Languages size={15} /> {t("lang.switch")}
          </button>
        </section>

        <section class="card anchor-row">
          <div>
            <h3>{t("settings.keybindings")}</h3>
            <p class="sub">{t("settings.keybindingsHelp")}</p>
          </div>
          <button type="button" class="primary sm" onclick={() => (showKeybindingsModal = true)}>
            <Keyboard size={15} /> {t("settings.openKeybindingsBtn")}
          </button>
        </section>

        <section class="card">
          <h3>{t("settings.oscTitle")}</h3>
          <div class="diag-grid">
            <div>
              <span class="diag-k">{t("settings.oscStatus")}</span>
              <strong>{status.connected ? t("settings.oscConnected") : t("settings.oscDisconnected")}</strong>
            </div>
            <div>
              <span class="diag-k">{t("settings.posePackets")}</span>
              <strong>{status.camera?.poseSeq ?? 0}</strong>
            </div>
            <div>
              <span class="diag-k">{t("settings.poseFreshness")}</span>
              <strong>{status.camera?.poseFresh ? t("settings.poseYes") : t("settings.poseNo")}</strong>
            </div>
            <div>
              <span class="diag-k">{t("settings.anchorState")}</span>
              <strong>
                {status.anchorState ? t(`anchor.state.${status.anchorState.state}`) : t("settings.none")}
                {#if status.anchorState?.provenance}
                  · {t(`anchor.prov.${status.anchorState.provenance}`)}
                {/if}
              </strong>
            </div>
            <div>
              <span class="diag-k">{t("settings.cameraMode")}</span>
              <strong>
                {status.anchorState?.cameraMode ?? status.camera?.mode ?? 0}
                {#if status.anchorState}
                  ({t("settings.anchorTransitions")}: {status.anchorState.modeSwitchCount} / {status.anchorState.recoveryCount})
                {/if}
              </strong>
            </div>
            <div>
              <span class="diag-k">{t("settings.lastPose")}</span>
              <code>
                {status.camera?.pose
                  ? `XYZ [${status.camera.pose.pos.map((v) => v.toFixed(3)).join(", ")}] · PYR [${status.camera.pose.rot.map((v) => v.toFixed(1)).join("°, ")}°]`
                  : t("settings.none")}
              </code>
            </div>
          </div>
        </section>

        <section class="card">
          <h3>{t("settings.companionTitle")}</h3>
          <p class="sub">{t("settings.companionDesc")}</p>
          <ul class="cap-list">
            <li>{t("settings.companionCurrent")}</li>
            <li>{t("settings.companionFuture")}</li>
          </ul>
        </section>

        <section class="card avatar-card">
          <div class="card-head-avatar">
            <div>
              <h3>Visualizador VRChat (Avatars 3.0 / Unity)</h3>
              <p class="sub">Rig de visualización de cámara y deformación de trayectoria de 5 huesos por OSC. «Mostrar gizmo» enciende el rig anclado al origen del mundo: el gizmo sigue a la cámara de VRChat sin calibrar y todos los jugadores lo ven en el mismo sitio. Si lo ves recorrido un poco, corrígelo con el ajuste fino. Al cambiar de avatar vuelve a pulsar «Mostrar gizmo».</p>
            </div>
            <span class="avatar-state-badge {visualizerSnapshot?.enabled ? 'ready' : 'idle'}">
              {visualizerSnapshot?.enabled ? "iris_enable = true" : "iris_enable = false"}
            </span>
          </div>

          <div class="diag-grid">
            <div>
              <span class="diag-k">Anclaje del rig:</span>
              <code>
                {#if visualizerSnapshot?.anchorPosition}
                  Origen del mundo (sin calibrar)
                {:else}
                  Apagado
                {/if}
              </code>
              {#if visualizerSnapshot?.lastCamNormalizedPos?.some((v) => Math.abs(v) >= 0.999)}
                <code>⚠ La cámara está a más de 50 m del origen del mundo: el gizmo se queda en el límite.</code>
              {/if}
            </div>
            <div>
              <span class="diag-k">Cámara OSC Normalizada (cam_x/y/z):</span>
              <code>
                {#if visualizerSnapshot?.lastCamNormalizedPos}
                  [{visualizerSnapshot.lastCamNormalizedPos[0].toFixed(3)}, {visualizerSnapshot.lastCamNormalizedPos[1].toFixed(3)}, {visualizerSnapshot.lastCamNormalizedPos[2].toFixed(3)}]
                {:else}
                  --
                {/if}
              </code>
            </div>
            <div>
              <span class="diag-k">Rotación OSC (cam_rx/ry/rz):</span>
              <code>
                {#if visualizerSnapshot?.lastCamNormalizedRot}
                  [{visualizerSnapshot.lastCamNormalizedRot[0].toFixed(3)}, {visualizerSnapshot.lastCamNormalizedRot[1].toFixed(3)}, {visualizerSnapshot.lastCamNormalizedRot[2].toFixed(3)}]
                {:else}
                  --
                {/if}
              </code>
            </div>
            <div>
              <span class="diag-k">Multiplexor de 5 Huesos (Path):</span>
              <code>
                {visualizerSnapshot?.isSyncingPath ? "Sincronizando (los params synced tardan ~3 s)..." : "Listo (iris_wp_index = 0)"}
              </code>
            </div>
            <div>
              <span class="diag-k">Estado del Sistema:</span>
              <code>{visualizerSnapshot?.lastError ? visualizerSnapshot.lastError : "OK (12 endpoints activos)"}</code>
            </div>
          </div>

          {#if visualizerSnapshot}
            {@const dg = visualizerSnapshot.diag}
            <div class="diag-grid">
              <div>
                <span class="diag-k">Sincronización con la cámara real:</span>
                <code>
                  ecos de pose {dg.poseEchoes} · enviados al rig {dg.framesSent} · omitidos: apagado {dg.skipped.disabled},
                  sin ancla {dg.skipped.noAnchor}, recorrido {dg.skipped.externalControl}, sin cambio {dg.skipped.deadband},
                  ocupado {dg.skipped.busy}
                </code>
                {#if dg.lastError}<code>último error: {dg.lastError}</code>{/if}
                {#if dg.poseEchoes === 0}
                  <code>⚠ VRChat no ha enviado ninguna pose: revisa que OSC esté habilitado (Menú de acción → Opciones → OSC → Enabled) y la cámara abierta.</code>
                {/if}
              </div>
              <div>
                <span class="diag-k">Eco de VRChat (enviado → eco):</span>
                <code>
                  {#each camParamNames as n, i}
                    {@const e = echoFor(n)}
                    {@const sent = sentFor(i)}
                    {n.replace("iris_cam_", "")}: {sent === null ? "--" : sent.toFixed(3)} → {e ? e.value.toFixed(3) : "sin eco"}{sent !== null && Math.abs(sent) >= 0.999 ? " (saturado: >50 m del ancla)" : ""}{e && sent !== null && Math.abs(e.value - sent) > 0.02 ? " ⚠" : ""}{i < 5 ? " · " : ""}
                  {/each}
                </code>
              </div>
              <div>
                <span class="diag-k">Ajuste fino del ancla (gizmo, ejes del rig):</span>
                <code>
                  x {visualizerSnapshot.nudgePos[0].toFixed(2)} m · y {visualizerSnapshot.nudgePos[1].toFixed(2)} m ·
                  z {visualizerSnapshot.nudgePos[2].toFixed(2)} m · giro {visualizerSnapshot.nudgeYaw.toFixed(0)}°
                </code>
              </div>
            </div>
            <div class="avatar-sim-controls">
              <span class="sim-label">Ajuste fino ({nudgeCoarse ? "10 cm / 10°" : "1 cm / 1°"}):</span>
              <div class="sim-buttons">
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(-1, 0, 0, 0)}>X −</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(1, 0, 0, 0)}>X +</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(0, -1, 0, 0)}>Y −</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(0, 1, 0, 0)}>Y +</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(0, 0, -1, 0)}>Z −</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(0, 0, 1, 0)}>Z +</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(0, 0, 0, -1)}>Giro −</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={() => handleNudge(0, 0, 0, 1)}>Giro +</button>
                <button type="button" class="btn-diag ghost" onclick={() => (nudgeCoarse = !nudgeCoarse)}>{nudgeCoarse ? "Paso fino" : "Paso grueso"}</button>
                <button type="button" class="btn-diag ghost" disabled={!visualizerSnapshot.anchorPosition} onclick={handleResetNudge}>Restablecer ajuste</button>
              </div>
            </div>
          {/if}

          <div class="avatar-sim-controls">
            <span class="sim-label">Acciones Cinemáticas OSC:</span>
            <div class="sim-buttons">
              <button
                type="button"
                class="btn-diag primary"
                disabled={!status.connected}
                onclick={handleVisualizerActivate}
                title="Vuelve a mandar iris_enable = true: úsalo si recargaste el avatar y no ves el gizmo"
              >
                {visualizerSnapshot?.enabled ? "Reenviar gizmo" : "Mostrar gizmo"}
              </button>
              <button type="button" class="btn-diag ghost" disabled={!status.connected} onclick={handleVisualizerToggleHide}>
                {visualizerSnapshot?.hideCam ? "Mostrar en cámaras de VRChat" : "Ocultar de cámaras de VRChat"}
              </button>
              <button type="button" class="btn-diag ghost" onclick={handleVisualizerSyncPath}>
                Sincronizar Path 5 Huesos (Flujo B)
              </button>
              <button type="button" class="btn-diag ghost" onclick={handleVisualizerResetAnchor}>
                Apagar gizmo
              </button>
            </div>
          </div>
        </section>
      </div>
    {/if}

    <footer>{t("footer.privacy")}</footer>
  </main>

  <KeybindingsModal bind:open={showKeybindingsModal} onclose={() => (showKeybindingsModal = false)} />
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
    background: var(--sidebar-bg, rgba(26, 18, 48, 0.85));
    border-right: 1px solid var(--sidebar-border, rgba(184, 169, 255, 0.12));
    backdrop-filter: blur(10px);
    z-index: 10;
    transition: background 200ms var(--ease), border-color 200ms var(--ease);
  }
  :global(:root[data-theme="light"]) .sidebar {
    background: var(--sidebar-bg, #f7f6fb);
    border-right-color: var(--sidebar-border, rgba(0, 0, 0, 0.08));
    box-shadow: 2px 0 16px rgba(0, 0, 0, 0.03);
  }
  :global(:root[data-theme="light"]) .brand {
    color: var(--royal);
  }
  :global(:root[data-theme="light"]) .item {
    color: #38334c;
    font-weight: 500;
  }
  :global(:root[data-theme="light"]) .item:hover {
    color: #12101a;
    background: rgba(0, 0, 0, 0.06);
  }
  :global(:root[data-theme="light"]) .item[aria-current="page"] {
    color: var(--royal);
    background: #ffffff;
    border-color: var(--accent);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.06);
    font-weight: 600;
  }
  :global(:root[data-theme="light"]) .pill-new {
    background: rgba(51, 82, 234, 0.12);
    color: var(--royal);
    border-color: rgba(51, 82, 234, 0.25);
  }
  .brand-wrap {
    display: flex;
    flex-direction: column;
    gap: 2px;
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
  .brand-version {
    font-size: 0.68rem;
    color: var(--text-muted);
    font-weight: 600;
    letter-spacing: 0.05em;
    padding-left: 38px;
    margin-top: -2px;
    user-select: none;
  }
  :global(:root[data-theme="light"]) .brand-version {
    color: var(--text-2);
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
  .chip-btn {
    cursor: pointer;
    font-family: inherit;
    transition: all 150ms ease;
  }
  .chip-btn:hover:not(:disabled) {
    border-color: var(--lavender);
    color: var(--text);
    background: rgba(184, 169, 255, 0.08);
  }
  .chip-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
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
    transition: border-color 150ms ease, box-shadow 150ms ease;
  }
  .import-box.is-dragging {
    border-color: var(--accent);
    box-shadow: 0 0 16px var(--accent-glow);
  }
  .import-box-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .import-box-head h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }
  .import-head-actions {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .import-json-textarea {
    width: 100%;
    background: var(--surface, #0e0a1a);
    color: var(--text);
    border: 1px solid rgba(184, 169, 255, 0.2);
    border-radius: 10px;
    padding: 10px 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.8rem;
    line-height: 1.45;
    transition: height 200ms var(--ease), border-color 150ms ease;
    resize: vertical;
  }
  :global(:root[data-theme="light"]) .import-json-textarea {
    background: #ffffff;
    border-color: rgba(0, 0, 0, 0.12);
    color: #12101a;
  }
  textarea {
    width: 100%;
    background: var(--surface, #0e0a1a);
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
  .theme-settings-card {
    display: flex;
    flex-direction: column;
    gap: 18px;
    background: var(--surface);
    border: 1px solid var(--select-border);
    border-radius: var(--radius);
    padding: 20px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
  }
  .theme-header-row {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    flex-wrap: wrap;
  }
  .theme-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: var(--select-surface-hover);
    border: 1px solid var(--select-border);
    border-radius: 999px;
    font-size: 0.78rem;
    font-weight: 500;
    color: var(--text-2);
  }
  .status-indicator {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent-color);
    box-shadow: 0 0 8px var(--accent-glow);
  }
  .theme-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 18px;
  }
  .theme-field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .theme-field-header {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .theme-field-title {
    font-size: 0.88rem;
    font-weight: 600;
    color: var(--text);
  }
  .theme-field-sub {
    font-size: 0.76rem;
    color: var(--text-2);
  }
  .swatches-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 14px;
    border-top: 1px solid var(--select-border);
  }
  .swatches-label {
    font-size: 0.8rem;
    color: var(--text-2);
    font-weight: 500;
  }
  .swatches-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .swatch-chip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 5px 12px;
    background: var(--select-bg);
    border: 1px solid var(--select-border);
    border-radius: 999px;
    cursor: pointer;
    font-size: 0.8rem;
    color: var(--text);
    transition:
      border-color 160ms var(--ease),
      background-color 160ms var(--ease),
      transform 120ms var(--ease);
  }
  .swatch-chip:hover {
    border-color: var(--accent-swatch);
    background: var(--select-surface-hover);
  }
  .swatch-chip.active {
    border-color: var(--accent-swatch);
    box-shadow: 0 0 10px rgba(var(--accent-color-rgb), 0.3);
    font-weight: 600;
  }
  .swatch-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent-swatch);
    flex-shrink: 0;
  }
  .swatch-chip:active {
    transform: scale(0.96);
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
  .card-head-avatar {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .avatar-state-badge {
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 0.76rem;
    font-weight: 600;
    background: rgba(184, 169, 255, 0.12);
    color: var(--text-2);
    border: 1px solid rgba(184, 169, 255, 0.2);
  }
  .avatar-state-badge.ready {
    background: rgba(16, 185, 129, 0.15);
    color: #34d399;
    border-color: rgba(16, 185, 129, 0.35);
  }
  .avatar-sim-controls {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid rgba(184, 169, 255, 0.1);
  }
  .sim-label {
    display: block;
    font-size: 0.78rem;
    color: var(--text-2);
    margin-bottom: 8px;
    font-weight: 500;
  }
  .sim-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .btn-diag {
    padding: 5px 10px;
    font-size: 0.78rem;
    font-weight: 500;
    border-radius: 6px;
    cursor: pointer;
    border: 1px solid rgba(184, 169, 255, 0.2);
    background: rgba(184, 169, 255, 0.08);
    color: var(--text);
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .btn-diag:hover {
    background: rgba(184, 169, 255, 0.18);
    border-color: var(--accent);
  }
  .btn-diag.primary {
    background: var(--accent);
    color: #ffffff;
    border-color: var(--accent);
  }
  .btn-diag.primary:hover {
    filter: brightness(1.1);
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
