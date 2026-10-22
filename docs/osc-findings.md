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
- [ ] Falta: confirmar que el eco coincide valor a valor con lo enviado y el sentido del signo de `rx`.

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
- [ ] `send /usercamera/LookAtMe true`, luego `LookAtMeXOffset 25` y `LookAtMeYOffset 25`, captura. ¿Unidades (% frame / grados)? ¿X/Y cruzados?
- Resultado:

### 5. Zoom
- [ ] `send /usercamera/Zoom 30`, objeto de referencia a distancia conocida. ¿FOV vertical u horizontal? ¿Tabla ≈mm válida?
- Resultado:

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
No probados en esta ventana (no se movieron): LookAtMeX/YOffset, SmoothingStrength, PhotoRate, Duration, Hue/Saturation/Lightness.
- [x] Zoom  [x] Exposure  [ ] FocalDistance  [ ] Aperture  [ ] LookAtMeXOffset  [ ] LookAtMeYOffset
- [ ] FlySpeed  [ ] TurnSpeed  [ ] SmoothingStrength  [ ] PhotoRate  [ ] Duration  [ ] Hue/Saturation/Lightness

### 8. Cámara cerrada / abierta
- [ ] Con la cámara cerrada: `send /usercamera/Mode i:1` → ¿se abre?
- [ ] ¿Qué mensajes llegan al abrir y cerrar (para detectar estado)?
- Resultado:

### 9. Modos de enfoque
- [ ] ¿Aperture solo en Semi Auto/Manual? ¿FocalDistance solo en Manual? ¿Hay endpoint para el modo de enfoque?
- Resultado:

## Ajustes al plan
- Modo A de ancla descartado → Modo B (calibrar). Ver sección 3.
- Frecuencia de envío de Pose: 60 Hz (eco tope ≈ 61–65 Hz).
- Zoom: rango real 20–300 (no 150). Actualizar clamp y tabla de la sección 1 del prompt al cerrar M0.
- Rangos que no vienen en OSCQuery (todos `null`) → hardcodeados desde la tabla.
