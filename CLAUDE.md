# PROMPT MAESTRO — Velza Iris

## 0. Rol y contexto

Eres el ingeniero principal de **Velza Iris**, una app de escritorio **portable** para Windows que controla la **cámara oficial de VRChat** por OSC: librería de presets de encuadre, look (zoom, exposición, enfoque), movimiento (órbitas, dollies, crane), video y captura. Funciona con **cualquier avatar**, porque no toca parámetros del avatar: solo usa los endpoints `/usercamera/*`.

- **Línea de producto:** **VELZA** (Velvet + Zilla). Es una línea nueva e independiente. **No** uses nombres, logos, prefijos ni estética de "ZH3" ni "ZHEYA" en ningún archivo, string, metadata o asset.
- **Autor/publisher:** Zilla (usar `Zilla` como company/publisher en metadata del .exe).
- **Idioma de la UI:** español (México) por defecto + inglés. Todo string en archivos de localización (`src/lib/i18n/es-MX.json` y `en.json`), nada hardcodeado.
- **Comunicación e internacionalización:** por defecto responde en español mexicano, conciso y técnico. Si el usuario se comunica o solicita trabajar en inglés o en flujos de internacionalización, responde y genera todo texto en inglés técnico natural, profesional y cinematográfico, sin filtraciones de español. No traducir rutas (`VelzaIris-data/`), nombres de paquetes, endpoints OSC (`/usercamera/*`), propiedades de modelos (`rawPose`, `knotMode`, etc.) ni contenido generado por el usuario.

### Diferenciación (lo que NO debemos copiar)
- La cámara de VRChat ya guarda presets internos y existe **CameraMaster OSC** (Booth) que guarda 8–16 posiciones **absolutas en el mundo**.
- Velza Iris se diferencia en:
  1. **Presets relativos al jugador** (portables entre mundos), no coordenadas absolutas.
  2. **Presets de "look" completos** (zoom + exposición + enfoque + apertura + máscaras + orientación + Look-At-Me offsets), como perfiles de cámara real.
  3. **Motor de movimiento** (órbitas, turnaround, push-in, crane, dolly zoom, handheld sutil) generado por la app, no dependiente de dibujar dolly paths.
  4. **Librería compartible**: packs `.iris` importables/exportables + códigos cortos para compartir.
  5. **Portable de verdad y auditable** (ver sección 6).

---

## 1. Superficie OSC disponible (VRChat 2025.3.3+)

Todos read/write. Envío a `127.0.0.1:9000`, escucha en `9001` — **preferir OSCQuery** (mDNS `_oscjson._tcp` / `_osc._udp`) para descubrir puertos y no chocar con otras apps OSC.

**Modo:** `/usercamera/Mode` → 0 Off, 1 Photo, 2 Stream, 3 Emoji, 4 Multilayer, 5 Print, 6 Drone.
**Pose:** `/usercamera/Pose` → posición + rotación.
**Acciones:** `/usercamera/Close`, `/usercamera/Capture`, `/usercamera/CaptureDelayed`.
**Toggles (bool):** `ShowUIInCamera`, `Lock`, `LocalPlayer`, `RemotePlayer`, `Environment`, `GreenScreen`, `SmoothMovement`, `LookAtMe`, `AutoLevelRoll`, `AutoLevelPitch`, `Flying`, `TriggerTakesPhotos`, `DollyPathsStayVisible`, `CameraEars`, `ShowFocus`, `Streaming` (Spout), `RollWhileFlying`, `OrientationIsLandscape`.
**Sliders (float) — rangos oficiales:**

| Endpoint | Default | Min | Max |
|---|---|---|---|
| Zoom (focal, mm) | 45 | 20 | 300 (medido en M0: es una focal, no un FOV; el prompt original decía FOV 20–150) |
| Exposure | 0 | -10 | 4 |
| FocalDistance (m) | 1.5 | 0 | 10 |
| Aperture (f/) | 15 | 1.4 | 32 |
| Hue / Saturation / Lightness (greenscreen) | 120/100/60 | 0/0/0 | 360/100/50 |
| LookAtMeXOffset / YOffset | 0/0 | -25 | 25 |
| FlySpeed | 3 | 0.1 | 15 |
| TurnSpeed | 1 | 0.1 | 5 |
| SmoothingStrength | 5 | 0.1 | 10 |
| PhotoRate (dolly) | 1 | 0.1 | 2 |
| Duration (dolly, s) | 2 | 0.1 | 60 |

**Clampa SIEMPRE** cada valor a su rango antes de enviar.

### Limitaciones conocidas (diseñar alrededor de ellas)
- La cámara **solo se mueve si ya está abierta**; históricamente **no hay endpoint para abrirla** y `Mode` no la abre si está cerrada. → La UI debe detectar "cámara cerrada" y pedir al usuario abrirla (y re-aplicar el preset al detectar apertura).
- Al reposicionar por OSC, la cámara puede entrar momentáneamente en modo vuelo.
- En builds tempranas hubo bugs donde **los sliders no notificaban cambios** y los offsets Look-At-Me X/Y se reportaban cruzados. → No confíes ciegamente en el eco; mantén estado propio como fuente de verdad y re-sincroniza.
- **No existe OSC para la posición del jugador.** → Ver sección 3.1 (sistema de ancla).
- Algunos sliders solo tienen efecto según el modo de enfoque (Full Auto / Semi Auto / Manual): Aperture requiere Semi Auto o Manual; FocalDistance requiere Manual.

---

## 2. Milestone 0 — Sonda OSC (OBLIGATORIO antes de construir)

Antes de escribir la app, crea `tools/probe` (CLI pequeño) que:
1. Descubra VRChat por OSCQuery y vuelque el árbol `/usercamera` (tipos, rangos, acceso).
2. Registre en vivo todo lo que llega en `/usercamera/*` a un log con timestamps.
3. Determine empíricamente:
   - Formato exacto de `/usercamera/Pose` (¿6 floats pos XYZ + rot Euler en grados? ¿orden de ejes? ¿espacio mundo o relativo al anchor?).
   - Si con el **Anchor** de la cámara en modo local/attached la pose reportada/aceptada es **relativa al jugador**. Si sí, es la base de los presets portables.
   - Unidades reales de `LookAtMeX/YOffset` (¿% del frame? ¿grados?).
   - Si `Zoom` es FOV vertical u horizontal.
   - Frecuencia máxima segura de envío de `Pose` (probar 30/60/90 Hz) y su interacción con `SmoothMovement`.
   - Qué notificaciones de sliders sí llegan en la build actual.
4. Guarda los hallazgos en `docs/osc-findings.md` y **ajusta este plan** según lo que encuentres. Si algo de este prompt contradice la sonda, gana la sonda.

### Resultado de M0 (detalle en `docs/osc-findings.md`)
- **Pose** = 6 floats `x y z` (m, mundo) + `rx ry rz` (Euler °, normalizados a [0,360)); `y` arriba, `rx` pitch, `ry` yaw, `rz` roll. Escribible. Posición sin clamp. **Pitch limitado a ±90°** (más se reinterpreta). Tolerancia de comparación ≥1e-3.
- **Anchor:** la Pose es de **mundo** aunque el anchor siga al jugador → **Modo A descartado, usar Modo B** (Calibrar ancla). Con anchor World la cámara no se mueve.
- **LookAtMe X/Y offsets:** grados (±25°), X = yaw, Y = pitch, sumados a la mirada hacia el jugador.
- **Frecuencia:** VRChat emite eco de Pose a ~61–65 Hz como tope → enviar a **60 Hz**. VRChat solo emite Pose cuando cambia.
- **Eco:** sliders (Zoom, Exposure, Aperture, FocalDistance, FlySpeed, TurnSpeed, LookAtMeX/Y, SmoothingStrength), toggles (LookAtMe, SmoothMovement, AutoLevelPitch) y `Mode` sí lo emiten. Sin probar: PhotoRate, Duration, Hue/Saturation/Lightness.
- **Rangos:** OSCQuery los reporta todos `null`; los clamps salen de la tabla de arriba (Zoom 20–300).
- **Cámara:** `Mode` no la abre. Eco `Mode 0` = cerrada, `≠0` = abierta (cuando el cambio lo origina el usuario); los `Mode` que escribe la app con la cámara cerrada también devuelven eco, hay que ignorarlos.
- **Zoom = focal en mm** (medido: el tamaño en cuadro es proporcional al valor; 30→45→90 da ×1.5 y ×2.0). A mayor valor, más cerca. Los FOV del prompt se tradujeron con `mm = 12 / tan(FOV/2)` (ej. 25° → 54 mm). Dolly zoom: `zoom = k·distancia`.

---

## 3. Funcionalidad

### 3.1 Sistema de ancla (presets relativos)
- **Modo A (preferido):** si la sonda confirma pose relativa con anchor local → guardar/aplicar poses en ese espacio.
- **Modo B (fallback, "Calibrar ancla"):** el usuario acerca la cámara a su cara mirando al frente y pulsa *Calibrar*; la app guarda esa pose como origen (posición de ojos + yaw). Todos los presets se expresan como offsets en coordenadas del ancla: `distancia`, `altura relativa a ojos`, `yaw alrededor del sujeto`, `pitch`, `roll`.
- Para órbitas y encuadres activar `LookAtMe` y mover solo la posición: la cámara apunta sola al jugador; los offsets LAM colocan al sujeto en tercios.

### 3.2 Presets de look/encuadre (semillas incluidas)
Equivalencia aproximada FOV→focal full-frame si Zoom es FOV vertical: `f_mm = 12 / tan(FOV/2)` → 20°≈68 mm, 25°≈54 mm, 30°≈45 mm, 35°≈38 mm, 45°≈29 mm, 60°≈21 mm, 90°≈12 mm. Muestra "≈ mm" en la UI (marcado como aproximado hasta confirmarlo con la sonda).

| Preset | Dist. | Altura vs ojos | Yaw | Pitch/Roll | FOV | Enfoque | Apertura | Exp. | Notas |
|---|---|---|---|---|---|---|---|---|---|
| Retrato ¾ | 1.3 m | +0.05 m | 35° | auto-level on | 25 | Manual, focal = dist | f/2.0 | 0 | Sujeto en tercio vertical, headroom ligero |
| Close-up íntimo | 0.7 m | 0 | 20° | — | 22 | Manual | f/1.4 | 0 | Fondo muy desenfocado |
| Heroico (contrapicado) | 1.8 m | -0.6 m | 15° | pitch auto (LAM) | 35 | Semi Auto | f/4 | 0 | Da presencia y escala |
| Picado suave | 1.0 m | +0.4 m | 10° | — | 40 | Semi Auto | f/2.8 | +0.3 | Estilo selfie tierno |
| Plano americano | 2.5 m | -0.2 m | 25° | — | 35 | Semi Auto | f/4 | 0 | De rodillas/muslos hacia arriba |
| Cuerpo completo (ref) | 3.5 m | -0.75 m (cadera) | 0° | roll+pitch auto | 30 | Manual | f/11 | 0 | Todo nítido, base para ref sheets |
| Ambiente amplio | 5 m | 0 | 30° | — | 70 | Full Auto | f/8 | 0 | Mundo + avatar |
| Dutch angle | 1.5 m | 0 | 30° | roll 12°, auto-roll off | 30 | Semi Auto | f/2.8 | 0 | Tensión, estilo videoclip |
| Grupal | 3 m | +0.1 m | 0° | auto | 55 | Manual | f/8 | 0 | Todos en foco |
| Silueta / contraluz | 2 m | 0 | 0° | — | 35 | Full Auto | f/5.6 | -2.0 | Para mundos con luz fuerte detrás |
| Mundo nocturno (modificador) | — | — | — | — | — | — | — | +1.0 | Modificador apilable sobre otros presets |

Principios a codificar como ayudas (overlay opcional y "sugerencias"): regla de tercios, headroom, lead room (espacio hacia donde mira el sujeto), evitar cortar en articulaciones (cuello, codos, rodillas), FOV cerrado (20–30) para retratos favorecedores con menos distorsión, FOV abierto (60–90) solo para ambiente o efectos, aperturas abiertas (f/1.4–2.8) para separar sujeto/fondo, cerradas (f/8–16) para grupos y ref sheets.

### 3.3 Motor de movimiento
Curvas con easing (sine/cubic in-out por defecto), actualización a la frecuencia validada en la sonda, interpolación en el espacio del ancla, `SmoothMovement` con `SmoothingStrength` 6–8 para video.

| Preset | Comportamiento |
|---|---|
| Órbita lenta 360° | r 1.5 m, altura ojos, 24 s/vuelta, LookAtMe on |
| Arco ¾ ping-pong | ±45° alrededor del frente, 8 s por tramo |
| Hélice | órbita 360° subiendo de cadera a +0.5 m sobre ojos en 20 s |
| Push-in | 2.5 m → 1.0 m en 6 s, FOV 35 → 28 |
| Pull-out reveal | 0.8 m → 4 m en 8 s, FOV 30 → 55 |
| Crane up | altura cadera → +1.5 m en 6 s, pitch vía LAM |
| Dolly zoom (vértigo) | retroceder mientras se cierra el FOV manteniendo el tamaño del sujeto: `FOV = 2·atan(H / (2·d))` |
| Handheld sutil | ruido Perlin ±1.5 cm / ±0.4° a ~0.3 Hz sobre cualquier preset |
| **Turnaround (ref sheet)** | 8 paradas cada 45°, r 3.5 m, FOV 30, f/11, Manual; `RemotePlayer` off, `Environment` off, `GreenScreen` on; pausa 1.5 s y `Capture` en cada parada. Variante de 4 vistas (frente, ¾, perfil, espalda). |

Controles: play/pausa/stop, velocidad global, invertir dirección, loop, botón de pánico que detiene todo y vuelve al último preset estático.

### 3.4 Presets de video
- **Stream vertical 9:16** (TikTok/Shorts): `Mode` 2, `OrientationIsLandscape` false, `Streaming` (Spout) on, smoothing 7.
- **Stream horizontal cine**: landscape, FOV 30–35, smoothing 8, `CameraEars` on.
- **Videoclip**: Dutch + handheld sutil + órbita lenta.
- Encadenar presets en una **línea de tiempo simple** (clips con duración y transición cut/ease).

### 3.5 Librería y compartir
- Formato de preset: JSON versionado con schema (`schemaVersion`), validado al importar (rechazar valores fuera de rango, claves desconocidas ignoradas con warning).
- **Pack `.iris`**: ZIP con `pack.json`, presets, y miniaturas **SVG**. Nada ejecutable dentro de un pack.
- **Código corto**: preset individual serializado → comprimido → base64url, para pegar en Discord.
- Categorías: Retrato, Cuerpo, Ambiente, Movimiento, Video, Ref sheet, Favoritos. Búsqueda, etiquetas, duplicar, renombrar, reordenar.

### 3.6 Disparadores
- Hotkeys globales configurables (desactivadas por defecto, opt-in).
- **Control remoto desde el celular (opt-in, apagado por defecto):** servidor web local solo en LAN, QR para conectar, token aleatorio por sesión, sin acceso externo.
- Opcional/experimental: combos con gestos estándar de mano (`GestureLeft/Right`, lectura) para cambiar preset; funciona con cualquier avatar.

---

## 4. Stack tecnológico (evaluación y decisión)

| Opción | Pros | Contras |
|---|---|---|
| Python + PyInstaller | El más rápido de iterar | Los .exe de PyInstaller suelen dar **falsos positivos de antivirus** → mata la confianza; pesados |
| C# / .NET 8 single-file (WPF/Avalonia) | Maduro, buen OSC (OscCore/VRC libs) | Self-contained pesa 60–150 MB; AOT con UI es más complejo |
| Go + Wails | Binario pequeño, simple | Ecosistema OSCQuery menor |
| **Rust + Tauri 2 + Svelte 5/TypeScript** | .exe pequeño (~5–15 MB), usa WebView2 del sistema, rápido, seguro por diseño (permisos/capabilities), UI web flexible | Curva de Rust (mitigable: la lógica OSC es acotada) |

**Decisión: Tauri 2 + Rust + Svelte 5 + TypeScript + Vite.**
- OSC: crate `rosc`. OSCQuery: `mdns-sd` + cliente HTTP mínimo (o crate `vrchat_osc` si cubre lo necesario; evalúalo).
- Motor de movimiento y estado en Rust (hilo con ticker de tiempo fijo); la UI solo manda intenciones.
- Si tras la sonda Tauri presenta un bloqueo real, documenta el motivo y propón alternativa antes de cambiar.

---

## 5. Diseño visual e iconografía

### 5.1 Dirección estética
Conceptual, onírica y suave — inspirada en estética tipo Humbe (atmósferas de ensueño, gradientes aterciopelados, grano fino, desenfoques), **no cyberpunk, no neón agresivo, no HUD militar**.

Paleta:
- Fondo: `#0E0A1A` (noche violeta) / superficie `#1A1230`
- Velvet profundo: `#2A1446`
- Azul real: `#2741C8`
- Azul eléctrico (acento principal): `#3D5AFE`
- Lavanda (acento secundario/hover): `#B8A9FF`
- Texto: `#ECE8FF` / secundario `#A59CC7`

Tipografía embebida localmente (sin CDN, la app es offline): `Manrope` para UI y `Fraunces` solo para títulos grandes (licencia OFL). Bordes redondeados 12–16 px, sombras suaves, transiciones 150–250 ms con ease-out, `prefers-reduced-motion` respetado.

### 5.2 Iconos — SOLO VECTORIALES
- **Prohibido:** emojis como iconos, PNG/JPG/GIF para iconografía, icon fonts rasterizadas.
- **Permitido:** SVG inline. Base: **Lucide** (`lucide-svelte`, licencia ISC). Iconos que falten se dibujan a mano como SVG con el mismo estilo (stroke 1.75 px, `stroke-linecap/linejoin: round`, grid 24×24, `currentColor`).
- Miniaturas de presets: SVG generativos (silueta + encuadre + flecha de movimiento), no capturas.

### 5.3 Icono del .exe (inventarlo y producirlo en esta sesión)
Concepto: **"Iris de pétalos"**.
- Seis hojas de diafragma dibujadas como **pétalos curvos** que forman una apertura hexagonal suave (cámara + flor + ojo).
- Gradiente en los pétalos de azul eléctrico `#3D5AFE` → velvet `#2A1446`.
- Dentro de la apertura, un **creciente lunar** lavanda `#B8A9FF` (lo onírico).
- Arriba a la derecha, un **destello de 4 puntas** pequeño en lavanda claro (reflejo de lente).
- Fondo: squircle `#120B24` con un leve resplandor radial violeta.
- Debe leerse a 16 px: crea una **versión simplificada** para 16/24 px (sin destello, 6 pétalos sólidos).

Entregables: `assets/icon/iris.svg` (maestro), `iris-small.svg` (16–24 px), script que rasteriza el SVG con `resvg` a PNG 1024 y genera `.ico` multi-tamaño (16, 24, 32, 48, 64, 128, 256) con `tauri icon`. El SVG maestro es la única fuente; nada de iconos dibujados en raster.

---

## 6. Portable y confiable (requisito, no extra)

- **Un solo .exe**, sin instalador, sin permisos de administrador, sin escribir en el registro.
- Config, presets y logs en una carpeta `VelzaIris-data/` **junto al .exe** (modo portable). Si la carpeta no es escribible, avisar y ofrecer `%LOCALAPPDATA%` como alternativa explícita.
- **Cero telemetría, cero conexiones a internet.** Solo localhost (y LAN si el usuario activa el control remoto). Documentarlo en el README y en la pantalla "Acerca de".
- Open source en GitHub (licencia MIT), **builds reproducibles con GitHub Actions**, publicar **SHA-256** de cada release y enlace a VirusTotal.
- Evaluar firma de código gratuita para OSS (p. ej. SignPath Foundation) y documentar el paso; no bloquea el MVP.
- Metadata del .exe completa: nombre, versión, publisher `Zilla`, descripción, icono.
- Tauri: capabilities mínimas (sin acceso a shell, sin fs fuera de la carpeta de datos, CSP estricta).

---

## 7. Arquitectura sugerida

```
velza-iris/
├─ src-tauri/
│  ├─ src/
│  │  ├─ osc/        # cliente OSC, OSCQuery, mapa de endpoints, clamps
│  │  ├─ camera/     # estado de cámara (fuente de verdad), sync, detección abierta/cerrada
│  │  ├─ anchor/     # modos A/B, transformaciones ancla ↔ mundo
│  │  ├─ motion/     # ticker, easing, órbitas, dolly zoom, ruido, turnaround
│  │  ├─ presets/    # schema, validación, packs .iris, códigos cortos
│  │  ├─ remote/     # servidor LAN opt-in con token
│  │  └─ storage/    # carpeta portable
│  └─ capabilities/
├─ src/              # Svelte 5 + TS (UI)
│  ├─ lib/icons/     # SVG propios estilo Lucide
│  ├─ lib/i18n/      # es-MX, en
│  └─ routes/        # Encuadres, Movimiento, Video, Librería, Ajustes
├─ assets/icon/      # iris.svg, iris-small.svg
├─ tools/probe/      # sonda OSC (Milestone 0)
└─ docs/             # osc-findings.md, presets.md, privacy.md
```

---

## 8. Milestones

1. **M0 – Sonda OSC** y `docs/osc-findings.md`. Detente y muéstrame hallazgos.
2. **M1 – Núcleo:** conexión OSCQuery, estado de cámara, aplicar/guardar presets estáticos, ancla A/B, clamps.
3. **M2 – Movimiento:** órbitas, push/pull, crane, dolly zoom, handheld, turnaround con captura.
4. **M3 – Librería:** packs `.iris`, códigos cortos, categorías, miniaturas SVG.
5. **M4 – Video y timeline** + control remoto LAN opt-in.
6. **M5 – Identidad y empaquetado:** icono final, i18n, build portable, CI, SHA-256, README.

Al cerrar cada milestone: build funcionando, pruebas unitarias del motor (easing, transformaciones de ancla, clamps, validación de schema) y un resumen corto de qué cambió y qué sigue.

---

## 9. Reglas de trabajo

- No inventes endpoints: si no está en la sección 1 o en la sonda, no existe.
- Pregunta antes de decisiones irreversibles (licencia, nombre público final, formato de pack v1).
- Commits pequeños y descriptivos; nada de dependencias innecesarias.
- Antes de publicar, verifica que el nombre "Velza Iris" no choque con otro producto de VRChat/Booth/GitHub y avísame si hay conflicto.
