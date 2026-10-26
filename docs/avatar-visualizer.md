# Visualizador de avatar (gizmo de cámara + path de 5 huesos)

Rig de Unity que previsualiza el recorrido dentro de VRChat. La app lo controla por OSC (`/avatar/parameters/iris_*`, UDP 9000).
El asset es el prefab `Assets/Velza Iris/Velza Iris.prefab`, que se integra con **VRCFury** (no destructivo).

## Hallazgos de la auditoría

### Zoom que sigue después del recorrido
- El hilo de playback (`studio/player.rs`, 60 Hz) mandaba `/usercamera/Zoom` en **cada tick** con Δ > 1e-3 (`commands.rs`, `camera/mod.rs`).
- VRChat procesa `/usercamera/Zoom` a **1 mensaje por frame** (a diferencia de `Pose`, que toma el último). A 60 Hz contra ~45 fps sobran ~15 msg/s
  y el zoom sigue moviéndose cuando la app ya terminó. No había loop de feedback ni reloj aparte: Pose y Zoom usan el mismo `t`.
- Fix: `Camera::set_paced` / `State::should_send_paced` limitan Zoom, Exposure, FocalDistance y Aperture a ≥ 31 ms entre mensajes (~30 Hz)
  y el último frame (`flush`) envía el valor destino exacto, saltándose el dedupe de 1e-3. `PACED_MIN_GAP` es la perilla.
  Si VRChat corre por debajo de 30 fps todavía puede quedar algo de backlog; la mejora sería esperar el eco del valor anterior.

### Mesh de cámara desacoplado de la cámara real
1. Nada reenviaba el eco de `/usercamera/Pose` (9001) a `iris_cam_*`: solo se sincronizaba con la pose calculada por la app.
   Ahora el handler de Pose llama a `forward_pose_to_visualizer` (salvo durante un recorrido, que ya sincroniza el hilo de 60 Hz).
2. Los clips generados usaban ±65 m y la app normaliza con 50 m (offsets al 77 %). Ahora ambos usan 50 m
   (`MaxRangeMeters` en `VelzaIrisBuilder.cs` ↔ `MAX_RANGE_METERS` en `avatar/protocol.rs`).
3. Origen y yaw: el rig quedaba congelado en `Head + 0.30 m` con el yaw del avatar, pero la app asumía ejes de mundo.
   Ahora el ancla es la pose de la cámara de VRChat **al confirmar** (posición + yaw) y la app expresa posición y yaw en los ejes del ancla
   (`Anchor::world_to_local`). El `VRCRotationConstraint` de `Iris_World_Root` sigue el yaw del avatar (ya no queda en 0,0,0 global).
4. No hay error de convención Euler: `Cam_RotY → Cam_RotX → Cam_RotZ` anidados equivalen al orden ZXY de Unity/VRChat.

### Bug extra en el instalador viejo
Los clips `WP_N_MoveBN` dejaban `FreezeToWorld = 1` también en el hueso seleccionado, y un constraint congelado ignora sus sources:
ningún waypoint podía moverse. Ahora el hueso activo va con `FreezeToWorld = 0` y peso 1.

## Anclaje al origen del mundo (sin calibrar)
`Iris_World_Root` tiene un `VRCParentConstraint` cuya única fuente es `Assets/Velza Iris/Iris_World_Origin.prefab` (un objeto vacío
en 0,0,0, **fuera de la escena**: técnica "World Constraint"). Así el rig vive siempre en el origen del mundo y la pose de mundo de
`/usercamera/Pose` es directamente la del gizmo; todos los clientes comparten la misma referencia.

Antes el rig se congelaba (`FreezeToWorld`) en cabeza + 0.30 m cuando llegaba `iris_enable`. Cada cliente lo congelaba donde veía la
cabeza en ese instante, y un remoto que cargaba el avatar con `iris_enable` ya en `true` lo congelaba antes de resolver la fuente
(captura de un 2º cliente: gizmo ~2.3 m más abajo que para el dueño). Un constraint sin fuente resuelta + FreezeToWorld toma la
posición inicial del avatar, distinta por usuario (reporte conocido en feedback.vrchat.com). Sumar un offset fijo no lo arregla.

Flujo: **Mostrar gizmo** (`visualizer_activate`) → `iris_enable = true`, región en (0,0,0) y manda la pose actual de la cámara.
Mover la cámara a mano o reproducir un recorrido mueve el gizmo en vivo. **Apagar gizmo** (`visualizer_reset_anchor`) lo oculta.
El ajuste fino de la tarjeta corrige un desfase residual del pivote del FBX (`CameraMeshPositionOffset` / `CameraMeshEulerOffset`
en `VelzaIrisBuilder.cs` si el desfase es fijo).

## Protocolo v2: dueño exacto, remotos por bus (82 → 42 bits sincronizados)
Un float sincronizado llega a los demás en 8 bits (múltiplos de 1/127 sobre [−1, 1]) y a ~5-10 Hz: ±50 m daban 0.39 m de paso (0.2 m de
error) y 1.41° en rotación. Se separan los canales con `IsLocal`:

**Dueño** (parámetros *no* sincronizados, float32 exacto, 0 bits de red; los escribe la app por OSC): `iris_cam_x/y/z/rx/ry/rz`
(pose en vivo, 1.0 = 50 m / 180°), `iris_reg_x/y/z` (centro de la región, 1.0 = 1000 m) y los 8 puntos de control `iris_pc_i_x/y/z`
(posición, 1.0 = 50 m relativa al centro) e `iris_pr_i_x/y/z` (rotación desenvuelta, 1.0 = 360°). El path aparece al instante.

**Remotos** (sincronizados, 42 bits: `iris_enable`, `iris_hide_cam`, `iris_net_slot` Int, `iris_net_x/y/z`, `iris_path_t`):
- *Bus time-sliced*: por slot la app escribe `iris_net_x/y/z`, espera 250 ms, escribe `iris_net_slot = k` y espera 250 ms. En el FX
  (`Iris_Net_Latch`) cada slot es un estado con un `VRCAvatarParameterDriver` que copia el bus al parámetro local del punto
  (Sample & Hold) y que corre también en remotos; el dueño no lo ejecuta (`IsLocal = false` en la condición).
- *Slots* (30): región, posición coarse ×8, rotación ×8, posición fine ×8, pose retenida (coarse/fine/rotación) y modo path/mano.
  Orden de envío: región, coarse, rotaciones, fine (el path aparece rápido y se afina después). ~12 s el ciclo completo.
- *Coarse + fine*: `c = q8(v)`, `f = q8((v − c)·254)`, `v ≈ c + f/254` ⇒ ~1.6 mm con ±50 m. La app precuantiza como la red y escribe el
  centro del bin (k ± 0.02) para que redondeo o truncado den el mismo k. Rotaciones: solo coarse sobre ±360° (error máx. 1.4°).
- *`iris_path_t`* (≤ 10 Hz, ya adelantado `0.12 s·dT/dt`): progreso sobre los 8 puntos. El gizmo remoto es
  `Σ_i (Pc_i + Pf_i)·R·w_i(T_s)`, con `T_s` = `T` suavizado exponencialmente (`T_s' = k·T + (1−k)·T_s`, k = 0.15) y `w_i` la "tienda"
  lineal de cada nudo (árboles 1D anidados; los pesos de un Direct Tree no pueden ser negativos, por eso el valor con signo va en un
  árbol 1D interior). La rotación sale por AAP (`iris_g_r*`) hacia los `RotTree` de cuaterniones cada 45° entre −360° y +360°.
- *Pose retenida*: con la cámara a mano quieta 300 ms (o tras un scrub) la app manda su pose por slots y el gizmo remoto se coloca ahí.
- *Región móvil*: el centro se elige alrededor del path (rejilla de 7.9 m) o, si la cámara sale de ±40 m, alrededor de ella
  (con enfriamiento de 5 s). Ya no hay límite de ±50 m del origen del mundo.
- Sin canal de retorno no hay ack: el path se repite (la app lo reenvía al cambiar y cada 30 s). Un trabajo de pose o modo se cuela
  entre slots de un ciclo largo, y un path nuevo aborta el viejo.
- Los huesos del path ya no usan FreezeToWorld ni constraint de posición: su posición local es función pura de los parámetros (5 de los
  8 puntos: índices 0, 2, 4, 5, 7), igual en todos los clientes y recuperable en cualquier momento.

`MuxTiming::REMOTE` (250/250 ms) son valores estimados: calibrar con un segundo cliente (`tools/probe`, `tools/test_osc_sync.py`).

## Look holográfico
`Shaders/VelzaIris_HideFromCamera.shader` (mismo nombre, así los materiales del builder lo siguen usando) dibuja el gizmo y el path como
holograma: pasada de profundidad + pasada aditiva con borde fresnel, líneas de escaneo que suben en espacio de mundo, parpadeo y glitch
horizontal ocasional. Sin GrabPass (apto para PC). El color del material se mezcla con `_HoloTint` (cian) para no perder la flecha.
Perillas por material: `_HoloTint`, `_TintMix`, `_Intensity`, `_Alpha` (brillo del cuerpo), `_RimPower`/`_RimStrength`, `_ScanDensity`/
`_ScanSpeed`/`_ScanStrength`, `_Flicker`, `_Glitch`. Vista previa: `VelzaIrisBatch.Preview` renderiza PNG (corre sin `-nographics`).

## Ocultar de las cámaras de VRChat
`iris_hide_cam` cambia los materiales del gizmo y del path entre la variante *show* y *hide*. La variante *hide* usa
`Shaders/VelzaIris_HideFromCamera.shader`, que descarta el fragmento si `_VRChatCameraMode > 0` (cámara de mano, foto, stream, dron).
Al ser un parámetro synced oculta el rig tanto en tu cámara como en las de los demás. No funciona en Quest/Android (shaders custom).

## VRCFury
`Velza Iris.prefab` lleva un **Full Controller** (FX + menú + parámetros) con todos los parámetros `iris_*` (OSC e internos) en
*Global Parameters* (sin eso VRCFury les antepone `VF##_` y OSC, los drivers y los clips AAP dejarían de encontrarlos).
Ya no usa Armature Link a Head. `Validate` comprueba coste (42 bits propios), bindings AAP y latch, y simula el FX completo
(dueño, remoto con cuantización de 8 bits, pose retenida y avatar recargado); los constraints solo se evalúan en VRChat.
Los clips usan rutas relativas a la raíz del prefab; VRCFury las reescribe donde se coloque.

Menú de Unity:
- `Tools/Velza Iris/Build Prefab`: regenera el prefab y `Generated/`.
- `Tools/Velza Iris/Uninstall Legacy From Selected Avatar`: quita lo que inyectaba el instalador viejo (con backup en `Assets/VelzaIris_Backup`).
- `Tools/Velza Iris/Add Prefab To Selected Avatar`.

Batchmode (Unity cerrado): `Unity.exe -batchmode -nographics -projectPath <proyecto> -executeMethod VelzaIris.VelzaIrisBatch.<Build|MigrateZilla|Validate> -quit -logFile out.log`.

## Probar en VRChat
1. Recorrido con Zoom de ~10 s: al llegar al final el zoom debe terminar junto con la pose (antes seguía varios segundos).
   Medición opcional: `cd tools/probe && cargo run -- lag Zoom,Pose 60 10`.
2. Mostrar gizmo (sin calibrar). Mover la cámara a mano: el gizmo la sigue, también tras caminar lejos.
3. Reproducir el recorrido: el gizmo recorre el path (8 puntos de control); el mesh de 5 huesos pasa por 5 de ellos.
4. "Ocultar de cámaras": el gizmo desaparece en foto/stream/dron pero se ve en tu vista normal.
5. Con un segundo cliente: ve el gizmo en el mismo sitio y a la misma altura que tú (antes lo veía hundido ~2.3 m) y el path
   (el path remoto tarda ~12 s en llegar la primera vez; quien entra tarde lo recibe en el siguiente reenvío, ≤ ~42 s; el gizmo se ve suavizado).
