//! Comandos Tauri: puente entre la UI y camera/anchor/presets.
use crate::anchor::Anchor;
use crate::camera::{Camera, Change};
use crate::motion::runner::{Cmd, Output, PlayState, Runner};
use crate::motion::sample::{motions, Motion};
use crate::osc::protocol::Value;
use crate::osc::query;
use crate::presets::{self, Preset};
use crate::storage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

pub struct App {
    camera: Mutex<Option<Camera>>,
    anchor: Mutex<Option<Anchor>>,
    /// Último preset estático aplicado; se reaplica cuando el usuario reabre la cámara.
    last: Mutex<Option<Preset>>,
    /// Movimiento en curso; soltarlo (`None`) detiene el hilo.
    runner: Mutex<Option<Runner>>,
    data: Result<PathBuf, String>,
}

impl App {
    pub fn new() -> App {
        App {
            camera: Mutex::default(),
            anchor: Mutex::default(),
            last: Mutex::default(),
            runner: Mutex::default(),
            data: storage::data_dir(false),
        }
    }

    fn library(&self) -> Library {
        let mut presets = presets::seeds();
        let mut warnings = Vec::new();
        match &self.data {
            Ok(dir) => {
                let (user, w) = presets::load_user(&dir.join("presets"));
                presets.extend(user);
                warnings = w;
            }
            Err(e) => warnings.push(e.clone()),
        }
        Library { presets, warnings }
    }

    fn apply(&self, preset: &Preset) -> Result<(), String> {
        let cam = self.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
        preset.apply(cam, self.anchor.lock().unwrap().as_ref())
    }
}

#[derive(Serialize)]
pub struct Library {
    presets: Vec<Preset>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    connected: bool,
    open: bool,
    anchored: bool,
}

/// Reaplica el último preset tras abrirse la cámara (en otro hilo para no frenar la recepción OSC).
fn reapply(app: AppHandle) {
    std::thread::spawn(move || {
        // ponytail: pausa fija para que VRChat termine de abrir; sin medir en vivo.
        std::thread::sleep(Duration::from_millis(300));
        let st = app.state::<App>();
        let last = st.last.lock().unwrap().clone();
        if let Some(p) = last {
            if let Err(e) = st.apply(&p) {
                eprintln!("no pude reaplicar '{}': {e}", p.name);
            }
        }
    });
}

#[tauri::command]
pub async fn connect(app: AppHandle, st: State<'_, App>) -> Result<(), String> {
    if st.camera.lock().unwrap().is_some() {
        return Ok(());
    }
    let target = tauri::async_runtime::spawn_blocking(|| query::vrchat_target(Duration::from_secs(8)))
        .await
        .map_err(|e| e.to_string())??;
    let cam = Camera::start(target, move |c| {
        if c == Change::Opened {
            reapply(app.clone());
        }
    })?;
    *st.camera.lock().unwrap() = Some(cam);
    Ok(())
}

#[tauri::command]
pub fn status(st: State<App>) -> Status {
    let open = st.camera.lock().unwrap().as_ref().is_some_and(|c| c.state.lock().unwrap().open);
    Status { connected: st.camera.lock().unwrap().is_some(), open, anchored: st.anchor.lock().unwrap().is_some() }
}

/// Toma la pose actual de la cámara (cerca de la cara, mirando al frente) como origen de los presets.
#[tauri::command]
pub fn calibrate(st: State<App>) -> Result<(), String> {
    let cam = st.camera.lock().unwrap();
    let state = cam.as_ref().ok_or("sin conexión con VRChat")?.state.lock().unwrap();
    if !state.open {
        return Err("cámara cerrada: ábrela en VRChat".into());
    }
    let pose = state.pose.ok_or("VRChat aún no reporta la pose: mueve un poco la cámara")?;
    *st.anchor.lock().unwrap() = Some(Anchor::from_pose(&pose));
    Ok(())
}

/// Devuelve las opciones de la cámara a los valores por defecto de VRChat y olvida el último preset.
#[tauri::command]
pub fn reset_defaults(st: State<App>) -> Result<(), String> {
    *st.runner.lock().unwrap() = None;
    st.camera.lock().unwrap().as_ref().ok_or("sin conexión con VRChat")?.reset()?;
    *st.last.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub fn list_presets(st: State<App>) -> Library {
    st.library()
}

#[tauri::command]
pub fn apply_preset(name: String, st: State<App>) -> Result<(), String> {
    let preset = st.library().presets.into_iter().find(|p| p.name == name).ok_or("el preset no existe")?;
    *st.runner.lock().unwrap() = None; // un encuadre estático reemplaza al movimiento en curso
    st.apply(&preset)?;
    if !preset.modifier {
        *st.last.lock().unwrap() = Some(preset);
    }
    Ok(())
}

// ---------- movimiento ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionStatus {
    name: Option<String>,
    state: PlayState,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MotionCmd {
    Play,
    Pause,
    Reverse,
    Loop(bool),
    Speed(f32),
}

#[tauri::command]
pub fn list_motions() -> Vec<Motion> {
    motions()
}

/// Reproduce un movimiento sobre el ancla. Activa LookAtMe y SmoothMovement (fuerza 7) y los deja así.
// ponytail: no restaura LookAtMe/smoothing al terminar; "Restablecer" los devuelve a los valores de VRChat.
#[tauri::command]
pub fn play_motion(name: String, app: AppHandle, st: State<App>) -> Result<(), String> {
    let motion = motions().into_iter().find(|m| m.name == name).ok_or("el movimiento no existe")?;
    let anchor = (*st.anchor.lock().unwrap()).ok_or("sin ancla: calibra primero")?;
    {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
        if !cam.state.lock().unwrap().open {
            return Err("cámara cerrada: ábrela en VRChat".into());
        }
        for (n, v) in [("LookAtMe", Value::B(true)), ("SmoothMovement", Value::B(true)), ("SmoothingStrength", Value::F(7.0))] {
            cam.set(n, v)?;
        }
    }
    let mut slot = st.runner.lock().unwrap();
    *slot = None; // detiene (y espera) al anterior antes de arrancar el nuevo
    *slot = Some(Runner::start(motion, move |o: &Output| {
        let st = app.state::<App>();
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
        let mut pose = anchor.pose(&o.sample.frame);
        if let Some((dp, dr)) = o.handheld {
            for i in 0..3 {
                pose.pos[i] += dp[i];
                pose.rot[i] += dr[i];
            }
        }
        cam.pose(&pose)?;
        if let Some(z) = o.sample.zoom {
            cam.set("Zoom", Value::F(z))?;
        }
        Ok(())
    }));
    Ok(())
}

#[tauri::command]
pub fn motion_control(cmd: MotionCmd, st: State<App>) -> Result<(), String> {
    let runner = st.runner.lock().unwrap();
    let r = runner.as_ref().ok_or("no hay movimiento en curso")?;
    r.send(match cmd {
        MotionCmd::Play => Cmd::Play,
        MotionCmd::Pause => Cmd::Pause,
        MotionCmd::Reverse => Cmd::Reverse,
        MotionCmd::Loop(on) => Cmd::Loop(on),
        MotionCmd::Speed(s) => Cmd::Speed(s),
    });
    Ok(())
}

#[tauri::command]
pub fn motion_status(st: State<App>) -> MotionStatus {
    match st.runner.lock().unwrap().as_ref() {
        Some(r) => {
            let s = r.status();
            MotionStatus { name: Some(r.name.clone()), state: s.state, error: s.error }
        }
        None => MotionStatus { name: None, state: PlayState::Stopped, error: None },
    }
}

/// Detiene el movimiento y deja la cámara donde está.
#[tauri::command]
pub fn stop_motion(st: State<App>) {
    *st.runner.lock().unwrap() = None;
}

/// Pánico: detiene todo y vuelve al último preset estático (si hubo uno).
#[tauri::command]
pub fn panic_stop(st: State<App>) -> Result<(), String> {
    *st.runner.lock().unwrap() = None;
    let last = st.last.lock().unwrap().clone();
    last.map_or(Ok(()), |p| st.apply(&p))
}
