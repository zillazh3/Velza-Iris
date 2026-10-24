//! Comandos Tauri: puente entre la UI y camera/anchor/presets/studio.
use crate::anchor::{Anchor, AnchorManager, AnchorSnapshot};
use crate::camera::{now_epoch_ms, Camera, CameraOscEvent, CameraSnapshot, Change, DEFAULTS};
use crate::motion::runner::{Cmd, Output, PlayState, Runner};
use crate::motion::sample::{motions, Motion};
use crate::motion::turnaround::{self, Action, Progress, Timing, Turnaround};
use crate::osc::protocol::Value;
use crate::osc::query;
use crate::presets::{self, Preset};
use crate::storage;
use crate::studio::calibration::{CalibrationRuntime, TelemetryReport};
use crate::studio::dolly::{export_vrc_dolly_json, import_vrc_dolly_json};
use crate::studio::evaluator::{evaluate, sample_trajectory, EvaluatedFrame, TrajectoryPreview};
use crate::studio::math3d::wrap180;
use crate::studio::model::{
    point_label, Keyframe, KnotMode, LookTrackPoint, RecordedSample, SequenceProject, TargetConfig, TargetSourceKind,
    Transition,
};
use crate::studio::player::{StudioCmd, StudioPlaybackStatus, StudioRunner};
use crate::studio::recorder::simplify_recording_to_keyframes;
use crate::studio::{delete_sequence, import_sequence_json, load_sequences, save_sequence};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

pub struct ActiveRecording {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<RecordedSample>>>,
    started_at: Instant,
}

pub struct App {
    camera: Mutex<Option<Camera>>,
    anchor_mgr: Arc<Mutex<AnchorManager>>,
    /// Último preset estático aplicado; se reaplica cuando el usuario reabre la cámara.
    last: Mutex<Option<Preset>>,
    /// Movimiento paramétrico en curso; soltarlo (`None`) detiene el hilo.
    runner: Mutex<Option<Runner>>,
    motion_prev_opts: Mutex<Option<Vec<(&'static str, Value)>>>,
    /// Reproductor de secuencias de Iris Studio; soltarlo (`None`) detiene el hilo.
    studio_runner: Mutex<Option<StudioRunner>>,
    /// Grabación continua de recorrido en curso.
    recording: Mutex<Option<ActiveRecording>>,
    turnaround: Mutex<Option<Turnaround>>,
    /// Estado dinámico de telemetría y calibración adaptativa (separado de los datos persistentes del proyecto).
    calibration_rt: Mutex<CalibrationRuntime>,
    data: Result<PathBuf, String>,
}

impl App {
    pub fn new() -> App {
        App {
            camera: Mutex::default(),
            anchor_mgr: Arc::new(Mutex::new(AnchorManager::new())),
            last: Mutex::default(),
            runner: Mutex::default(),
            motion_prev_opts: Mutex::default(),
            studio_runner: Mutex::default(),
            recording: Mutex::default(),
            turnaround: Mutex::default(),
            calibration_rt: Mutex::default(),
            data: storage::data_dir(false),
        }
    }

    /// Obtiene el ancla efectiva del operador local tras validar los plazos temporales.
    fn current_anchor(&self) -> Option<Anchor> {
        let mut mgr = self.anchor_mgr.lock().unwrap();
        mgr.check_timeouts(now_epoch_ms());
        mgr.active_anchor()
    }

    /// Obtiene una instantánea diagnóstica del gestor de ancla tras validar plazos temporales.
    fn anchor_snapshot(&self) -> AnchorSnapshot {
        let mut mgr = self.anchor_mgr.lock().unwrap();
        mgr.check_timeouts(now_epoch_ms());
        mgr.snapshot()
    }

    /// Detiene movimiento, reproducción de Studio y turnaround. No llamar con el lock de `camera` tomado.
    fn halt(&self) {
        *self.runner.lock().unwrap() = None;
        *self.studio_runner.lock().unwrap() = None;
        *self.turnaround.lock().unwrap() = None;
        self.anchor_mgr.lock().unwrap().set_external_control_active(false);
        if let Some(rec) = self.recording.lock().unwrap().take() {
            rec.stop.store(true, Ordering::Relaxed);
        }
        if let Some(prev) = self.motion_prev_opts.lock().unwrap().take() {
            if let Some(cam) = self.camera.lock().unwrap().as_ref() {
                for (n, v) in prev {
                    let _ = cam.set(n, v);
                }
            }
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
        let anchor = self.current_anchor();
        let cam = self.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
        if let (Some(f), Some(a)) = (&preset.frame, anchor) {
            self.anchor_mgr
                .lock()
                .unwrap()
                .record_commanded_pose(a.pose(f), now_epoch_ms());
        }
        preset.apply(cam, anchor.as_ref())
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
    anchor: Option<Anchor>,
    anchor_state: AnchorSnapshot,
    camera: Option<CameraSnapshot>,
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
    let mgr_arc = st.anchor_mgr.clone();
    let cam = Camera::start_with_events(target, move |change, osc_ev| {
        let mut is_fresh_open = matches!(change, Some(Change::Opened));
        if let Some(ev) = osc_ev {
            let mut mgr = mgr_arc.lock().unwrap();
            let prev_switches = mgr.snapshot().mode_switch_count;
            match ev {
                CameraOscEvent::Mode { new_mode, timestamp_ms, .. } => mgr.on_mode(i32::from(new_mode), timestamp_ms),
                CameraOscEvent::Pose { pose, seq, timestamp_ms } => mgr.on_pose(pose, seq, timestamp_ms),
                CameraOscEvent::LookAtMe { enabled, offsets_xy, timestamp_ms } => {
                    mgr.on_look_at_me(enabled, offsets_xy, timestamp_ms)
                }
            }
            // Si fue una transición de cambio de modo (p. ej. Photo 1 -> 0 -> Stream 2),
            // conservamos el ancla y el estado sin reinicializar el preset.
            if mgr.snapshot().mode_switch_count > prev_switches {
                is_fresh_open = false;
            }
        }
        if is_fresh_open {
            reapply(app.clone());
        }
    })?;
    *st.camera.lock().unwrap() = Some(cam);
    Ok(())
}

#[tauri::command]
pub fn status(st: State<App>) -> Status {
    let cam_guard = st.camera.lock().unwrap();
    let connected = cam_guard.is_some();
    let camera = cam_guard.as_ref().map(|c| c.state.lock().unwrap().snapshot());
    let open = camera.as_ref().is_some_and(|s| s.open);
    drop(cam_guard);
    let anchor_state = st.anchor_snapshot();
    let anchor = anchor_state.anchor;
    Status {
        connected,
        open,
        anchored: anchor.is_some(),
        anchor,
        anchor_state,
        camera,
    }
}

/// Toma la pose actual de la cámara (cerca de la cara, mirando al frente) como origen de los presets.
#[tauri::command]
pub fn calibrate(st: State<App>) -> Result<(), String> {
    let snap = {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
        cam.state.lock().unwrap().snapshot()
    };
    if !snap.open {
        return Err("cámara cerrada: ábrela en VRChat".into());
    }
    let pose = snap
        .pose
        .ok_or("VRChat aún no reporta la pose: mueve un poco la cámara")?;
    let now_ms = snap.pose_epoch_ms.unwrap_or_else(now_epoch_ms);
    st.anchor_mgr
        .lock()
        .unwrap()
        .calibrate_manual(pose, snap.pose_fresh, now_ms)?;
    Ok(())
}

/// Devuelve las opciones de la cámara a los valores por defecto de VRChat y olvida el último preset.
#[tauri::command]
pub fn reset_defaults(st: State<App>) -> Result<(), String> {
    st.halt();
    st.camera.lock().unwrap().as_ref().ok_or("sin conexión con VRChat")?.reset()?;
    *st.last.lock().unwrap() = None;
    Ok(())
}

/// Conmuta un toggle booleano de la cámara de VRChat (p. ej. "Streaming", "LookAtMe", "SmoothMovement").
#[tauri::command]
pub fn set_camera_toggle(name: String, enabled: bool, st: State<App>) -> Result<(), String> {
    let cam = st.camera.lock().unwrap();
    let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
    cam.set(&name, Value::B(enabled))
}

#[tauri::command]
pub fn list_presets(st: State<App>) -> Library {
    st.library()
}

#[tauri::command]
pub fn apply_preset(name: String, st: State<App>) -> Result<(), String> {
    let preset = st.library().presets.into_iter().find(|p| p.name == name).ok_or("el preset no existe")?;
    st.halt(); // un encuadre estático reemplaza al movimiento en curso
    st.apply(&preset)?;
    if !preset.modifier {
        *st.last.lock().unwrap() = Some(preset);
    }
    Ok(())
}

#[tauri::command]
pub fn save_user_preset(preset: Preset, st: State<App>) -> Result<(), String> {
    let dir = st.data.as_ref().map_err(|e| e.clone())?.join("presets");
    presets::save(&dir, &preset)
}

#[tauri::command]
pub fn import_preset_json(raw: String, st: State<App>) -> Result<Preset, String> {
    let (preset, _) = Preset::from_json(&raw)?;
    let dir = st.data.as_ref().map_err(|e| e.clone())?.join("presets");
    presets::save(&dir, &preset)?;
    Ok(preset)
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

/// Reproduce un movimiento sobre el ancla. Activa LookAtMe y SmoothMovement (fuerza 7) mientras reproduce y restaura al parar.
#[tauri::command]
pub fn play_motion(name: String, app: AppHandle, st: State<App>) -> Result<(), String> {
    let motion = motions().into_iter().find(|m| m.name == name).ok_or("el movimiento no existe")?;
    *st.turnaround.lock().unwrap() = None;
    *st.studio_runner.lock().unwrap() = None;
    let anchor = st.current_anchor().ok_or("sin ancla: calibra primero")?;
    let prev = {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
        let state = cam.state.lock().unwrap();
        if !state.open {
            return Err("cámara cerrada: ábrela en VRChat".into());
        }
        let opts = ["LookAtMe", "SmoothMovement", "SmoothingStrength"].map(|n| {
            let default = DEFAULTS.iter().find(|(k, _)| *k == n).map(|(_, v)| *v).unwrap();
            (n, state.values.get(n).copied().unwrap_or(default))
        });
        drop(state);
        for (n, v) in [("LookAtMe", Value::B(true)), ("SmoothMovement", Value::B(true)), ("SmoothingStrength", Value::F(7.0))] {
            cam.set(n, v)?;
        }
        opts.to_vec()
    };
    *st.motion_prev_opts.lock().unwrap() = Some(prev);
    st.anchor_mgr.lock().unwrap().set_external_control_active(true);
    let mut slot = st.runner.lock().unwrap();
    *slot = None;
    *slot = Some(Runner::start(motion, move |o: &Output| {
        let st = app.state::<App>();
        let mut pose = anchor.pose(&o.sample.frame);
        if let Some((dp, dr)) = o.handheld {
            for i in 0..3 {
                pose.pos[i] += dp[i];
                pose.rot[i] += dr[i];
            }
        }
        st.anchor_mgr.lock().unwrap().record_commanded_pose(pose, now_epoch_ms());
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
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
    st.halt();
}

/// Pánico: detiene todo y vuelve al último preset estático (si hubo uno).
#[tauri::command]
pub fn panic_stop(st: State<App>) -> Result<(), String> {
    st.halt();
    let last = st.last.lock().unwrap().clone();
    last.map_or(Ok(()), |p| st.apply(&p))
}

/// Turnaround: 8 paradas cada 45° (o 4 vistas) con captura.
#[tauri::command]
pub fn start_turnaround(four_views: bool, app: AppHandle, st: State<App>) -> Result<(), String> {
    st.halt();
    let anchor = st.current_anchor().ok_or("sin ancla: calibra primero")?;
    let prev: Vec<(&'static str, Value)> = {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
        let prev = {
            let state = cam.state.lock().unwrap();
            if !state.open {
                return Err("cámara cerrada: ábrela en VRChat".into());
            }
            ["RemotePlayer", "Environment", "GreenScreen", "SmoothMovement", "LookAtMe", "LookAtMeYOffset"].map(|n| {
                let default = DEFAULTS.iter().find(|(k, _)| *k == n).map(|(_, v)| *v).unwrap();
                (n, state.values.get(n).copied().unwrap_or(default))
            })
        };
        let setup = [
            ("RemotePlayer", Value::B(false)),
            ("Environment", Value::B(false)),
            ("GreenScreen", Value::B(true)),
            ("SmoothMovement", Value::B(false)),
            ("LookAtMe", Value::B(true)),
            ("LookAtMeYOffset", Value::F(turnaround::look_at_y_offset())),
            ("Zoom", Value::F(turnaround::ZOOM)),
            ("FocalDistance", Value::F(turnaround::DISTANCE)),
            ("Aperture", Value::F(11.0)),
        ];
        for (n, v) in setup {
            cam.set(n, v)?;
        }
        prev.to_vec()
    };
    st.anchor_mgr.lock().unwrap().set_external_control_active(true);
    let (go, done) = (app.clone(), app);
    let t = Turnaround::start(
        turnaround::yaws(four_views),
        Timing::default(),
        move |action| {
            let st = go.state::<App>();
            match action {
                Action::Go(yaw) => {
                    let pose = anchor.pose(&turnaround::frame(yaw));
                    st.anchor_mgr.lock().unwrap().record_commanded_pose(pose, now_epoch_ms());
                    let cam = st.camera.lock().unwrap();
                    let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
                    cam.pose(&pose)
                }
                Action::Capture => {
                    let cam = st.camera.lock().unwrap();
                    let cam = cam.as_ref().ok_or("sin conexión con VRChat")?;
                    cam.set("Capture", Value::B(true))
                }
            }
        },
        move || {
            let st = done.state::<App>();
            st.anchor_mgr.lock().unwrap().set_external_control_active(false);
            if let Some(cam) = st.camera.lock().unwrap().as_ref() {
                for (n, v) in prev {
                    let _ = cam.set(n, v);
                }
            }
        },
    );
    *st.turnaround.lock().unwrap() = Some(t);
    Ok(())
}

#[tauri::command]
pub fn turnaround_status(st: State<App>) -> Option<Progress> {
    st.turnaround.lock().unwrap().as_ref().map(|t| t.progress())
}

// ---------- Iris Studio ----------

/// Captura un Keyframe exacto desde la posición y rotación reales reportadas por la cámara de VRChat.
/// Si no hay datos válidos/recientes desde que se abrió la cámara, devuelve un error explícito.
#[tauri::command]
pub fn capture_camera_point(
    index: usize,
    time: f32,
    capture_optics: bool,
    st: State<App>,
) -> Result<Keyframe, String> {
    let snap = {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("No hay conexión activa con VRChat.")?;
        cam.state.lock().unwrap().snapshot()
    };
    if !snap.open {
        return Err("La cámara de VRChat está cerrada. Ábrela primero para capturar su posición.".into());
    }
    if !snap.pose_fresh {
        return Err("Aún no recibimos la posición actual de tu cámara. Muévela ligeramente en VRChat e inténtalo de nuevo.".into());
    }
    let raw_pose = snap
        .pose
        .ok_or("No pudimos leer la posición actual de tu cámara. Muévela un poco en VRChat.")?;
    let anchor = st.current_anchor();
    let local_pose = anchor.map(|a| a.world_to_local(&raw_pose));

    Ok(Keyframe {
        id: format!("kf-{}-{}", index + 1, snap.pose_seq),
        label: point_label(index),
        time: (time * 100.0).round() / 100.0,
        raw_pose,
        local_pose,
        captured_at_ms: snap.pose_epoch_ms.unwrap_or(0),
        transition: Transition::Smooth,
        custom_curve: None,
        knot_mode: KnotMode::PassThrough,
        look: if capture_optics {
            LookTrackPoint {
                zoom: snap.zoom,
                exposure: snap.exposure,
                focal_distance: snap.focal_distance,
                aperture: snap.aperture,
            }
        } else {
            LookTrackPoint::default()
        },
    })
}

/// Captura un punto de interés (`TargetConfig`) colocando la cámara de VRChat en el objetivo a encuadrar.
#[tauri::command]
pub fn capture_target_point(blend: f32, st: State<App>) -> Result<TargetConfig, String> {
    let snap = {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("No hay conexión activa con VRChat.")?;
        cam.state.lock().unwrap().snapshot()
    };
    if !snap.open {
        return Err("La cámara de VRChat está cerrada. Ábrela primero para capturar el objetivo.".into());
    }
    if !snap.pose_fresh {
        return Err("Mueve ligeramente tu cámara en VRChat sobre el objetivo para leer su posición.".into());
    }
    let raw_pose = snap
        .pose
        .ok_or("No pudimos leer la posición actual de tu cámara.")?;

    Ok(TargetConfig {
        enabled: true,
        source: TargetSourceKind::StaticCalibratedPoint,
        world_pos: raw_pose.pos,
        blend: blend.clamp(0.0, 1.0),
        confidence: 1.0,
        captured_at_ms: snap.pose_epoch_ms.unwrap_or(0),
    })
}

/// Evalúa la secuencia en un instante `time` (p. ej. al arrastrar el cursor) y opcionalmente envía la pose a VRChat.
#[tauri::command]
pub fn evaluate_sequence(
    project: SequenceProject,
    time: f32,
    send_to_camera: bool,
    st: State<App>,
) -> Result<EvaluatedFrame, String> {
    let anchor = st.current_anchor();
    // Si hay una secuencia reproduciéndose en vivo o en vista previa, sincronizar cualquier cambio de FX/puntos al instante
    if let Some(runner) = st.studio_runner.lock().unwrap().as_ref() {
        runner.send(StudioCmd::UpdateProject(project.clone(), anchor));
    }
    let frame = evaluate(&project, anchor.as_ref(), time);
    if send_to_camera {
        st.anchor_mgr
            .lock()
            .unwrap()
            .record_commanded_pose(frame.final_pose, now_epoch_ms());
        let cam_guard = st.camera.lock().unwrap();
        if let Some(cam) = cam_guard.as_ref() {
            if cam.state.lock().unwrap().open {
                let _ = cam.set("LookAtMe", Value::B(false));
                let _ = cam.pose(&frame.final_pose);
                if let Some(z) = frame.zoom {
                    let _ = cam.set("Zoom", Value::F(z));
                }
                if let Some(e) = frame.exposure {
                    let _ = cam.set("Exposure", Value::F(e));
                }
                if let Some(fd) = frame.focal_distance {
                    let _ = cam.set("FocalDistance", Value::F(fd));
                }
                if let Some(ap) = frame.aperture {
                    let _ = cam.set("Aperture", Value::F(ap));
                }
            }
        }
    }
    Ok(frame)
}

/// Genera la vista previa espacial 3D de la secuencia (nodos, curvas base y con efectos, distancias y ancla).
#[tauri::command]
pub fn preview_trajectory(project: SequenceProject, steps: usize, st: State<App>) -> TrajectoryPreview {
    let anchor = st.current_anchor();
    sample_trajectory(&project, anchor.as_ref(), steps)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StudioControlCmd {
    Play,
    Pause,
    Seek(f32),
    Loop(bool),
    Speed(f32),
}

/// Inicia la reproducción de una secuencia de Iris Studio (en modo Vista Previa local o en Vivo hacia VRChat OSC).
#[tauri::command]
pub fn play_sequence(
    project: SequenceProject,
    start_time: f32,
    looping: bool,
    live_osc: bool,
    app: AppHandle,
    st: State<App>,
) -> Result<(), String> {
    if project.keyframes.is_empty() {
        return Err("Agrega al menos un punto en la línea de tiempo antes de reproducir.".into());
    }
    st.halt();
    let anchor = st.current_anchor();
    if live_osc {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("Conecta con VRChat antes de reproducir en la cámara, o usa Vista Previa.")?;
        if !cam.state.lock().unwrap().open {
            return Err("Abre la cámara en VRChat para reproducir en vivo, o usa Vista Previa.".into());
        }
        // Ajustamos SmoothMovement de VRChat según el Perfil Cinematográfico activo de Iris Studio
        let (smooth_on, smooth_strength) = project.cinematic.vrchat_smoothing();
        for (n, v) in [
            ("LookAtMe", Value::B(false)),
            ("SmoothMovement", Value::B(smooth_on)),
            ("SmoothingStrength", Value::F(smooth_strength)),
        ] {
            cam.set(n, v)?;
        }
        st.anchor_mgr.lock().unwrap().set_external_control_active(true);
    }

    let cal_cfg = project.calibration.clone();
    let runner = StudioRunner::start(project, anchor, start_time, looping, live_osc, move |ev, send_osc| {
        if !send_osc {
            return Ok(());
        }
        let st = app.state::<App>();
        let now_ms = now_epoch_ms();
        let mut pose_to_send = ev.final_pose;
        {
            let mut cal = st.calibration_rt.lock().unwrap();
            cal.record_sent_pose(now_ms, ev.final_pose);
            if cal_cfg.enabled && cal_cfg.auto_correct {
                for i in 0..3 {
                    pose_to_send.pos[i] += cal.active_pos_correction[i];
                }
                pose_to_send.rot[0] = (pose_to_send.rot[0] + cal.active_rot_correction[0]).clamp(-89.9, 89.9);
                pose_to_send.rot[1] = wrap180(pose_to_send.rot[1] + cal.active_rot_correction[1]);
                pose_to_send.rot[2] = wrap180(pose_to_send.rot[2] + cal.active_rot_correction[2]);
            }
        }
        st.anchor_mgr.lock().unwrap().record_commanded_pose(pose_to_send, now_ms);
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("Se perdió la conexión con VRChat")?;
        cam.pose(&pose_to_send)?;
        if let Some(z) = ev.zoom {
            cam.set("Zoom", Value::F(z))?;
        }
        if let Some(e) = ev.exposure {
            cam.set("Exposure", Value::F(e))?;
        }
        if let Some(fd) = ev.focal_distance {
            cam.set("FocalDistance", Value::F(fd))?;
        }
        if let Some(ap) = ev.aperture {
            cam.set("Aperture", Value::F(ap))?;
        }
        Ok(())
    });

    *st.studio_runner.lock().unwrap() = Some(runner);
    Ok(())
}

#[tauri::command]
pub fn studio_control(cmd: StudioControlCmd, st: State<App>) -> Result<(), String> {
    let guard = st.studio_runner.lock().unwrap();
    let r = guard.as_ref().ok_or("No hay secuencia reproduciéndose")?;
    r.send(match cmd {
        StudioControlCmd::Play => StudioCmd::Play,
        StudioControlCmd::Pause => StudioCmd::Pause,
        StudioControlCmd::Seek(t) => StudioCmd::Seek(t),
        StudioControlCmd::Loop(on) => StudioCmd::Loop(on),
        StudioControlCmd::Speed(s) => StudioCmd::Speed(s),
    });
    Ok(())
}

#[tauri::command]
pub fn studio_status(st: State<App>) -> StudioPlaybackStatus {
    let status = st
        .studio_runner
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.status())
        .unwrap_or_default();
    if matches!(status.state, PlayState::Stopped) {
        st.anchor_mgr.lock().unwrap().set_external_control_active(false);
    }
    status
}

#[tauri::command]
pub fn stop_studio(st: State<App>) {
    *st.studio_runner.lock().unwrap() = None;
    st.anchor_mgr.lock().unwrap().set_external_control_active(false);
}

// ---------- Grabación continua de recorrido ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub active: bool,
    pub elapsed: f32,
    pub sample_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishedRecording {
    pub duration: f32,
    pub raw_samples: Vec<RecordedSample>,
    pub keyframes: Vec<Keyframe>,
}

#[tauri::command]
pub fn start_recording(app: AppHandle, st: State<App>) -> Result<(), String> {
    st.halt();
    {
        let cam = st.camera.lock().unwrap();
        let cam = cam.as_ref().ok_or("Conecta con VRChat antes de grabar un recorrido.")?;
        let snap = cam.state.lock().unwrap().snapshot();
        if !snap.open {
            return Err("Abre la cámara en VRChat antes de iniciar la grabación.".into());
        }
    }

    let stop = Arc::new(AtomicBool::new(false));
    let samples = Arc::new(Mutex::new(Vec::<RecordedSample>::new()));
    let started_at = Instant::now();

    let stop_clone = stop.clone();
    let samples_clone = samples.clone();
    std::thread::spawn(move || {
        let mut last_seq = 0_u64;
        while !stop_clone.load(Ordering::Relaxed) {
            {
                let st = app.state::<App>();
                let cam_guard = st.camera.lock().unwrap();
                if let Some(cam) = cam_guard.as_ref() {
                    let snap = cam.state.lock().unwrap().snapshot();
                    if snap.open && snap.pose_fresh {
                        if let Some(pose) = snap.pose {
                            let mut buf = samples_clone.lock().unwrap();
                            // Registrar en el primer instante o cada vez que VRChat emita un nuevo paquete Pose
                            if buf.is_empty() || snap.pose_seq != last_seq {
                                last_seq = snap.pose_seq;
                                buf.push(RecordedSample {
                                    time: started_at.elapsed().as_secs_f32(),
                                    pose,
                                    zoom: snap.zoom,
                                });
                            }
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    });

    *st.recording.lock().unwrap() = Some(ActiveRecording {
        stop,
        samples,
        started_at,
    });
    Ok(())
}

#[tauri::command]
pub fn recording_status(st: State<App>) -> RecordingStatus {
    match st.recording.lock().unwrap().as_ref() {
        Some(rec) => RecordingStatus {
            active: true,
            elapsed: rec.started_at.elapsed().as_secs_f32(),
            sample_count: rec.samples.lock().unwrap().len(),
        },
        None => RecordingStatus {
            active: false,
            elapsed: 0.0,
            sample_count: 0,
        },
    }
}

#[tauri::command]
pub fn stop_recording(
    pos_tol_m: f32,
    rot_tol_deg: f32,
    smoothing_passes: usize,
    st: State<App>,
) -> Result<FinishedRecording, String> {
    let rec = st.recording.lock().unwrap().take().ok_or("No hay grabación activa")?;
    rec.stop.store(true, Ordering::Relaxed);
    let duration = rec.started_at.elapsed().as_secs_f32().max(1.0);
    let raw_samples = rec.samples.lock().unwrap().clone();
    if raw_samples.is_empty() {
        return Err("No se recibieron movimientos de la cámara durante la grabación. Mueve la cámara en VRChat mientras grabas.".into());
    }
    let anchor = st.current_anchor();
    let keyframes = simplify_recording_to_keyframes(&raw_samples, anchor.as_ref(), pos_tol_m, rot_tol_deg, smoothing_passes);
    Ok(FinishedRecording {
        duration: (duration * 10.0).round() / 10.0,
        raw_samples,
        keyframes,
    })
}

#[tauri::command]
pub fn resimplify_recording(
    raw_samples: Vec<RecordedSample>,
    pos_tol_m: f32,
    rot_tol_deg: f32,
    smoothing_passes: usize,
    st: State<App>,
) -> Vec<Keyframe> {
    let anchor = st.current_anchor();
    simplify_recording_to_keyframes(&raw_samples, anchor.as_ref(), pos_tol_m, rot_tol_deg, smoothing_passes)
}

// ---------- Persistencia de secuencias ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceLibrary {
    pub sequences: Vec<SequenceProject>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub fn list_sequences(st: State<App>) -> SequenceLibrary {
    match &st.data {
        Ok(dir) => {
            let (sequences, warnings) = load_sequences(&dir.join("sequences"));
            SequenceLibrary { sequences, warnings }
        }
        Err(e) => SequenceLibrary {
            sequences: crate::studio::starter_sequences(),
            warnings: vec![e.clone()],
        },
    }
}

#[tauri::command]
pub fn save_sequence_cmd(mut project: SequenceProject, st: State<App>) -> Result<SequenceProject, String> {
    if let Some(anchor) = st.current_anchor() {
        if project.anchor_snapshot.is_none() {
            project.bind_anchor(anchor);
        }
    }
    project.sort_keyframes();
    let dir = st.data.as_ref().map_err(|e| e.clone())?.join("sequences");
    save_sequence(&dir, &project)?;
    Ok(project)
}

#[tauri::command]
pub fn delete_sequence_cmd(id: String, st: State<App>) -> Result<(), String> {
    let dir = st.data.as_ref().map_err(|e| e.clone())?.join("sequences");
    delete_sequence(&dir, &id)
}

#[tauri::command]
pub fn import_sequence_cmd(raw: String, st: State<App>) -> Result<SequenceProject, String> {
    let seq = import_sequence_json(&raw)?;
    let dir = st.data.as_ref().map_err(|e| e.clone())?.join("sequences");
    save_sequence(&dir, &seq)?;
    Ok(seq)
}

// ---------- Calibración, Telemetría y Adaptador Camera Dolly ----------

#[tauri::command]
pub fn telemetry_status(project: SequenceProject, st: State<App>) -> TelemetryReport {
    let snap = {
        let cam_guard = st.camera.lock().unwrap();
        cam_guard.as_ref().map(|c| c.state.lock().unwrap().snapshot())
    };
    let now_ms = now_epoch_ms();
    let anchor_state = {
        let mut mgr = st.anchor_mgr.lock().unwrap();
        mgr.check_timeouts(now_ms);
        mgr.state()
    };
    let zoom_mm = snap.as_ref().and_then(|s| s.zoom).unwrap_or(45.0);
    let mut cal = st.calibration_rt.lock().unwrap();
    cal.observe_and_update_with_anchor(
        snap.as_ref(),
        Some(anchor_state),
        now_ms,
        0.08,
        &project.calibration,
        zoom_mm,
        0.0,
    )
}

#[tauri::command]
pub fn reset_calibration_cmd(st: State<App>) -> TelemetryReport {
    let now_ms = now_epoch_ms();
    let mut cal = st.calibration_rt.lock().unwrap();
    cal.reset(now_ms, "Restaurado manualmente por el usuario");
    TelemetryReport::default()
}

/// Convierte un movimiento predefinido (`Motion`) en un `SequenceProject` editable para alternar entre Modo Simple y Avanzado.
#[tauri::command]
pub fn load_motion_as_sequence(name: String, st: State<App>) -> Result<SequenceProject, String> {
    let motion = motions()
        .into_iter()
        .find(|m| m.name == name)
        .ok_or("El movimiento predefinido solicitado no existe")?;
    let anchor = st.current_anchor();
    Ok(SequenceProject::from_preset_motion(&motion, anchor))
}

/// Importa un archivo `.json` oficial de VRChat Camera Dolly hacia un `SequenceProject` reproducible sin VRC+.
#[tauri::command]
pub fn import_vrc_dolly_cmd(raw: String, st: State<App>) -> Result<SequenceProject, String> {
    let anchor = st.current_anchor();
    let seq = import_vrc_dolly_json(&raw, anchor)?;
    if let Ok(dir) = &st.data {
        let _ = save_sequence(&dir.join("sequences"), &seq);
    }
    Ok(seq)
}

/// Exporta un `SequenceProject` al formato `.json` compatible con VRChat Camera Dolly.
#[tauri::command]
pub fn export_vrc_dolly_cmd(project: SequenceProject, bake_effects: bool, st: State<App>) -> Result<String, String> {
    let anchor = st.current_anchor();
    export_vrc_dolly_json(&project, anchor.as_ref(), bake_effects)
}

