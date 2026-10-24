<script lang="ts">
  import { Compass, Eye, Layers, Maximize2 } from "@lucide/svelte";
  import type { EvaluatedFrame, Pose, TrajectoryPreview } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";

  let {
    preview,
    currentFrame,
    liveCameraPose,
    selectedKeyframeId = null,
    compact = false,
    onSelectKeyframe,
    onUpdateKeyframePos,
  }: {
    preview: TrajectoryPreview | null;
    currentFrame: EvaluatedFrame | null;
    liveCameraPose: Pose | null;
    selectedKeyframeId?: string | null;
    compact?: boolean;
    onSelectKeyframe?: (id: string) => void;
    onUpdateKeyframePos?: (id: string, pos: [number, number, number]) => void;
  } = $props();

  type ViewMode = "orbit" | "top" | "side";
  let viewMode = $state<ViewMode>("orbit");
  let orbitYaw = $state(-32);
  let orbitPitch = $state(26);
  let zoomScale = $state(1);
  let panOffset = $state<[number, number]>([0, 0]);
  let dragging = $state(false);
  let panning = $state(false);
  let lastMouse = $state<[number, number]>([0, 0]);

  let draggingNodeId = $state<string | null>(null);
  let draggingNodePos = $state<[number, number, number] | null>(null);
  let draggingNodeLabel = $state<string>("");
  let dragOriginWorld = $state<[number, number, number] | null>(null);
  let dragOriginClient = $state<[number, number] | null>(null);
  let dragFrozenScale = $state<number>(1);
  let dragSvgScale = $state<[number, number]>([1, 1]);
  let svgEl = $state<SVGSVGElement | null>(null);

  const W = 560;
  const H = 310;

  // Calcula el centro y radio del bounding box espacial para auto-encuadrar la trayectoria
  const bounds = $derived.by(() => {
    const pts: [number, number, number][] = [];
    let hasTrajectoryReference = false;
    if (preview) {
      for (const n of preview.nodes) {
        pts.push(n.worldPos);
        hasTrajectoryReference = true;
      }
      for (const p of preview.finalPath) {
        pts.push(p);
        hasTrajectoryReference = true;
      }
      if (preview.anchorPos) {
        pts.push(preview.anchorPos);
        hasTrajectoryReference = true;
      }
      if (preview.targetPos) {
        pts.push(preview.targetPos);
        hasTrajectoryReference = true;
      }
    }
    // Si no hay nodos fijos en la trayectoria, establecer un marco de referencia estable
    // alrededor del ancla calibrada o del origen para que la cámara pueda desplazarse libremente
    // por la grilla sin auto-centrarse en su propia posición en cada frame
    if (!hasTrajectoryReference) {
      const anchorEye = preview?.anchorPos ?? [0, 1.6, 0];
      pts.push([anchorEye[0] - 1.5, anchorEye[1] - 0.6, anchorEye[2] - 1.5]);
      pts.push([anchorEye[0] + 1.5, anchorEye[1] + 0.9, anchorEye[2] + 1.5]);
    }
    if (liveCameraPose) pts.push(liveCameraPose.pos);
    if (pts.length === 0) {
      return { center: [0, 1.6, 0] as [number, number, number], span: 3.5 };
    }
    let minX = Infinity,
      maxX = -Infinity,
      minY = Infinity,
      maxY = -Infinity,
      minZ = Infinity,
      maxZ = -Infinity;
    for (const [x, y, z] of pts) {
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
      if (z < minZ) minZ = z;
      if (z > maxZ) maxZ = z;
    }
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    const cz = (minZ + maxZ) / 2;
    const span = Math.max(maxX - minX, maxY - minY, maxZ - minZ, 2.2) * 1.35;
    return { center: [cx, cy, cz] as [number, number, number], span };
  });

  function project(pt: [number, number, number]): { x: number; y: number; depth: number } {
    const [cx, cy, cz] = bounds.center;
    const scale = (Math.min(W, H) / bounds.span) * 0.78 * zoomScale;
    const dx = pt[0] - cx;
    const dy = pt[1] - cy;
    const dz = pt[2] - cz;

    if (viewMode === "top") {
      return { x: W / 2 + dx * scale + panOffset[0], y: H / 2 - dz * scale + panOffset[1], depth: -dy };
    }
    if (viewMode === "side") {
      return { x: W / 2 + dz * scale + panOffset[0], y: H / 2 - dy * scale + panOffset[1], depth: dx };
    }

    const yawRad = (orbitYaw * Math.PI) / 180;
    const pitchRad = (orbitPitch * Math.PI) / 180;
    const cosY = Math.cos(yawRad);
    const sinY = Math.sin(yawRad);
    const cosP = Math.cos(pitchRad);
    const sinP = Math.sin(pitchRad);

    const rx = dx * cosY - dz * sinY;
    const rz = dx * sinY + dz * cosY;
    const ry = dy * cosP - rz * sinP;
    const depth = dy * sinP + rz * cosP;

    return {
      x: W / 2 + rx * scale + panOffset[0],
      y: H / 2 - ry * scale + panOffset[1],
      depth,
    };
  }

  function polylineAttr(pts: [number, number, number][]): string {
    return pts
      .map((p) => {
        const pr = project(p);
        return `${pr.x.toFixed(1)},${pr.y.toFixed(1)}`;
      })
      .join(" ");
  }

  const gridLines = $derived.by(() => {
    const [cx, cy, cz] = bounds.center;
    const floorY = preview?.anchorPos ? preview.anchorPos[1] - 1.55 : cy - 0.8;
    const step = bounds.span > 6 ? 2 : 1;
    const half = Math.ceil(bounds.span / 2);
    const lines: { x1: number; y1: number; x2: number; y2: number }[] = [];
    for (let i = -half; i <= half; i += step) {
      const a1 = project([cx + i, floorY, cz - half]);
      const a2 = project([cx + i, floorY, cz + half]);
      lines.push({ x1: a1.x, y1: a1.y, x2: a2.x, y2: a2.y });

      const b1 = project([cx - half, floorY, cz + i]);
      const b2 = project([cx + half, floorY, cz + i]);
      lines.push({ x1: b1.x, y1: b1.y, x2: b2.x, y2: b2.y });
    }
    return lines;
  });

  function onNodePointerDown(nodeId: string, worldPos: [number, number, number], label: string, e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    onSelectKeyframe?.(nodeId);
    draggingNodeId = nodeId;
    draggingNodePos = [worldPos[0], worldPos[1], worldPos[2]];
    draggingNodeLabel = label;
    dragOriginWorld = [worldPos[0], worldPos[1], worldPos[2]];
    dragOriginClient = [e.clientX, e.clientY];

    // Congelar la escala espacial en el momento del click para evitar saltos o recientes dinámicos
    dragFrozenScale = (Math.min(W, H) / Math.max(1, bounds.span)) * 0.78 * zoomScale;

    // Medir la relación real entre píxeles de pantalla y unidades del SVG viewBox
    if (svgEl) {
      const rect = svgEl.getBoundingClientRect();
      const sx = rect.width > 0 ? rect.width / W : 1;
      const sy = rect.height > 0 ? rect.height / H : 1;
      dragSvgScale = [sx, sy];
    } else {
      dragSvgScale = [1, 1];
    }

    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerDown(e: PointerEvent) {
    if (draggingNodeId) return;
    // Paneo con botón central (1), botón derecho (2) o botón primario con Shift
    if (e.button === 1 || e.button === 2 || (e.button === 0 && e.shiftKey)) {
      e.preventDefault();
      panning = true;
      lastMouse = [e.clientX, e.clientY];
      if (svgEl) {
        const rect = svgEl.getBoundingClientRect();
        dragSvgScale = [rect.width > 0 ? rect.width / W : 1, rect.height > 0 ? rect.height / H : 1];
      }
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      return;
    }
    if (viewMode !== "orbit" || e.button !== 0) return;
    dragging = true;
    lastMouse = [e.clientX, e.clientY];
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (draggingNodeId && dragOriginWorld && dragOriginClient) {
      const totalClientDx = e.clientX - dragOriginClient[0];
      const totalClientDy = e.clientY - dragOriginClient[1];

      // Convertir desplazamiento de pantalla a unidades SVG del viewBox
      const svgDx = totalClientDx / dragSvgScale[0];
      const svgDy = totalClientDy / dragSvgScale[1];

      // Convertir unidades SVG a metros de mundo usando la escala fija del inicio del arrastre
      const scale = Math.max(0.001, dragFrozenScale);
      let newX = dragOriginWorld[0];
      let newY = dragOriginWorld[1];
      let newZ = dragOriginWorld[2];

      if (viewMode === "top") {
        // En top view: pantalla X -> mundo X, pantalla Y (abajo) -> mundo Z (atrás)
        newX = dragOriginWorld[0] + svgDx / scale;
        newZ = dragOriginWorld[2] - svgDy / scale;
      } else if (viewMode === "side") {
        // En side view: pantalla X -> mundo Z, pantalla Y (abajo) -> mundo Y (abajo)
        newZ = dragOriginWorld[2] + svgDx / scale;
        newY = dragOriginWorld[1] - svgDy / scale;
      } else {
        // En vista orbital 3D: orientar sobre el plano del ángulo de cámara
        const yawRad = (orbitYaw * Math.PI) / 180;
        const cosY = Math.cos(yawRad);
        const sinY = Math.sin(yawRad);
        newX = dragOriginWorld[0] + (svgDx * cosY) / scale;
        newZ = dragOriginWorld[2] - (svgDx * sinY) / scale;
        newY = dragOriginWorld[1] - svgDy / scale;
      }

      newX = Math.round(newX * 100) / 100;
      newY = Math.round(newY * 100) / 100;
      newZ = Math.round(newZ * 100) / 100;

      draggingNodePos = [newX, newY, newZ];
      onUpdateKeyframePos?.(draggingNodeId, [newX, newY, newZ]);
      return;
    }

    if (panning) {
      const dxClient = e.clientX - lastMouse[0];
      const dyClient = e.clientY - lastMouse[1];
      lastMouse = [e.clientX, e.clientY];
      const sx = dragSvgScale[0] > 0 ? dragSvgScale[0] : 1;
      const sy = dragSvgScale[1] > 0 ? dragSvgScale[1] : 1;
      panOffset = [panOffset[0] + dxClient / sx, panOffset[1] + dyClient / sy];
      return;
    }

    if (!dragging || viewMode !== "orbit") return;
    const dx = e.clientX - lastMouse[0];
    const dy = e.clientY - lastMouse[1];
    lastMouse = [e.clientX, e.clientY];
    orbitYaw = (orbitYaw + dx * 0.45) % 360;
    orbitPitch = Math.max(-75, Math.min(75, orbitPitch + dy * 0.35));
  }

  function onPointerUp() {
    dragging = false;
    panning = false;
    draggingNodeId = null;
    draggingNodePos = null;
    dragOriginWorld = null;
    dragOriginClient = null;
  }

  function resetView() {
    orbitYaw = -32;
    orbitPitch = 26;
    zoomScale = 1;
    panOffset = [0, 0];
  }
</script>

<div class="viewport-wrap" class:compact>
  <div class="vp-bar">
    <span class="vp-title"><Compass size={15} /> {t("studio.viewportTitle")}</span>
    <div class="vp-modes" role="tablist">
      <button
        type="button"
        class="vp-btn"
        class:active={viewMode === "orbit"}
        onclick={() => (viewMode = "orbit")}
      >
        <Layers size={13} /> {t("studio.viewOrbit")}
      </button>
      <button
        type="button"
        class="vp-btn"
        class:active={viewMode === "top"}
        onclick={() => (viewMode = "top")}
      >
        {t("studio.viewTop")}
      </button>
      <button
        type="button"
        class="vp-btn"
        class:active={viewMode === "side"}
        onclick={() => (viewMode = "side")}
      >
        {t("studio.viewSide")}
      </button>
      <button type="button" class="vp-btn" title="Restablecer vista" onclick={resetView}>
        <Maximize2 size={13} />
      </button>
    </div>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <svg
    bind:this={svgEl}
    viewBox="0 0 {W} {H}"
    class="vp-svg"
    class:panning
    role="img"
    aria-label={t("studio.viewportTitle")}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointerleave={onPointerUp}
    oncontextmenu={(e) => e.preventDefault()}
    onwheel={(e) => {
      e.preventDefault();
      const zoomFactor = e.deltaY < 0 ? 1.15 : 0.87;
      const prevScale = zoomScale;
      const newScale = Math.max(0.4, Math.min(4.5, zoomScale * zoomFactor));
      if (Math.abs(newScale - prevScale) < 1e-4) return;
      const kEff = newScale / prevScale;

      // Medir coordenadas SVG del cursor para centrar el zoom en el punto bajo el ratón
      let mx = W / 2;
      let my = H / 2;
      if (svgEl) {
        const rect = svgEl.getBoundingClientRect();
        if (rect.width > 0 && rect.height > 0) {
          mx = ((e.clientX - rect.left) / rect.width) * W;
          my = ((e.clientY - rect.top) / rect.height) * H;
        }
      }

      // Desplazar panOffset para mantener invariante la posición espacial bajo el cursor
      panOffset = [
        (mx - W / 2) * (1 - kEff) + panOffset[0] * kEff,
        (my - H / 2) * (1 - kEff) + panOffset[1] * kEff,
      ];
      zoomScale = newScale;
    }}
  >
    <defs>
      <radialGradient id="vpGlow" cx="50%" cy="50%" r="60%">
        <stop offset="0%" stop-color="#2a1446" stop-opacity="0.55" />
        <stop offset="100%" stop-color="#0e0a1a" stop-opacity="0.95" />
      </radialGradient>
    </defs>

    <rect width={W} height={H} fill="url(#vpGlow)" rx="12" />

    <!-- Grilla espacial de referencia -->
    <g class="grid-lines">
      {#each gridLines as ln, idx (idx)}
        <line x1={ln.x1} y1={ln.y1} x2={ln.x2} y2={ln.y2} stroke="rgba(184, 169, 255, 0.07)" stroke-width="1" />
      {/each}
    </g>

    <!-- Ancla calibrada (Origen relativo del jugador) -->
    {#if preview?.anchorPos}
      {@const aProj = project(preview.anchorPos)}
      {@const yawRad = ((preview.anchorYaw ?? 0) * Math.PI) / 180}
      {@const aFwd = project([
        preview.anchorPos[0] + Math.sin(yawRad) * 0.45,
        preview.anchorPos[1],
        preview.anchorPos[2] + Math.cos(yawRad) * 0.45,
      ])}
      <g class="anchor-marker">
        <line x1={aProj.x} y1={aProj.y} x2={aFwd.x} y2={aFwd.y} stroke="#b8a9ff" stroke-width="2" stroke-dasharray="3 2" />
        <polygon
          points="{aProj.x},{aProj.y - 7} {aProj.x + 7},{aProj.y} {aProj.x},{aProj.y + 7} {aProj.x - 7},{aProj.y}"
          fill="#2a1446"
          stroke="#b8a9ff"
          stroke-width="2"
        />
        <text x={aProj.x + 10} y={aProj.y + 4} fill="#b8a9ff" font-size="11" font-weight="600">{t("viewport.anchor")}</text>
      </g>
    {/if}

    <!-- Punto de interés (TargetProvider) -->
    {#if preview?.targetPos}
      {@const tProj = project(preview.targetPos)}
      <g class="target-marker">
        <circle cx={tProj.x} cy={tProj.y} r="8" fill="none" stroke="#ff708d" stroke-width="1.8" stroke-dasharray="2 2" />
        <circle cx={tProj.x} cy={tProj.y} r="3" fill="#ff708d" />
        <text x={tProj.x + 10} y={tProj.y + 4} fill="#ff708d" font-size="11">{t("viewport.target")}</text>
      </g>
    {/if}

    <!-- Trayectoria interpolada base (punteada) y trayectoria final con efectos (continua) -->
    {#if preview && preview.basePath.length > 1}
      <polyline
        fill="none"
        stroke="rgba(184, 169, 255, 0.35)"
        stroke-width="2"
        stroke-dasharray="4 4"
        points={polylineAttr(preview.basePath)}
      />
      <polyline
        fill="none"
        stroke="#3d5afe"
        stroke-width="3"
        stroke-linecap="round"
        stroke-linejoin="round"
        points={polylineAttr(preview.finalPath)}
      />
    {/if}

    <!-- Nodos de Keyframes (Puntos A, B, C...) con vector de dirección de cámara -->
    {#if preview}
      {#each preview.nodes as node, idx (node.id)}
        {@const p = project(node.worldPos)}
        {@const tip = project([
          node.worldPos[0] + node.forward[0] * 0.4,
          node.worldPos[1] + node.forward[1] * 0.4,
          node.worldPos[2] + node.forward[2] * 0.4,
        ])}
        {@const isSelected = selectedKeyframeId === node.id}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <g
          class="kf-node"
          class:is-dragging={draggingNodeId === node.id}
          onpointerdown={(e) => onNodePointerDown(node.id, node.worldPos, node.label, e)}
          onclick={(e) => {
            e.stopPropagation();
            onSelectKeyframe?.(node.id);
          }}
        >
          <!-- Cono/Vector de mirada -->
          <line x1={p.x} y1={p.y} x2={tip.x} y2={tip.y} stroke={isSelected ? "#ece8ff" : "#8c52ff"} stroke-width="2" />
          <circle cx={tip.x} cy={tip.y} r="2.5" fill={isSelected ? "#ece8ff" : "#8c52ff"} />

          <!-- Punto Keyframe -->
          <circle
            cx={p.x}
            cy={p.y}
            r={isSelected ? 9 : 7}
            fill={idx === 0 ? "#26c6da" : idx === preview.nodes.length - 1 ? "#ff708d" : "#3d5afe"}
            stroke={isSelected ? "#ffffff" : "#0e0a1a"}
            stroke-width={isSelected ? 2.5 : 1.5}
          />
          <text x={p.x} y={p.y - 12} text-anchor="middle" fill="#ece8ff" font-size="11" font-weight="600">
            {node.label} ({node.time.toFixed(1)}s)
          </text>
        </g>
      {/each}

      {#if draggingNodeId && draggingNodePos}
        {@const pDrag = project(draggingNodePos)}
        <g class="drag-coord-tooltip">
          <rect
            x={pDrag.x - 90}
            y={pDrag.y - 42}
            width="180"
            height="24"
            rx="6"
            fill="rgba(22, 16, 36, 0.95)"
            stroke="var(--accent, #3d5afe)"
            stroke-width="1.5"
          />
          <text
            x={pDrag.x}
            y={pDrag.y - 26}
            text-anchor="middle"
            fill="#ffffff"
            font-size="10.5"
            font-weight="600"
            font-family="monospace"
          >
            {draggingNodeLabel}: {draggingNodePos[0].toFixed(2)}, {draggingNodePos[1].toFixed(2)}, {draggingNodePos[2].toFixed(2)}m
          </text>
        </g>
      {/if}
    {/if}

    <!-- Pose Evaluada en el Cursor Actual (Playhead) -->
    {#if currentFrame}
      {@const cProj = project(currentFrame.finalPose.pos)}
      <g class="eval-cursor">
        <circle cx={cProj.x} cy={cProj.y} r="10" fill="rgba(255, 255, 255, 0.15)" />
        <circle cx={cProj.x} cy={cProj.y} r="5" fill="#ffffff" stroke="#3d5afe" stroke-width="2" />
      </g>
    {/if}

    <!-- Pose Real Recibida de VRChat (diferenciada explícitamente de la interpolada) -->
    {#if liveCameraPose}
      {@const lProj = project(liveCameraPose.pos)}
      <g class="live-vrc-cam">
        <circle cx={lProj.x} cy={lProj.y} r="8" fill="none" stroke="#4ade80" stroke-width="2" />
        <circle cx={lProj.x} cy={lProj.y} r="3" fill="#4ade80" />
        <text x={lProj.x + 11} y={lProj.y + 4} fill="#4ade80" font-size="10.5" font-weight="600">{t("viewport.liveVrc")}</text>
      </g>
    {/if}
  </svg>

  <div class="vp-footer">
    <div class="legend">
      <span><i class="dot start"></i> {t("viewport.legend.start")}</span>
      <span><i class="dot mid"></i> {t("viewport.legend.mid")}</span>
      <span><i class="dot end"></i> {t("viewport.legend.end")}</span>
      <span><i class="dot vrc"></i> {t("viewport.legend.vrc")}</span>
      <span><i class="dot cursor"></i> {t("viewport.legend.cursor")}</span>
    </div>
    {#if preview}
      <span class="dist-info">
        <Eye size={13} /> {t("viewport.pathInfo", { dist: preview.totalDistance.toFixed(2), nodes: preview.nodes.length })}
      </span>
    {/if}
  </div>
</div>

<style>
  .viewport-wrap {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--surface);
    border: 1px solid rgba(184, 169, 255, 0.12);
    border-radius: 14px;
    padding: 12px;
  }
  .vp-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .vp-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--lavender);
  }
  .vp-modes {
    display: flex;
    gap: 4px;
  }
  .vp-btn {
    padding: 4px 9px;
    font-size: 0.76rem;
    border-radius: 8px;
    background: rgba(14, 10, 26, 0.6);
    color: var(--text-2);
    border: 1px solid rgba(184, 169, 255, 0.12);
  }
  .vp-btn.active {
    background: var(--velvet);
    color: var(--text);
    border-color: var(--accent);
  }
  .vp-svg {
    width: 100%;
    height: auto;
    max-height: 300px;
    border-radius: 10px;
    cursor: grab;
    user-select: none;
    border: 1px solid rgba(184, 169, 255, 0.08);
  }
  .compact .vp-svg {
    max-height: 215px;
  }
  .vp-svg:active {
    cursor: grabbing;
  }
  .kf-node {
    cursor: grab;
    transition: filter 120ms ease;
  }
  .kf-node:hover {
    filter: drop-shadow(0 0 6px var(--accent, #3d5afe));
  }
  .kf-node:active,
  .kf-node.is-dragging {
    cursor: grabbing;
  }
  .drag-coord-tooltip {
    pointer-events: none;
    filter: drop-shadow(0 4px 12px rgba(0, 0, 0, 0.6));
  }
  .vp-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 0.76rem;
    color: var(--text-2);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    display: inline-block;
  }
  .dot.start {
    background: #26c6da;
  }
  .dot.mid {
    background: #3d5afe;
  }
  .dot.end {
    background: #ff708d;
  }
  .dot.vrc {
    background: #4ade80;
  }
  .dot.cursor {
    background: #ffffff;
    border: 1.5px solid #3d5afe;
  }
  .dist-info {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
</style>
