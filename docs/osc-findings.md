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
- [ ] `probe tree` → ¿qué endpoints/rangos/acceso reporta? ¿Coincide con la tabla de la sección 1?
- Resultado:

### 2. Formato de `/usercamera/Pose`
- [ ] `listen` + mover la cámara a mano. ¿Cuántos floats? ¿Orden (x y z rx ry rz)? ¿Grados?
- [ ] `send /usercamera/Pose x y z rx ry rz` → ¿la cámara va a donde se espera? ¿Qué eje es "arriba"?
- Resultado:

### 3. Pose relativa al jugador (Anchor)
- [ ] Anchor World: caminar/girar el avatar con la cámara quieta → ¿cambia la Pose reportada?
- [ ] Anchor Local/Attached: lo mismo. ¿La Pose es relativa al jugador?
- Conclusión (Modo A viable / usar Modo B): 

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
| 30 | off | | | | |
| 30 | on | | | | |
| 60 | off | | | | |
| 60 | on | | | | |
| 90 | off | | | | |
| 90 | on | | | | |

Hz seguro elegido:

### 7. Eco de sliders
Mover cada slider desde el menú de la cámara y marcar si llega mensaje en `listen`.
- [ ] Zoom  [ ] Exposure  [ ] FocalDistance  [ ] Aperture  [ ] LookAtMeXOffset  [ ] LookAtMeYOffset
- [ ] FlySpeed  [ ] TurnSpeed  [ ] SmoothingStrength  [ ] PhotoRate  [ ] Duration  [ ] Hue/Saturation/Lightness

### 8. Cámara cerrada / abierta
- [ ] Con la cámara cerrada: `send /usercamera/Mode i:1` → ¿se abre?
- [ ] ¿Qué mensajes llegan al abrir y cerrar (para detectar estado)?
- Resultado:

### 9. Modos de enfoque
- [ ] ¿Aperture solo en Semi Auto/Manual? ¿FocalDistance solo en Manual? ¿Hay endpoint para el modo de enfoque?
- Resultado:

## Ajustes al plan
(Se llena tras los experimentos.)
