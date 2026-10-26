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

Flujo: **Mostrar gizmo** (`visualizer_activate`) → `iris_enable = true`, ancla = (0,0,0) yaw 0 y manda la pose actual de la cámara.
Mover la cámara a mano o reproducir un recorrido mueve el gizmo en vivo. **Apagar gizmo** (`visualizer_reset_anchor`) lo oculta.
Límite: ±50 m del origen del mundo (la app avisa); el ajuste fino de la tarjeta corrige un desfase residual del pivote del FBX
(`CameraMeshPositionOffset` / `CameraMeshEulerOffset` en `VelzaIrisBuilder.cs` si el desfase es fijo).
El path se reenvía solo: al cambiar puntos (debounce 0.8 s) y cada 15 s, para que quien entre tarde lo reciba.

## Parámetros OSC (todos *synced*, 82 bits)
`iris_enable` (bool), `iris_hide_cam` (bool), `iris_wp_index` (int 0..5), `iris_cam_x/y/z/rx/ry/rz` y `iris_wp_x/y/z` (float −1..1).
Posición: 1.0 = 50 m. Rotación: 1.0 = 180°. Con 8 bits de sync la posición que ven *otros* jugadores avanza en pasos de ~0.4 m;
el propietario del avatar ve el valor sin cuantizar.

Como los parámetros son synced, el multiplexor de los 5 huesos usa `MuxTiming::REMOTE` (250/250/150 ms, ~3.3 s por sync) en vez de
30/30/20 ms. Son valores estimados: calibrar con un segundo cliente.

## Ocultar de las cámaras de VRChat
`iris_hide_cam` cambia los materiales del gizmo y del path entre la variante *show* y *hide*. La variante *hide* usa
`Shaders/VelzaIris_HideFromCamera.shader`, que descarta el fragmento si `_VRChatCameraMode > 0` (cámara de mano, foto, stream, dron).
Al ser un parámetro synced oculta el rig tanto en tu cámara como en las de los demás. No funciona en Quest/Android (shaders custom).

## VRCFury
`Velza Iris.prefab` lleva un **Full Controller** (FX + menú + parámetros) con los 12 parámetros OSC en *Global Parameters*
(sin eso VRCFury les antepone `VF##_` y OSC dejaría de llegar). Ya no usa Armature Link a Head.
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
3. Reproducir el recorrido: el gizmo recorre el path y el path de 5 huesos coincide con la trayectoria.
4. "Ocultar de cámaras": el gizmo desaparece en foto/stream/dron pero se ve en tu vista normal.
5. Con un segundo cliente: ve el gizmo en el mismo sitio y a la misma altura que tú (antes lo veía hundido ~2.3 m) y el path
   (los waypoints remotos tardan ~3 s en actualizarse; quien entra tarde lo recibe en ≤ ~18 s por el reenvío periódico).
