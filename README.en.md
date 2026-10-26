<div align="center">

# 🎥 Velza Iris

**Your cinematography studio for VRChat.**
Control VRChat's official camera over OSC: framings, motions and sequences, with no VRC+ and no 3D math.

[🇲🇽 Español](README.md) · 🇺🇸 English

![Velza Iris home screen in dark theme](docs/images/en/home.png)

</div>

---

## ✨ Features

- 📸 **Framings:** a catalog of portraits, body shots, environment and video looks (zoom, aperture, exposure and focus), one click to apply.
- 🌀 **Built-in motions:** orbits, helix, push-in, pull-out, crane, dolly zoom and handheld, plus **Turnaround** for reference sheets.
- 🎬 **Iris Studio:** build sequences by capturing points (A, B, C…) or recording a continuous path; refine them with a timeline, effects, curves and a 3D spatial view.
- 📚 **Library:** save, open, import and export sequences as JSON, with **VRChat Camera Dolly** support.
- 🧭 **Player-relative:** calibrate an anchor and your framings and sequences work in any world.
- 🕺 **Avatar visualizer:** an in-game camera gizmo and 5-bone path (requires the Unity prefab; see [`docs/avatar-visualizer.md`](docs/avatar-visualizer.md), written in Spanish).
- 🌐 **English and Spanish**, dark/light theme and a selectable accent color.
- 🔒 **Portable and telemetry-free:** no internet access; it only talks to VRChat on your PC.

## 📸 Screenshots

| Framings | Iris Studio · Capture |
| :---: | :---: |
| ![Framings catalog](docs/images/en/framings.png) | ![Iris Studio in Capture mode](docs/images/en/studio-capture.png) |
| **Iris Studio · Editor** | **Library** |
| ![Iris Studio in Editor mode](docs/images/en/studio-editor.png) | ![Sequence library](docs/images/en/library.png) |

> Screenshots were taken with VRChat running but its camera closed, hence the yellow notice.

## 📦 Download (portable)

1. Download the latest `.zip` (`VelzaIris-vX.Y.Z-win-x64-portable.zip`) from the **Releases** section.
2. Extract it anywhere and run `velza-iris.exe`. There is no installer.
3. Your data lives in `VelzaIris-data/` next to the `.exe` (or in `%LOCALAPPDATA%` if you choose that in Settings). To update, replace only the `.exe` and keep that folder.

**Requirements:** 64-bit Windows 10/11, Microsoft Edge WebView2 (included in Windows 11) and VRChat with OSC enabled.

## 🚀 How to use it

1. 🎮 In VRChat, enable OSC under **Action Menu → Options → OSC → Enabled**.
2. 📷 Open VRChat's camera (Velza Iris can't open it for you).
3. ▶️ Run `velza-iris.exe`. The top bar should read **VRChat connected** and **Camera open**.
4. 🎯 Bring the camera close to your face looking forward, nudge it slightly and press **Calibrate anchor**. Recalibrate after teleporting.
5. 🖼️ Go to **Framings** to apply a shot or motion, or to **Iris Studio** to build your own sequence: capture Point A, move, capture Point B and press **Play in VRChat**.

Full step-by-step guide: [`docs/user_guide.en.md`](docs/user_guide.en.md).

## 🛠️ Tech stack

| Layer | Technology |
| --- | --- |
| Desktop shell | [Tauri 2](https://tauri.app) |
| Backend | Rust (2024 edition) · [`rosc`](https://crates.io/crates/rosc) (OSC) · [`mdns-sd`](https://crates.io/crates/mdns-sd) (OSCQuery/mDNS) · `serde` / `serde_json` |
| Frontend | [Svelte 5](https://svelte.dev) + [SvelteKit](https://svelte.dev/docs/kit) (`adapter-static`) · TypeScript |
| Build | [Vite](https://vite.dev) · [pnpm](https://pnpm.io) |
| UI | [Lucide](https://lucide.dev) (icons) · Manrope (typeface) · custom i18n (`es-MX` / `en`) |
| Protocol | OSC over UDP (send to `9000`, listen on `9001`) and VRChat OSCQuery |

## ⌨️ Commands

Run from the repository root in PowerShell. You need Node.js, pnpm, Rust and Cargo.

| Command | Purpose |
| --- | --- |
| `pnpm install` | Install the frontend dependencies. |
| `pnpm tauri dev` | Start the desktop app in development mode. |
| `pnpm dev` | Vite web dev server (no OSC connection). |
| `pnpm check` | Type-check TypeScript and Svelte components. |
| `pnpm test:i18n` | Verify `es-MX.json` and `en.json` are in sync and free of hardcoded text. |
| `pnpm test:logs` | Tests for the Logs view. |
| `pnpm build` | Build the web frontend (`build/`). |
| `pnpm tauri build --no-bundle` | Produce the release executable at `src-tauri/target/release/velza-iris.exe`. |

Tauri bundling is disabled (`bundle.active: false`), so no installer is generated: the portable build is the `.exe` compressed into a `.zip`.

### 🔎 OSC probe (`tools/probe`)

A command-line tool to discover OSCQuery, listen to and send OSC messages:

```powershell
cargo run --manifest-path tools/probe/Cargo.toml -- tree
cargo run --manifest-path tools/probe/Cargo.toml -- listen
cargo run --manifest-path tools/probe/Cargo.toml -- send /usercamera/Zoom 45
cargo run --manifest-path tools/probe/Cargo.toml -- rate 60 10
```

| Subcommand | Purpose |
| --- | --- |
| `tree [host:port]` | Finds VRChat via mDNS and saves the `/usercamera` tree to `docs/oscquery-usercamera.json`. |
| `listen [port]` | Listens to `/usercamera/*` on UDP 9001 and writes logs to `tools/probe/logs/`. |
| `send /usercamera/ENDPOINT VALUE [--to host:port]` | Sends an OSC message (`true`/`false` boolean, `i:N` integer, decimal number, or text). |
| `rate HZ SECONDS [RADIUS]` | Sends a test orbit and measures Pose echoes (default radius: `0.1` m). |

> ⚠️ `send` changes real camera settings and `rate` moves the camera. Open the camera first and close other OSC apps using UDP 9001.

## 🗂️ Project structure

```
src/                  Frontend (SvelteKit)
  routes/             Main views
  lib/studio/         Iris Studio (timeline, editor, 3D viewport)
  lib/i18n/           es-MX / en dictionaries
src-tauri/src/        Rust backend
  osc/ camera/        OSC/OSCQuery protocol and camera control
  motion/ studio/     Motions, sequences, recording and playback
  avatar/ anchor/     Avatar visualizer and anchor calibration
  presets/ storage/   Framings and portable data
tools/                OSC probe and verification scripts
docs/                 Guides and technical findings
```

## 🔒 Local data and privacy

Velza Iris uses no telemetry and no internet connection: it talks to VRChat over OSC/OSCQuery inside your local network. Presets, sequences and settings are stored in `VelzaIris-data/` next to the executable (or in `%LOCALAPPDATA%\VelzaIris-data`). Theme and language preferences are stored in the app's WebView.

## 📚 Documentation

- 📖 [User guide (English)](docs/user_guide.en.md) · [Guía de usuario (español)](docs/user_guide.es.md)
- 🔬 [`docs/osc-findings.md`](docs/osc-findings.md): OSC findings and tests (Spanish).
- 🕺 [`docs/avatar-visualizer.md`](docs/avatar-visualizer.md): avatar visualizer (Spanish).

## 📄 License

Released under the [MIT](LICENSE) license.
