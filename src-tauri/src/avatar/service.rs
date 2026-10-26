//! Servicio de integración cinemática y sincronización del Visualizador VRChat (Avatars 3.0 / Unity).
//! Implementa los 3 flujos canónicos:
//! - Flujo A: Calibración y Reseteo del Ancla (`SetAnchor` / `ResetAnchor`).
//! - Flujo B: Deformación del Mesh del Path de 5 Huesos (`SyncPathMeshAsync`) con multiplexación temporal secuencial sin solapamiento.
//! - Flujo C: Actualización en tiempo real a 60 Hz del Visualizador de Cámara (`SyncCameraVisualizerFrame`) vía OSC Bundles.

use super::config::VisualizerConfig;
use super::protocol::*;
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use crate::studio::evaluator::sample_base_pose_at;
use crate::studio::model::SequenceProject;
use rosc::{OscMessage, OscType};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{spawn, JoinHandle};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Instantánea del estado del visualizador para la UI y diagnóstico.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizerSnapshot {
    pub enabled: bool,
    pub anchor_position: Option<[f32; 3]>,
    pub anchor_yaw: Option<f32>,
    pub hide_cam: bool,
    pub last_cam_pose: Option<Pose>,
    pub last_cam_normalized_pos: Option<[f32; 3]>,
    pub last_cam_normalized_rot: Option<[f32; 3]>,
    pub last_wp_samples: Option<Vec<[f32; 3]>>,
    pub is_syncing_path: bool,
    pub last_error: Option<String>,
    /// Ajuste fino acumulado del ancla (desplazamiento del gizmo en ejes del rig, m; y giro, °).
    pub nudge_pos: [f32; 3],
    pub nudge_yaw: f32,
    pub diag: VizDiagSnapshot,
}

/// Resultado de `sync_camera_frame`: qué pasó con la pose recibida.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncOutcome {
    Sent,
    Disabled,
    NoAnchor,
    Deadband,
}

/// Por qué no se mandó una pose al rig (diagnóstico de "el gizmo no sigue a la cámara").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    Disabled,
    NoAnchor,
    /// Hay un recorrido/movimiento en curso: el hilo de 60 Hz ya sincroniza la pose enviada.
    ExternalControl,
    Deadband,
    /// El sink falló (p. ej. el lock de `camera` estaba ocupado).
    Busy,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkipCounts {
    pub disabled: u64,
    pub no_anchor: u64,
    pub external_control: u64,
    pub deadband: u64,
    pub busy: u64,
}

/// Último valor de `/avatar/parameters/iris_*` que VRChat devolvió por 9001 (prueba de que el avatar lo aplicó).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarEchoEntry {
    pub name: String,
    pub value: f32,
    pub age_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VizDiagSnapshot {
    /// Ecos de `/usercamera/Pose` recibidos desde VRChat (la cámara real se movió o la movimos nosotros).
    pub pose_echoes: u64,
    /// Poses que sí se enviaron al rig (6 mensajes `iris_cam_*`).
    pub frames_sent: u64,
    pub skipped: SkipCounts,
    pub last_error: Option<String>,
    pub avatar_echo: Vec<AvatarEchoEntry>,
}

#[derive(Default)]
struct DiagInner {
    pose_echoes: u64,
    frames_sent: u64,
    skipped: SkipCounts,
    last_error: Option<String>,
    avatar_echo: HashMap<String, (f32, Instant)>,
}

/// Los 6 parámetros normalizados `(iris_cam_x/y/z, iris_cam_rx/ry/rz)` de una pose de mundo respecto al ancla.
/// Posición y yaw se expresan en los ejes del ancla (`Anchor::world_to_local`), que son los de `Iris_World_Root`.
fn camera_params(anchor: &Anchor, pose: &Pose) -> ([f32; 3], [f32; 3]) {
    let local = anchor.world_to_local(pose);
    (normalize_position(local.pos, [0.0; 3]), normalize_rotation(local.rot))
}

/// Mensaje de tarea para el worker de multiplexación de los 5 huesos.
struct PathSyncJob {
    version: u64,
    samples: [[f32; 3]; 5],
    anchor: [f32; 3],
    timing: MuxTiming,
    sink: Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync>,
}

/// Esperas del multiplexor de huesos. Con parámetros synced los demás jugadores reciben los cambios a unos ~10 Hz:
/// con los tiempos locales (30 ms) el `wp_index` transitorio ni llega y los waypoints remotos quedan sin mover.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MuxTiming {
    /// Tras escribir `wp_x/y/z` (el BlendTree posiciona `WP_Target`).
    pub settle_ms: u64,
    /// Con `wp_index = i` (el constraint del hueso salta a `WP_Target`).
    pub hold_ms: u64,
    /// Con `wp_index = 0` (el hueso queda congelado en mundo).
    pub freeze_ms: u64,
}

impl MuxTiming {
    /// Solo lo ve el propietario del avatar (params locales).
    pub const LOCAL: MuxTiming = MuxTiming { settle_ms: 30, hold_ms: 30, freeze_ms: 20 };
    /// Visible para otros jugadores. ponytail: valores estimados; calibrar con un segundo cliente (~3,3 s por sync).
    pub const REMOTE: MuxTiming = MuxTiming { settle_ms: 250, hold_ms: 250, freeze_ms: 150 };
}

pub struct VisualizerService {
    pub config: VisualizerConfig,
    pub enabled: Arc<AtomicBool>,
    pub anchor_position: Arc<Mutex<Option<[f32; 3]>>>,
    /// Yaw (°) de la cámara al confirmar el ancla; los ejes del rig son los de ese yaw.
    pub anchor_yaw: Arc<Mutex<f32>>,
    /// Ajuste fino sobre el ancla confirmado: (desplazamiento del gizmo en ejes del rig en m, giro en °).
    anchor_nudge: Arc<Mutex<([f32; 3], f32)>>,
    diag: Arc<Mutex<DiagInner>>,
    /// Último valor enviado de `iris_hide_cam` (el gizmo se oculta de las cámaras de VRChat).
    pub hide_cam: Arc<AtomicBool>,
    pub last_cam_pose: Arc<Mutex<Option<Pose>>>,
    pub last_wp_samples: Arc<Mutex<Option<Vec<[f32; 3]>>>>,
    pub is_syncing_path: Arc<AtomicBool>,
    pub last_error: Arc<Mutex<Option<String>>>,
    path_version: Arc<AtomicU64>,
    job_tx: Sender<PathSyncJob>,
    _worker_handle: Option<JoinHandle<()>>,
}

impl VisualizerService {
    pub fn new(config: VisualizerConfig) -> Self {
        let enabled = Arc::new(AtomicBool::new(false));
        let anchor_position = Arc::new(Mutex::new(None));
        let anchor_yaw = Arc::new(Mutex::new(0.0));
        let anchor_nudge = Arc::new(Mutex::new(([0.0; 3], 0.0)));
        let diag = Arc::new(Mutex::new(DiagInner::default()));
        let hide_cam = Arc::new(AtomicBool::new(false));
        let last_cam_pose = Arc::new(Mutex::new(None));
        let last_wp_samples = Arc::new(Mutex::new(None));
        let is_syncing_path = Arc::new(AtomicBool::new(false));
        let last_error = Arc::new(Mutex::new(None));
        let path_version = Arc::new(AtomicU64::new(0));

        let (job_tx, job_rx) = channel::<PathSyncJob>();

        let syncing_flag = is_syncing_path.clone();
        let path_ver_counter = path_version.clone();
        let err_sink = last_error.clone();

        let worker_handle = spawn(move || {
            Self::path_multiplexer_worker(job_rx, syncing_flag, path_ver_counter, err_sink);
        });

        VisualizerService {
            config,
            enabled,
            anchor_position,
            anchor_yaw,
            anchor_nudge,
            diag,
            hide_cam,
            last_cam_pose,
            last_wp_samples,
            is_syncing_path,
            last_error,
            path_version,
            job_tx,
            _worker_handle: Some(worker_handle),
        }
    }

    /// Hilo dedicado para el multiplexor del Path de 5 huesos.
    /// Garantiza secuencialidad estricta con cerrojo lógico: nunca se solapan dos envíos.
    fn path_multiplexer_worker(
        rx: Receiver<PathSyncJob>,
        syncing: Arc<AtomicBool>,
        current_version: Arc<AtomicU64>,
        err_sink: Arc<Mutex<Option<String>>>,
    ) {
        while let Ok(mut job) = rx.recv() {
            // Drenar trabajos acumulados para tomar siempre la versión más reciente solicitada
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }

            // Si hay una versión más nueva en cola, ignorar la obsoleta
            let job_ver = job.version;
            if job_ver < current_version.load(Ordering::Relaxed) {
                continue;
            }

            syncing.store(true, Ordering::SeqCst);

            if let Err(e) = Self::execute_path_mesh_multiplexing(job.samples, job.anchor, job.timing, &*job.sink) {
                *err_sink.lock().unwrap() = Some(format!("Error en multiplexación del Path: {}", e));
            } else {
                *err_sink.lock().unwrap() = None;
            }

            syncing.store(false, Ordering::SeqCst);
        }
    }

    /// Ejecuta el protocolo temporal exacto de multiplexación de los 5 huesos:
    ///
    /// 1. Enviar /avatar/parameters/iris_wp_index = 0 (int)
    /// 2. Esperar 25 ms
    /// 3. PARA i = 1 HASTA 5:
    ///     a. Normalizar W[i] (`samples` ya en ejes del ancla; `anchor` es el origen a restar, normalmente 0)
    ///     b. Enviar:
    ///          /avatar/parameters/iris_wp_x = float(nx)
    ///          /avatar/parameters/iris_wp_y = float(ny)
    ///          /avatar/parameters/iris_wp_z = float(nz)
    ///     c. Esperar `timing.settle_ms` (BlendTree posiciona WP_Target)
    ///     d. Enviar:
    ///          /avatar/parameters/iris_wp_index = int(i)
    ///     e. Esperar `timing.hold_ms` (Constraint de Waypoint_0i salta a WP_Target)
    ///     f. Enviar:
    ///          /avatar/parameters/iris_wp_index = 0 (int)
    ///     g. Esperar `timing.freeze_ms` (Waypoint_0i queda congelado en mundo con Weight=0)
    pub fn execute_path_mesh_multiplexing(
        samples: [[f32; 3]; 5],
        anchor: [f32; 3],
        timing: MuxTiming,
        sink: &(dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync),
    ) -> Result<(), String> {
        // Paso 1: Resetear índice a 0
        sink(ADDR_IRIS_WP_INDEX.into(), vec![OscType::Int(0)])?;

        // Paso 2: Esperar 25 ms
        std::thread::sleep(Duration::from_millis(25));

        // Paso 3: Iterar los 5 huesos
        for (idx, w_pos) in samples.iter().enumerate() {
            let bone_num = (idx + 1) as i32;
            let norm = normalize_position(*w_pos, anchor);

            // a & b: Enviar coordenadas normalizadas del cursor
            sink(ADDR_IRIS_WP_X.into(), vec![OscType::Float(norm[0])])?;
            sink(ADDR_IRIS_WP_Y.into(), vec![OscType::Float(norm[1])])?;
            sink(ADDR_IRIS_WP_Z.into(), vec![OscType::Float(norm[2])])?;

            // c: Esperar settle
            std::thread::sleep(Duration::from_millis(timing.settle_ms));

            // d: Enviar índice del hueso activo
            sink(ADDR_IRIS_WP_INDEX.into(), vec![OscType::Int(bone_num)])?;

            // e: Esperar hold
            std::thread::sleep(Duration::from_millis(timing.hold_ms));

            // f: Enviar índice 0 para congelar el hueso en su lugar
            sink(ADDR_IRIS_WP_INDEX.into(), vec![OscType::Int(0)])?;

            // g: Esperar freeze
            std::thread::sleep(Duration::from_millis(timing.freeze_ms));
        }

        Ok(())
    }

    /// Snapshot para la UI.
    pub fn snapshot(&self) -> VisualizerSnapshot {
        let enabled = self.enabled.load(Ordering::SeqCst);
        let anchor = self.anchor();
        let anchor_position = anchor.map(|a| a.eye);
        let last_cam_pose = *self.last_cam_pose.lock().unwrap();
        let last_wp_samples = self.last_wp_samples.lock().unwrap().clone();
        let is_syncing_path = self.is_syncing_path.load(Ordering::SeqCst);
        let last_error = self.last_error.lock().unwrap().clone();
        // Una sola toma del lock: dos `.lock()` en el mismo struct literal dejan vivo el primer guard (deadlock).
        let (nudge_pos, nudge_yaw) = *self.anchor_nudge.lock().unwrap();

        let (last_cam_normalized_pos, last_cam_normalized_rot) = match (last_cam_pose, anchor) {
            (Some(pose), Some(a)) => {
                let (p, r) = camera_params(&a, &pose);
                (Some(p), Some(r))
            }
            _ => (None, None),
        };

        VisualizerSnapshot {
            enabled,
            anchor_position,
            anchor_yaw: anchor.map(|a| a.yaw),
            hide_cam: self.hide_cam.load(Ordering::SeqCst),
            last_cam_pose,
            last_cam_normalized_pos,
            last_cam_normalized_rot,
            last_wp_samples,
            is_syncing_path,
            last_error,
            nudge_pos,
            nudge_yaw,
            diag: self.diag_snapshot(),
        }
    }

    /// FLUJO A: Calibración del Sistema "Ancla" (`SetAnchor`).
    ///
    /// 1. Si estaba activo, envía iris_enable = false y iris_wp_index = 0, esperando 50 ms.
    /// 2. Captura anchorPosition.
    /// 3. Envía iris_enable = true.
    /// 4. Espera 50 ms.
    pub fn set_anchor(
        &self,
        anchor: Anchor,
        sink: Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync>,
    ) -> Result<Anchor, String> {
        if !anchor.eye.iter().all(|x| x.is_finite()) || !anchor.yaw.is_finite() {
            return Err("Ancla no finita (NaN o Inf)".into());
        }

        // Si ya está activo NO se reconmuta `iris_enable`: eso soltaría y volvería a congelar `Iris_World_Root`, que
        // saltaría a otro sitio y el mesh dejaría de coincidir con el gizmo sobre el que se calibró.
        if !self.enabled.load(Ordering::SeqCst) {
            sink(ADDR_IRIS_ENABLE.into(), vec![OscType::Bool(true)])?;
            self.enabled.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
        }

        *self.anchor_position.lock().unwrap() = Some(anchor.eye);
        *self.anchor_yaw.lock().unwrap() = anchor.yaw;
        *self.anchor_nudge.lock().unwrap() = ([0.0; 3], 0.0);
        *self.last_cam_pose.lock().unwrap() = None;

        Ok(anchor)
    }

    /// FLUJO A (paso 1): muestra el gizmo en el origen de `Iris_World_Root` (cámara en 0,0,0) para que el usuario
    /// encaje encima la cámara de VRChat; el ancla se confirma después con `set_anchor`. Idempotente.
    pub fn activate(
        &self,
        sink: Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync>,
    ) -> Result<(), String> {
        if self.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }
        sink(ADDR_IRIS_ENABLE.into(), vec![OscType::Bool(true)])?;
        self.enabled.store(true, Ordering::SeqCst);
        for addr in [ADDR_IRIS_CAM_X, ADDR_IRIS_CAM_Y, ADDR_IRIS_CAM_Z, ADDR_IRIS_CAM_RX, ADDR_IRIS_CAM_RY, ADDR_IRIS_CAM_RZ] {
            sink(addr.into(), vec![OscType::Float(0.0)])?;
        }
        Ok(())
    }

    /// Oculta (o muestra) el gizmo de la cámara y el path en las cámaras de VRChat (foto/stream/dron), tuyas y ajenas.
    pub fn set_hide_cam(
        &self,
        hidden: bool,
        sink: &(dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync),
    ) -> Result<(), String> {
        sink(ADDR_IRIS_HIDE_CAM.into(), vec![OscType::Bool(hidden)])?;
        self.hide_cam.store(hidden, Ordering::SeqCst);
        Ok(())
    }

    /// Ancla confirmada (posición + yaw de la cámara al calibrar), sin ajuste fino.
    pub fn raw_anchor(&self) -> Option<Anchor> {
        let eye = (*self.anchor_position.lock().unwrap())?;
        Some(Anchor { eye, yaw: *self.anchor_yaw.lock().unwrap() })
    }

    /// Ancla EFECTIVA: la confirmada más el ajuste fino. Es la que usan la cámara, el path y el snapshot.
    /// Mover el gizmo `+d` en ejes del rig equivale a desplazar el origen `-d`, y girarlo `+dyaw` a girar el marco `-dyaw`.
    pub fn anchor(&self) -> Option<Anchor> {
        let raw = self.raw_anchor()?;
        let (d, dyaw) = *self.anchor_nudge.lock().unwrap();
        if d == [0.0; 3] && dyaw == 0.0 {
            return Some(raw);
        }
        let eye = raw.local_to_world(&Pose { pos: [-d[0], -d[1], -d[2]], rot: [0.0; 3] }, 1.0).pos;
        Some(Anchor { eye, yaw: raw.yaw - dyaw })
    }

    /// Suma un ajuste fino (gizmo +`d_pos` m en ejes del rig, +`d_yaw` °), limitado a ±5 m / ±180°.
    /// Hay que llamar a `resend_last` para que el rig lo refleje sin esperar un nuevo eco de pose.
    pub fn nudge(&self, d_pos: [f32; 3], d_yaw: f32) {
        let mut n = self.anchor_nudge.lock().unwrap();
        for (cur, d) in n.0.iter_mut().zip(d_pos) {
            if d.is_finite() {
                *cur = (*cur + d).clamp(-5.0, 5.0);
            }
        }
        if d_yaw.is_finite() {
            n.1 = (n.1 + d_yaw).clamp(-180.0, 180.0);
        }
    }

    pub fn reset_nudge(&self) {
        *self.anchor_nudge.lock().unwrap() = ([0.0; 3], 0.0);
    }

    /// Reenvía la última pose (saltándose el deadband), p. ej. tras un ajuste fino.
    pub fn resend_last(
        &self,
        frame_sink: &(dyn Fn(Vec<OscMessage>) -> Result<(), String> + Send + Sync),
    ) -> Result<SyncOutcome, String> {
        let last = *self.last_cam_pose.lock().unwrap();
        match last {
            Some(pose) => self.sync_inner(&pose, frame_sink, true),
            None => Ok(SyncOutcome::Deadband), // nada que reenviar todavía
        }
    }

    // ---- Diagnóstico -----------------------------------------------------------------------------------------

    pub fn note_pose_echo(&self) {
        self.diag.lock().unwrap().pose_echoes += 1;
    }

    pub fn note_skip(&self, reason: SkipReason) {
        let mut d = self.diag.lock().unwrap();
        match reason {
            SkipReason::Disabled => d.skipped.disabled += 1,
            SkipReason::NoAnchor => d.skipped.no_anchor += 1,
            SkipReason::ExternalControl => d.skipped.external_control += 1,
            SkipReason::Deadband => d.skipped.deadband += 1,
            SkipReason::Busy => d.skipped.busy += 1,
        }
    }

    /// Registra un `/avatar/parameters/iris_*` que VRChat devolvió por 9001. Cualquier otra dirección se ignora.
    pub fn note_avatar_echo(&self, addr: &str, value: f32) {
        if let Some(name) = addr.strip_prefix("/avatar/parameters/iris_") {
            self.diag.lock().unwrap().avatar_echo.insert(format!("iris_{name}"), (value, Instant::now()));
        }
    }

    pub fn diag_snapshot(&self) -> VizDiagSnapshot {
        let d = self.diag.lock().unwrap();
        let mut avatar_echo: Vec<AvatarEchoEntry> = d
            .avatar_echo
            .iter()
            .map(|(name, (value, at))| AvatarEchoEntry { name: name.clone(), value: *value, age_ms: at.elapsed().as_millis() as u64 })
            .collect();
        avatar_echo.sort_by(|a, b| a.name.cmp(&b.name));
        VizDiagSnapshot {
            pose_echoes: d.pose_echoes,
            frames_sent: d.frames_sent,
            skipped: d.skipped.clone(),
            last_error: d.last_error.clone(),
            avatar_echo,
        }
    }

    /// FLUJO A: Reseteo del Sistema "Ancla" (`ResetAnchor`).
    pub fn reset_anchor(
        &self,
        sink: &(dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync),
    ) -> Result<(), String> {
        let _ = sink(ADDR_IRIS_ENABLE.into(), vec![OscType::Bool(false)]);
        let _ = sink(ADDR_IRIS_WP_INDEX.into(), vec![OscType::Int(0)]);
        self.enabled.store(false, Ordering::SeqCst);
        *self.anchor_position.lock().unwrap() = None;
        *self.anchor_yaw.lock().unwrap() = 0.0;
        *self.anchor_nudge.lock().unwrap() = ([0.0; 3], 0.0);
        *self.last_cam_pose.lock().unwrap() = None;
        Ok(())
    }

    /// Muestrea la trayectoria matemática de Velza Iris en 5 puntos equidistantes en parámetro normalizado `t`:
    /// `t_samples = [0.0, 0.25, 0.50, 0.75, 1.0]`
    pub fn sample_5_waypoints(
        project: &SequenceProject,
        active_anchor: Option<&Anchor>,
        fallback_anchor_pos: [f32; 3],
    ) -> [[f32; 3]; 5] {
        if project.keyframes.is_empty() {
            // Fallback sintético en caso de secuencia sin keyframes: 5 puntos en línea recta frente al ancla
            let mut def = [[0.0; 3]; 5];
            for (i, p) in def.iter_mut().enumerate() {
                let d = (i + 1) as f32 * 0.5;
                *p = [fallback_anchor_pos[0], fallback_anchor_pos[1], fallback_anchor_pos[2] + d];
            }
            return def;
        }

        let duration = project.duration.max(0.1);
        let t_fractions = [0.0_f32, 0.25_f32, 0.50_f32, 0.75_f32, 1.0_f32];
        let mut samples = [[0.0; 3]; 5];

        for (i, u) in t_fractions.iter().enumerate() {
            let t = (*u * duration).clamp(0.0, duration);
            let pose = sample_base_pose_at(project, active_anchor, t);
            samples[i] = pose.pos;
        }

        samples
    }

    /// FLUJO B: Encola la sincronización asíncrona no bloqueante del Path de 5 huesos (`SyncPathMeshAsync`).
    pub fn queue_sync_path_mesh(
        &self,
        project: &SequenceProject,
        active_anchor: Option<&Anchor>,
        sink: Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync>,
    ) -> Result<(), String> {
        let anchor = self
            .anchor()
            .ok_or("El Ancla no está calibrada; calibra primero antes de sincronizar el Path")?;

        let samples = Self::sample_5_waypoints(project, active_anchor, anchor.eye);

        // Guardar muestra (en mundo) para telemetría
        *self.last_wp_samples.lock().unwrap() = Some(samples.to_vec());

        // Los huesos viven bajo Iris_World_Root, cuyos ejes son los del yaw del ancla: se manda la posición en ese espacio.
        let samples = samples.map(|p| anchor.world_to_local(&Pose { pos: p, rot: [0.0; 3] }).pos);

        let next_ver = self.path_version.fetch_add(1, Ordering::SeqCst) + 1;
        let job = PathSyncJob {
            version: next_ver,
            samples,
            anchor: [0.0; 3],
            timing: self.config.mux_timing,
            sink,
        };

        let _ = self.job_tx.send(job);
        Ok(())
    }

    /// FLUJO C: Actualización en Tiempo Real del Visualizador de Cámara (`SyncCameraVisualizerFrame`).
    /// Genera y emite los 6 parámetros de la cámara en ráfaga inmediata.
    pub fn sync_camera_frame(
        &self,
        pose: &Pose,
        frame_sink: &(dyn Fn(Vec<OscMessage>) -> Result<(), String> + Send + Sync),
    ) -> Result<SyncOutcome, String> {
        self.sync_inner(pose, frame_sink, false)
    }

    /// `force`: no aplicar el deadband (reenvío tras un ajuste fino).
    fn sync_inner(
        &self,
        pose: &Pose,
        frame_sink: &(dyn Fn(Vec<OscMessage>) -> Result<(), String> + Send + Sync),
        force: bool,
    ) -> Result<SyncOutcome, String> {
        if !self.enabled.load(Ordering::SeqCst) {
            self.note_skip(SkipReason::Disabled);
            return Ok(SyncOutcome::Disabled);
        }
        // Sin ancla confirmada el rig sigue mostrando el gizmo en su origen: no hay referencia contra la que expresar la pose.
        let Some(anchor) = self.anchor() else {
            self.note_skip(SkipReason::NoAnchor);
            return Ok(SyncOutcome::NoAnchor);
        };

        // Si la pose no ha cambiado de forma medible, omitir reenvío redundante
        let prev = if force { None } else { *self.last_cam_pose.lock().unwrap() };
        if let Some(lp) = prev {
            let dp = (pose.pos[0] - lp.pos[0])
                .abs()
                .max((pose.pos[1] - lp.pos[1]).abs())
                .max((pose.pos[2] - lp.pos[2]).abs());
            let dr = (pose.rot[0] - lp.rot[0])
                .abs()
                .max((pose.rot[1] - lp.rot[1]).abs())
                .max((pose.rot[2] - lp.rot[2]).abs());
            if dp < 1e-4 && dr < 1e-3 {
                self.note_skip(SkipReason::Deadband);
                return Ok(SyncOutcome::Deadband);
            }
        }

        *self.last_cam_pose.lock().unwrap() = Some(*pose);

        let (norm_pos, norm_rot) = camera_params(&anchor, pose);

        let messages = vec![
            OscMessage {
                addr: ADDR_IRIS_CAM_X.into(),
                args: vec![OscType::Float(norm_pos[0])],
            },
            OscMessage {
                addr: ADDR_IRIS_CAM_Y.into(),
                args: vec![OscType::Float(norm_pos[1])],
            },
            OscMessage {
                addr: ADDR_IRIS_CAM_Z.into(),
                args: vec![OscType::Float(norm_pos[2])],
            },
            OscMessage {
                addr: ADDR_IRIS_CAM_RX.into(),
                args: vec![OscType::Float(norm_rot[0])],
            },
            OscMessage {
                addr: ADDR_IRIS_CAM_RY.into(),
                args: vec![OscType::Float(norm_rot[1])],
            },
            OscMessage {
                addr: ADDR_IRIS_CAM_RZ.into(),
                args: vec![OscType::Float(norm_rot[2])],
            },
        ];

        match frame_sink(messages) {
            Ok(()) => {
                let mut d = self.diag.lock().unwrap();
                d.frames_sent += 1;
                d.last_error = None;
                Ok(SyncOutcome::Sent)
            }
            Err(e) => {
                // La pose no llegó: olvidarla para que el siguiente intento no caiga en el deadband.
                *self.last_cam_pose.lock().unwrap() = None;
                self.note_skip(SkipReason::Busy);
                self.diag.lock().unwrap().last_error = Some(e.clone());
                Err(e)
            }
        }
    }
}
