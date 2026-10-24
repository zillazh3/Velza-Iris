# Velza Iris

Aplicación de escritorio para controlar la cámara oficial de VRChat mediante OSC. Incluye encuadres y ajustes de cámara, movimientos predefinidos y una sonda de línea de comandos para inspeccionar y probar la comunicación OSC.

## 🧰 Requisitos

- Windows.
- Node.js y pnpm para la interfaz y los comandos de desarrollo.
- Rust y Cargo para ejecutar o compilar la aplicación de escritorio y la sonda.
- VRChat abierto y OSC habilitado para conectar la aplicación con la cámara.

## 🚀 Preparar y ejecutar la aplicación

Desde PowerShell, en la carpeta del proyecto:

```powershell
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` inicia Velza Iris como aplicación de escritorio y compila el código Rust cuando hace falta. Para ejecutar únicamente el servidor de desarrollo web de Vite:

```powershell
pnpm dev
```

La interfaz web por sí sola no ofrece la conexión OSC de la aplicación de escritorio.

## 🎮 Conectar con VRChat

1. Abre VRChat y activa OSC en **Menú de acción → Opciones → OSC → Activado**.
2. Abre la cámara de VRChat. Velza Iris no puede abrirla por OSC.
3. Inicia la aplicación con `pnpm tauri dev`; se conectará a VRChat mediante OSCQuery.
4. Para usar encuadres y movimientos relativos al avatar, coloca la cámara cerca de la cara, mirando al frente, y pulsa **Calibrar ancla**. Recalibra después de teletransportarte.
5. Elige un encuadre o movimiento en la interfaz. Los movimientos requieren la cámara abierta y un ancla calibrada.

El movimiento **Turnaround** captura vistas alrededor del avatar. Sigue las indicaciones de la aplicación y pon el enfoque de la cámara de VRChat en modo Manual antes de iniciarlo.

## 🧪 Comandos de desarrollo

Ejecuta estos comandos desde la raíz del repositorio:

| Comando | Uso |
| --- | --- |
| `pnpm tauri dev` | Inicia la aplicación de escritorio en modo desarrollo. |
| `pnpm dev` | Inicia el servidor de desarrollo web de Vite. |
| `pnpm check` | Revisa los tipos y componentes Svelte. |
| `pnpm check:watch` | Repite la revisión mientras editas archivos. |
| `pnpm build` | Compila la interfaz web. |
| `pnpm preview` | Sirve localmente la compilación web para previsualizarla. |
| `pnpm tauri build` | Ejecuta la compilación de escritorio de Tauri. |

La configuración actual de Tauri tiene el empaquetado desactivado (`bundle.active: false`), así que `pnpm tauri build` no genera un instalador empaquetado en este estado del proyecto.

## 🔎 Sonda OSC (`tools/probe`)

La sonda sirve para descubrir el servicio OSCQuery de VRChat, escuchar mensajes OSC y enviar mensajes de prueba. Puedes ejecutarla desde la raíz del repositorio con Cargo:

```powershell
cargo run --manifest-path tools/probe/Cargo.toml -- tree
```

Comandos disponibles:

| Comando | Uso |
| --- | --- |
| `cargo run --manifest-path tools/probe/Cargo.toml -- tree` | Busca VRChat por mDNS, consulta OSCQuery y guarda el árbol de `/usercamera` en `docs/oscquery-usercamera.json`. |
| `cargo run --manifest-path tools/probe/Cargo.toml -- tree 127.0.0.1:PORT` | Consulta un host y puerto OSCQuery indicados manualmente. |
| `cargo run --manifest-path tools/probe/Cargo.toml -- listen` | Escucha `/usercamera/*` en UDP 9001, muestra los mensajes y los guarda en `tools/probe/logs/`. |
| `cargo run --manifest-path tools/probe/Cargo.toml -- listen PORT` | Escucha en otro puerto UDP. |
| `cargo run --manifest-path tools/probe/Cargo.toml -- send /usercamera/ENDPOINT VALUE` | Envía un mensaje OSC a `127.0.0.1:9000`. |
| `cargo run --manifest-path tools/probe/Cargo.toml -- rate HZ SEGUNDOS [RADIO]` | Envía una órbita de prueba a esa frecuencia y mide los ecos de Pose. El radio predeterminado es `0.1` metros. |

Ejemplos:

```powershell
cargo run --manifest-path tools/probe/Cargo.toml -- listen
cargo run --manifest-path tools/probe/Cargo.toml -- send /usercamera/Zoom 45
cargo run --manifest-path tools/probe/Cargo.toml -- send /usercamera/LookAtMe true
cargo run --manifest-path tools/probe/Cargo.toml -- send /usercamera/Mode i:1 --to 127.0.0.1:9000
cargo run --manifest-path tools/probe/Cargo.toml -- rate 60 10
```

Para `send`, `true` y `false` se interpretan como booleanos, `i:N` como enteros, los números como decimales y el resto como texto. Puedes cambiar el destino con `--to host:port`.

**Precaución:** `send` cambia el endpoint OSC indicado en VRChat. `rate` mueve la cámara en una órbita pequeña y luego intenta devolverla a la pose inicial. Abre la cámara antes de usarlo y cierra otras herramientas OSC que estén usando UDP 9001 antes de ejecutar `listen` o `rate`. Pulsa `Ctrl+C` para detener la escucha.

## 💾 Datos locales y privacidad

La aplicación busca y controla VRChat en la red local mediante OSC/OSCQuery. La interfaz indica que no usa telemetría ni conexión a internet. Al iniciar, carga los presets incluidos y busca presets adicionales en formato JSON en `VelzaIris-data/presets`, junto al ejecutable; durante desarrollo, la carpeta de datos queda junto al binario de desarrollo de Tauri. La sonda guarda sus logs en `tools/probe/logs/`.

## 📚 Documentación del proyecto

- [`docs/osc-findings.md`](docs/osc-findings.md): hallazgos y comandos de la sonda OSC.
- [`CLAUDE.md`](CLAUDE.md): contexto y plan técnico del proyecto.
