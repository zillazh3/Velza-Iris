# Hallazgos OSC (M0)

Estado: **pendiente de correr contra VRChat**. La sonda (`tools/probe`) ya pasó loopback (listen+send); falta validar con el cliente real.
Si algo contradice el prompt maestro (`CLAUDE.md`), gana lo medido aquí.

## Preparación
- VRChat con OSC habilitado (Action Menu → Options → OSC → Enabled), cámara abierta.
- `cd tools/probe && cargo build --release`; binario en `target/release/probe.exe`.
- Cierra otras apps OSC (el puerto UDP 9001 debe estar libre para `listen`/`rate`).

## Comandos
| Comando | Qué hace |
|---|---|
| `probe tree [host:port]` | OSCQuery por mDNS → `docs/oscquery-usercamera.json` + resumen tipo/rango/acceso |
| `probe listen [port]` | Log con timestamp de todo `/usercamera/*` en `tools/probe/logs/` |
| `probe send <addr> [args…] [--to host:port]` | `true/false`→bool, `i:N`→int, número→float |
| `probe rate <hz> <secs> [radius]` | Orbita la última Pose a `hz`; reporta retraso de envío y ecos |

## Experimentos (marca y anota resultado)

### 1. Árbol OSCQuery
- [x] `probe tree` (VRChat-Client-EC5D92, OSC 127.0.0.1:9000 UDP; árbol en `oscquery-usercamera.json`).
- **Todos los `RANGE` vienen `null`**: los clamps salen solo de la tabla oficial del prompt.
- `Pose` figura con ACCESS=1 (solo lectura) en OSCQuery, pero VRChat sí la acepta por OSC (ver sección 6).
- `Zoom` reportó `NaN` con la cámara en reposo; no usar el valor del árbol como estado inicial.
- Endpoints extra no documentados en el prompt: `AudioFromCamera`, `FlingToClose` (no se usan).
- `Capture`, `CaptureDelayed`, `Close`: ACCESS=2 (solo escritura).

### 2. Formato de `/usercamera/Pose`
- [x] `listen` con 4072 muestras (60 s moviendo la cámara a mano):
  - 6 floats: `x y z` en metros, `rx ry rz` en grados.
  - `y` es arriba (1.58–1.87 con altura de ojos ≈ 1.62–1.67). `x`/`z` horizontales.
  - `rx` = pitch (≈ 0–11° mirando al frente; envuelve a 0–360, valores >180 = mirar abajo/arriba según signo, por confirmar), `ry` = yaw 0–360, `rz` = roll (siempre 0 con AutoLevelRoll off y sin tocarlo).
  - Posiciones en el log son coordenadas de mundo (z 0.2–5.3, x −2.9–2.3). Pendiente confirmar espacio con anchor (sección 3).
- [x] Escritura: `send`/`rate` mueve la cámara y VRChat responde con eco de Pose → **Pose es escribible** pese al ACCESS=1 del árbol.
- [x] Eco valor a valor (7 poses de prueba con `LookAtMe` off):
  - Posición: eco idéntico, sin clamp (aceptó `100 50 -30`).
  - Rotación Euler en grados, **normalizada a [0, 360)**: `-10→349.99997`, `400→40`, `-90→270`.
  - Roll se acepta (`rz=15` → `15.000001`).
  - **Pitch > 90° se reinterpreta** como equivalente de Euler: `95 90 0` → `85.00009 270 180`. La app debe limitar el pitch a ±90° (rx en [0,90] ∪ [270,360)) y no enviar más.
  - Error de float ~1e-5; tolerancia de comparación ≥1e-3.
  - Latencia del eco ≈ 90 ms tras el envío (en pruebas separadas 2 s).
- El sentido físico de `rx` (+ = mirar abajo, convención Unity) sigue sin confirmar visualmente.

### 3. Pose relativa al jugador (Anchor)
- [x] Intento 1 (60 s, cámara quieta): 0 mensajes Pose; solo `LookAtMe false` (el toggle sí hace eco). El usuario no había activado el anchor.
- [x] Intento 2 (60 s, 2902 Pose; el usuario dice que activó el anchor y caminó en línea recta/giró): la Pose reportada **sí cambia** al moverse el jugador.
  - `x` recorre −6.7…+2.6 m yendo y viniendo; `z`≈0.4–1.3 casi constante; `y` ≈1.62–1.66.
  - `ry` alterna ≈270° y ≈89° en cada vuelta de 180° (la cámara gira con el avatar).
  - Es decir: la cámara sigue al jugador y su Pose sale **en espacio de mundo**, no relativa al jugador.
- Conclusión provisional: **Modo A no viable** (la Pose no es relativa aunque el anchor siga al jugador) → **Modo B** (Calibrar ancla). Confirmado por el usuario: la cámara sí se movió visualmente con el avatar (anchor Local/Attached).
- [x] Intento 3, anchor **World**, cámara quieta, avatar caminando/girando (60 s): **0 mensajes**. Con World la cámara se queda fija en el mundo; la diferencia viene del anchor.
- Idea para Modo B: con anchor attached la cámara es rígida respecto al jugador; la Pose de la cámara en el momento de calibrar da el origen (ojos + yaw), y los presets se resuelven en mundo con esa referencia. Re-calibrar tras teletransportes.

### 4. LookAtMe X/Y
- [x] Enviado `LookAtMe true` y offsets por pasos de 8 s; el eco de Pose reveló el efecto (posición fija 2.49, 1.55, 1.37):
  - Base (X=0,Y=0): `rx=358.43` (−1.57°), `ry=272.15`.
  - X=+25 → `ry=297.15` (**+25°**); X=−25 → `ry=247.15` (**−25°**). `rx` no cambia.
  - Y=+25 → `rx=23.43` (**+25°**); Y=−25 → `rx=333.43` (**−25°**). `ry` no cambia.
- **Unidades: grados**, offset sumado a la rotación que apunta al jugador. **X = yaw, Y = pitch**; no están cruzados en esta build (con respecto a Pose).
- `rx` positivo = mirar hacia abajo (convención Unity) por confirmar con lo que vio el usuario en pantalla.
- Cada comando produce eco doble del propio slider (2 mensajes en ~5 ms) y un eco de Pose.
- Pendiente: confirmar con lo visto en pantalla el sentido (¿X+ mueve el sujeto a izquierda o derecha en el encuadre?).

### 5. Zoom
- [x] `Zoom 30 / 45 / 90` con la cámara quieta y `Capture` (fotos 3840×2160): el ancho de un mismo objeto (la cama circular) mide ≈270 / 410 / 825 px (a 1280 px de ancho).
  - Cocientes: 45/30 = ×1.52 y 90/45 = ×2.01, iguales a los cocientes de las focales (×1.5 y ×2.0).
- **Conclusión: el `Zoom` de VRChat se comporta como una focal (mm), no como un ángulo.** A mayor valor, más cerca (teleobjetivo); el tamaño en cuadro es proporcional al Zoom. Explica el rango 20–300 y el default 45. La doc oficial lo llama "FOV" y está equivocada en esta build.
- Convención usada por la app: `mm = 12 / tan(FOV/2)` (la tabla del prompt, sensor de 24 mm de alto) para traducir encuadres pensados en FOV. Si el sensor real fuera otro, la focal sigue siendo el valor de Zoom; solo cambiaría el ángulo equivalente.
- Vertical u horizontal ya no importa para el dolly zoom: `zoom = k·distancia` mantiene el tamaño del sujeto con cualquier sensor.
- Nota: las 3 fotos de la prueba quedaron en `OneDrive/Imágenes/VRChat/2026-10/` (2026-10-22 12-04-38/43/49).

### 6. Frecuencia de Pose
Con la cámara abierta: `probe rate 30 10`, `rate 60 10`, `rate 90 10`; repetir con `SmoothMovement` on y off.

| Hz | Smooth | Retraso medio (ms) | Máx (ms) | Ecos | Movimiento visual |
|---|---|---|---|---|---|
| 30 | off | 0.00 | 0.02 | 241/240 | (no evaluado visualmente) |
| 30 | on | 0.00 | 0.08 | 559/240 | |
| 60 | off | — | — | — | falló: sin Pose base (ver nota) |
| 60 | on | 0.00 | 0.08 | 515/480 | |
| 90 | off | 0.00 | 1.34 | 518/720 | |
| 90 | on | 0.00 | 1.24 | 554/720 | |

Ecos = mensajes Pose recibidos durante envío + 0.5 s, 8 s por prueba, órbita r=0.1 m.
- El eco tope ronda **~61–65 Hz** (≈ framerate de VRChat); a 90 Hz VRChat coalesce (518–554 de 720).
- Con `SmoothMovement` on el eco sale a ~65 Hz aunque se envíe a 30 Hz (VRChat interpola y reporta cada frame).
- Sin smoothing y a 30 Hz hay 1 eco por mensaje.
- Nuestro lado envía con retraso ≤1.3 ms (spin fino); no es el cuello de botella.
- Nota técnica: `rate` toma como base la última Pose **recibida**; VRChat solo emite Pose cuando cambia, así que con la cámara quieta no hay base (falló 60 Hz off). Mejorar `rate` o mover la cámara antes.
- El mismo comportamiento implica que la cámara puede haber quedado ligeramente desplazada entre corridas.

Hz seguro elegido (provisional): **60 Hz** (≈ el tope de eco); falta juzgar suavidad visual a 30 vs 60.

### 7. Eco de sliders
Mover cada slider desde el menú de la cámara y marcar si llega mensaje en `listen`.
Ventana de 90 s moviendo sliders desde Quick Menu → Camera (2609 mensajes). **Sí hacen eco** (mensajes por endpoint, rango observado):
| Endpoint | Msgs | Mín | Máx | Nota |
|---|---|---|---|---|
| Zoom | 490 | 20 | **300** | contradice el prompt (150); gana la sonda |
| Exposure | 298 | −10 | 4 | coincide |
| Aperture | 425 | 1.4 | 32 | coincide |
| FocalDistance | 290 | 0 | 10 | coincide |
| FlySpeed | 612 | 0.1 | 15 | coincide |
| TurnSpeed | 476 | 0.1 | 5 | coincide |
Toggles con eco: LookAtMe, SmoothMovement, AutoLevelPitch. Los sliders emiten ~60 Hz mientras se arrastran y al soltar vuelven al valor que fije el usuario.
Segunda ventana (120 s, 975 msgs), también con eco y rango igual a la tabla:
| Endpoint | Msgs | Mín | Máx |
|---|---|---|---|
| LookAtMeXOffset | 104 | −25 | 25 |
| LookAtMeYOffset | 42 | −25 | 25 |
| SmoothingStrength | 420 | 0.1 | 10 |
Los X/Y solo mostrados como eco: si están cruzados respecto al menú no se puede ver aquí; hay que enviarlos y mirar (sección 4).
Sin probar aún: PhotoRate, Duration, Hue/Saturation/Lightness.
- [x] Zoom  [x] Exposure  [ ] FocalDistance  [ ] Aperture  [ ] LookAtMeXOffset  [ ] LookAtMeYOffset
- [ ] FlySpeed  [ ] TurnSpeed  [ ] SmoothingStrength  [ ] PhotoRate  [ ] Duration  [ ] Hue/Saturation/Lightness

### 8. Cámara cerrada / abierta
- [x] `Close true` → la cámara se cierra (confirmado por el usuario) y llega eco `Mode [i:0]`.
- [x] Con la cámara cerrada, `Mode i:1` y luego `Mode i:2`: llegan ecos `Mode [i:1]` / `[i:2]` pero **la cámara no se abre** (confirmado por el usuario).
- Conclusión: **`Mode` no abre la cámara** (el prompt acertó), y su eco solo refleja el valor escrito, así que **no sirve para detectar si está abierta**. Solo `Mode=0` tras un `Close` es fiable.
- [x] Ventana de 75 s abriendo/cerrando a mano desde el menú (38 msgs: 34 `Mode`, 4 `Pose`):
  - **Cerrar desde el menú → `Mode [i:0]`; abrir → `Mode [i:1]` (Photo) o `[i:2]` (Stream)** según el modo elegido. Cada apertura/cierre produce un eco, sin latencia apreciable.
  - A veces llega un `Pose` ~20 ms después de abrir (4 de ~17 aperturas); no es fiable como detector.
  - Cambiar de modo (1↔2) con la cámara abierta pasa por `Mode 0` (cierra y reabre).
- **Detector de apertura/cierre: el eco de `Mode`** (0 = cerrada, ≠0 = abierta) cuando lo origina el usuario. Las escrituras propias de la app mientras está cerrada también devuelven eco sin abrirla, así que la app debe **ignorar el eco de sus propios `Mode`** (p. ej. no escribir `Mode` salvo con cámara abierta) y reaplicar el preset al ver `0→N` iniciado por el usuario.

### 9. Modos de enfoque
- [ ] ¿Aperture solo en Semi Auto/Manual? ¿FocalDistance solo en Manual? ¿Hay endpoint para el modo de enfoque?
- Resultado:

## Ajustes al plan
- Modo A de ancla descartado → Modo B (calibrar). Ver sección 3.
- Frecuencia de envío de Pose: 60 Hz (eco tope ≈ 61–65 Hz).
- Zoom: rango real 20–300 y es una **focal en mm** (no FOV). Semillas y movimientos convertidos a mm (Ambiente amplio 70° → 17 mm, recortado al mínimo 20).
- Rangos que no vienen en OSCQuery (todos `null`) → hardcodeados desde la tabla.

## Prueba en vivo de M1 (app Tauri contra VRChat)
- Conexión OSCQuery, detección de cámara abierta, Calibrar y aplicar presets funcionan.
- Ancla de la prueba: ojos `(-0.87, 1.65, 1.93)`, yaw 117.6°. Retrato ¾ salió a 1.30 m de ese punto, mirando de vuelta (yaw 332.6° = 152.6° + 180°); el usuario confirma que "se ve bien".
- Bug propio encontrado y corregido: un log de depuración tomaba el mismo `Mutex` dos veces en una expresión y colgaba la ventana al pulsar Aplicar (std `Mutex` no es reentrante). No llegó a ningún commit.
- Confirmado visualmente por el usuario: yaw positivo = **derecha del jugador** (Retrato ¾ con yaw 35° queda a la derecha del avatar, casi de frente); el roll del Dutch angle (12°) **sí se aplica con `LookAtMe` activo**.
- Sin verificar aún: reaplicado de preset al reabrir la cámara.
