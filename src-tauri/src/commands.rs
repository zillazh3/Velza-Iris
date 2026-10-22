//! Comandos Tauri: puente entre la UI y camera/anchor/presets.
use crate::anchor::Anchor;
use crate::camera::{Camera, Change};
use crate::osc::query;
use crate::presets::{self, Preset};
use crate::storage;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

pub struct App {
    camera: Mutex<Option<Camera>>,
    anchor: Mutex<Option<Anchor>>,
    /// Último preset estático aplicado; se reaplica cuando el usuario reabre la cámara.
    last: Mutex<Option<Preset>>,
    data: Result<PathBuf, String>,
}

impl App {
    pub fn new() -> App {
        App { camera: Mutex::default(), anchor: Mutex::default(), last: Mutex::default(), data: storage::data_dir(false) }
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
    st.apply(&preset)?;
    if !preset.modifier {
        *st.last.lock().unwrap() = Some(preset);
    }
    Ok(())
}
