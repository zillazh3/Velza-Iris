//! Servicio de integración cinemática y sincronización del Visualizador VRChat (Avatars 3.0 / Unity).
//!
//! Dos canales (ver `protocol.rs`):
//! - **Dueño**: `iris_cam_*` a 60 Hz y los puntos del path en parámetros locales (float32 exacto, 0 bits de sync).
//! - **Remotos**: bus time-sliced (`iris_net_slot` + `iris_net_x/y/z`), progreso `iris_path_t` y pose retenida.
//!   Sin canal de retorno no hay ack: los datos se repiten en bucle abierto (el front reenvía el path cada 20 s).

use super::config::VisualizerConfig;
use super::net::{self, hold_slots, NetPath, SlotWrite};
use super::protocol::*;
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use crate::studio::model::SequenceProject;
use rosc::{OscMessage, OscType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{spawn, JoinHandle};
use std::time::{Duration, Instant};

type NetSink = Arc<dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync>;

/// `iris_path_t` hacia los remotos: como mucho a ~10 Hz (más rápido no llega, la red sincroniza a 5-10 Hz).
const T_MIN_GAP: Duration = Duration::from_millis(100);
/// El suavizado del cliente retrasa el gizmo remoto: se envía `T + LEAD · dT/dt` para compensarlo.
const T_LEAD_S: f32 = 0.12;
/// Quieta durante este tiempo, la pose de la cámara a mano se manda a los remotos como pose retenida.
const HAND_QUIET: Duration = Duration::from_millis(300);
/// Un recorrido se considera en curso mientras haya frames de reproducción más recientes que esto.
const PLAYBACK_GRACE: Duration = Duration::from_millis(1000);
/// Si la cámara se aleja más de esto del centro de la región, se recentra (el rango del dueño es ±50 m).
const RECENTER_METERS: f32 = 40.0;
const RECENTER_COOLDOWN: Duration = Duration::from_secs(5);

/// Instantánea del estado del visualizador para la UI y diagnóstico.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizerSnapshot {
    pub enabled: bool,
    /// Centro de la región de trabajo (mundo, m); el gizmo y el path se expresan relativos a él.
    pub anchor_position: Option<[f32; 3]>,
    pub anchor_yaw: Option<f32>,
    pub hide_cam: bool,
    pub last_cam_pose: Option<Pose>,
    pub last_cam_normalized_pos: Option<[f32; 3]>,
    pub last_cam_normalized_rot: Option<[f32; 3]>,
    /// Los 8 puntos de control del path (mundo).
    pub last_wp_samples: Option<Vec<[f32; 3]>>,
    pub is_syncing_path: bool,
    pub last_error: Option<String>,
    /// Ajuste fino acumulado del ancla (desplazamiento del gizmo en ejes del rig, m; y giro, °).
    pub nudge_pos: [f32; 3],
    pub nudge_yaw: f32,
    pub diag: VizDiagSnapshot,
    /// Qué ven los remotos: `"path"` (siguen `iris_path_t`) o `"hand"` (pose retenida).
    pub net_mode: String,
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

/// Esperas del bus hacia los remotos. Con parámetros synced los demás jugadores reciben los cambios a unos ~5-10 Hz:
/// los datos deben llevar estables más de un tick antes de cambiar el selector, y el selector debe durar más de un tick.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MuxTiming {
    /// Tras escribir `iris_net_x/y/z` y antes de cambiar `iris_net_slot` (el remoto ya ve los datos estables).
    pub data_ms: u64,
    /// Con `iris_net_slot = k` (el remoto entra al estado de latch de ese slot).
    pub slot_ms: u64,
}

impl MuxTiming {
    /// Para pruebas locales (sin red real).
    pub const LOCAL: MuxTiming = MuxTiming { data_ms: 5, slot_ms: 5 };
    /// Visible para otros jugadores. ponytail: valores estimados; calibrar con un segundo cliente (~0,5 s por slot).
    pub const REMOTE: MuxTiming = MuxTiming { data_ms: 250, slot_ms: 250 };
}

/// Lo que ven los remotos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetMode {
    /// Siguen `iris_path_t` sobre los puntos de control.
    Path,
    /// Muestran la pose retenida (cámara a mano o scrub).
    Hand,
}

enum NetJob {
    /// Región + puntos de control del proyecto.
    Path(Vec<SlotWrite>),
    /// Pose retenida (+ modo `Hand`).
    Hold(Vec<SlotWrite>),
    /// Cambio de modo a `Path`.
    Mode(Vec<SlotWrite>),
}

impl NetJob {
    fn rank(&self) -> usize {
        match self {
            NetJob::Path(_) => 0,
            NetJob::Hold(_) => 1,
            NetJob::Mode(_) => 2,
        }
    }
}

struct QueuedJob {
    job: NetJob,
    timing: MuxTiming,
    sink: NetSink,
}

#[derive(Default)]
struct TState {
    last_k: Option<i32>,
    last_at: Option<Instant>,
    pending: Option<f32>,
    prev: Option<(f32, Instant)>,
}

struct HandState {
    pose: Option<Pose>,
    changed_at: Instant,
    sent: Option<Pose>,
}

/// Estado compartido con el hilo de mantenimiento (`net_tick_loop`).
struct NetShared {
    sink: Mutex<Option<NetSink>>,
    path: Mutex<Option<NetPath>>,
    mode: Mutex<NetMode>,
    t: Mutex<TState>,
    hand: Mutex<HandState>,
    last_playback: Mutex<Option<Instant>>,
    last_recenter: Mutex<Option<Instant>>,
    enabled: Arc<AtomicBool>,
    anchor_position: Arc<Mutex<Option<[f32; 3]>>>,
    timing: MuxTiming,
    jobs: Sender<QueuedJob>,
    shutdown: AtomicBool,
}

impl NetShared {
    fn region(&self) -> [f32; 3] {
        self.anchor_position.lock().unwrap().unwrap_or([0.0; 3])
    }

    fn queue(&self, job: NetJob) {
        let Some(sink) = self.sink.lock().unwrap().clone() else { return };
        let _ = self.jobs.send(QueuedJob { job, timing: self.timing, sink });
    }

    fn playing(&self) -> bool {
        self.last_playback.lock().unwrap().is_some_and(|a| a.elapsed() < PLAYBACK_GRACE)
    }

    /// Envía `iris_path_t` limitado a ~10 Hz y solo si cambió el escalón de red; el último valor pendiente lo vacía el tick.
    fn send_t(&self, t: f32) {
        let Some(sink) = self.sink.lock().unwrap().clone() else { return };
        let v = 2.0 * t.clamp(0.0, 1.0) - 1.0;
        let k = (v * NET_QUANT_STEPS).round() as i32;
        {
            let mut st = self.t.lock().unwrap();
            if st.last_k == Some(k) {
                st.pending = None;
                return;
            }
            if st.last_at.is_some_and(|a| a.elapsed() < T_MIN_GAP) {
                st.pending = Some(t);
                return;
            }
            st.last_k = Some(k);
            st.last_at = Some(Instant::now());
            st.pending = None;
        }
        let _ = sink(ADDR_IRIS_PATH_T.into(), vec![OscType::Float(net_bin(v))]);
    }

    /// `T` con adelanto para compensar el suavizado exponencial del cliente.
    fn lead(&self, t: f32) -> f32 {
        let now = Instant::now();
        let mut st = self.t.lock().unwrap();
        let out = match st.prev {
            Some((pt, at)) => {
                let dt = now.duration_since(at).as_secs_f32();
                if dt > 0.004 && dt < 0.5 {
                    let dtdt = ((t - pt) / dt).clamp(-2.0, 2.0);
                    (t + (T_LEAD_S * dtdt).clamp(-0.05, 0.05)).clamp(0.0, 1.0)
                } else {
                    t
                }
            }
            None => t,
        };
        st.prev = Some((t, now));
        out
    }

    fn set_mode_path(&self) {
        let mut m = self.mode.lock().unwrap();
        if *m != NetMode::Path {
            *m = NetMode::Path;
            drop(m);
            self.queue(NetJob::Mode(vec![SlotWrite { slot: slot::MODE_PATH, data: [0.0; 3] }]));
        }
    }

    fn note_hand(&self, pose: &Pose) {
        let mut h = self.hand.lock().unwrap();
        let moved = h.pose.map_or(true, |p| pose_differs(&p, pose));
        if moved {
            h.pose = Some(*pose);
            h.changed_at = Instant::now();
        }
    }

    /// Un paso del hilo de mantenimiento: vacía el `T` pendiente y manda la pose retenida cuando la cámara se quedó quieta.
    fn tick(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let pending = self.t.lock().unwrap().pending;
        if let Some(t) = pending {
            let due = self.t.lock().unwrap().last_at.map_or(true, |a| a.elapsed() >= T_MIN_GAP);
            if due {
                self.send_t(t);
            }
        }
        if self.playing() {
            return;
        }
        let hold = {
            let mut h = self.hand.lock().unwrap();
            match h.pose {
                Some(p) if h.changed_at.elapsed() >= HAND_QUIET && h.sent != Some(p) => {
                    h.sent = Some(p);
                    Some(p)
                }
                _ => None,
            }
        };
        if let Some(pose) = hold {
            let mut slots = hold_slots(self.region(), &pose);
            slots.push(SlotWrite { slot: slot::MODE_HAND, data: [0.0; 3] });
            *self.mode.lock().unwrap() = NetMode::Hand;
            self.queue(NetJob::Hold(slots));
        }
    }
}

fn pose_differs(a: &Pose, b: &Pose) -> bool {
    (0..3).any(|i| (a.pos[i] - b.pos[i]).abs() > 0.02) || (0..3).any(|i| (a.rot[i] - b.rot[i]).abs() > 0.5)
}

pub struct VisualizerService {
    pub config: VisualizerConfig,
    pub enabled: Arc<AtomicBool>,
    /// Centro de la región de trabajo (mundo, m). El rig vive en el origen del mundo y `Iris_Region` se desplaza a este centro.
    pub anchor_position: Arc<Mutex<Option<[f32; 3]>>>,
    /// Yaw (°) del ancla; solo distinto de 0 en rigs con origen propio (el de producción no gira).
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
    net: Arc<NetShared>,
    _worker_handle: Option<JoinHandle<()>>,
    _tick_handle: Option<JoinHandle<()>>,
}

impl Drop for VisualizerService {
    fn drop(&mut self) {
        self.net.shutdown.store(true, Ordering::SeqCst);
    }
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

        let (job_tx, job_rx) = channel::<QueuedJob>();

        let net = Arc::new(NetShared {
            sink: Mutex::new(None),
            path: Mutex::new(None),
            mode: Mutex::new(NetMode::Hand),
            t: Mutex::new(TState::default()),
            hand: Mutex::new(HandState { pose: None, changed_at: Instant::now(), sent: None }),
            last_playback: Mutex::new(None),
            last_recenter: Mutex::new(None),
            enabled: enabled.clone(),
            anchor_position: anchor_position.clone(),
            timing: config.mux_timing,
            jobs: job_tx,
            shutdown: AtomicBool::new(false),
        });

        let syncing_flag = is_syncing_path.clone();
        let err_sink = last_error.clone();
        let worker_handle = spawn(move || Self::net_worker(job_rx, syncing_flag, err_sink));

        let tick_net = net.clone();
        let tick_handle = spawn(move || {
            while !tick_net.shutdown.load(Ordering::Relaxed) {
                tick_net.tick();
                std::thread::sleep(Duration::from_millis(50));
            }
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
            net,
            _worker_handle: Some(worker_handle),
            _tick_handle: Some(tick_handle),
        }
    }

    /// Hilo dedicado al bus hacia los remotos. Garantiza secuencialidad estricta (nunca se solapan dos envíos) y, de lo
    /// pendiente, ejecuta solo el trabajo más reciente de cada tipo (región+path, pose retenida, modo).
    /// Un ciclo de path dura ~12 s: entre slot y slot atiende lo urgente (pose retenida y modo van al instante) y, si llega un
    /// path más nuevo, aborta el viejo y empieza el nuevo.
    fn net_worker(rx: Receiver<QueuedJob>, syncing: Arc<AtomicBool>, err_sink: Arc<Mutex<Option<String>>>) {
        let mut carry: Vec<QueuedJob> = Vec::new();
        loop {
            if carry.is_empty() {
                match rx.recv() {
                    Ok(j) => carry.push(j),
                    Err(_) => return,
                }
            }
            while let Ok(j) = rx.try_recv() {
                carry.push(j);
            }
            let mut latest: [Option<QueuedJob>; 3] = [None, None, None];
            for q in carry.drain(..) {
                let r = q.job.rank();
                latest[r] = Some(q);
            }
            syncing.store(true, Ordering::SeqCst);
            for q in latest.into_iter().flatten() {
                let slots = match &q.job {
                    NetJob::Path(s) | NetJob::Hold(s) | NetJob::Mode(s) => s,
                };
                let is_path = matches!(q.job, NetJob::Path(_));
                let result = Self::execute_slots_with(slots, q.timing, &*q.sink, &mut || {
                    if !is_path {
                        return false;
                    }
                    let mut abort = false;
                    while let Ok(j) = rx.try_recv() {
                        if matches!(j.job, NetJob::Path(_)) {
                            carry.push(j); // un path más nuevo reemplaza al que se está enviando
                            abort = true;
                        } else {
                            let s = match &j.job {
                                NetJob::Path(s) | NetJob::Hold(s) | NetJob::Mode(s) => s,
                            };
                            let _ = Self::execute_slots(s, j.timing, &*j.sink);
                        }
                    }
                    abort
                });
                match result {
                    Ok(()) => *err_sink.lock().unwrap() = None,
                    Err(e) => *err_sink.lock().unwrap() = Some(format!("Error en el bus de red: {e}")),
                }
            }
            syncing.store(false, Ordering::SeqCst);
        }
    }

    /// Protocolo del bus hacia los remotos, por slot:
    /// 1. escribir `iris_net_x/y/z`; 2. esperar `data_ms` (el remoto ya ve los datos estables);
    /// 3. escribir `iris_net_slot = k` (el remoto latchea); 4. esperar `slot_ms`.
    /// Al final `iris_net_slot = 0` para que un siguiente envío con el mismo primer slot vuelva a disparar el latch.
    pub fn execute_slots(
        slots: &[SlotWrite],
        timing: MuxTiming,
        sink: &(dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync),
    ) -> Result<(), String> {
        Self::execute_slots_with(slots, timing, sink, &mut || false)
    }

    /// Igual que `execute_slots`; `between` se llama tras cada slot y puede abortar el envío devolviendo `true`.
    fn execute_slots_with(
        slots: &[SlotWrite],
        timing: MuxTiming,
        sink: &(dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync),
        between: &mut dyn FnMut() -> bool,
    ) -> Result<(), String> {
        for s in slots {
            for (addr, v) in [ADDR_IRIS_NET_X, ADDR_IRIS_NET_Y, ADDR_IRIS_NET_Z].iter().zip(s.data) {
                sink((*addr).into(), vec![OscType::Float(v)])?;
            }
            std::thread::sleep(Duration::from_millis(timing.data_ms));
            sink(ADDR_IRIS_NET_SLOT.into(), vec![OscType::Int(s.slot)])?;
            std::thread::sleep(Duration::from_millis(timing.slot_ms));
            if between() {
                break;
            }
        }
        sink(ADDR_IRIS_NET_SLOT.into(), vec![OscType::Int(0)])
    }

    /// Escribe parámetros locales del dueño en tandas con una pausa corta (una ráfaga de ~50 mensajes seguidos podría
    /// perderse en el socket de VRChat).
    fn send_owner_writes(sink: &NetSink, writes: &[(String, f32)]) -> Result<(), String> {
        for (n, (addr, v)) in writes.iter().enumerate() {
            sink(addr.clone(), vec![OscType::Float(*v)])?;
            if n % 12 == 11 {
                std::thread::sleep(Duration::from_millis(3));
            }
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
        let net_mode = match *self.net.mode.lock().unwrap() {
            NetMode::Path => "path",
            NetMode::Hand => "hand",
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
            net_mode: net_mode.to_string(),
        }
    }

    /// Ancla arbitraria (solo pruebas y rigs con origen propio; el rig de producción usa el centro de región, ver `activate`).
    ///
    /// 1. Si estaba activo, envía iris_enable = false y iris_wp_index = 0, esperando 50 ms.
    /// 2. Captura anchorPosition.
    /// 3. Envía iris_enable = true.
    /// 4. Espera 50 ms.
    pub fn set_anchor(&self, anchor: Anchor, sink: NetSink) -> Result<Anchor, String> {
        if !anchor.eye.iter().all(|x| x.is_finite()) || !anchor.yaw.is_finite() {
            return Err("Ancla no finita (NaN o Inf)".into());
        }

        // Se REenvía `true`: si ya estaba encendido en VRChat es un no-op, y si el avatar se recargó (arranca en false) lo repara.
        let was_enabled = self.enabled.load(Ordering::SeqCst);
        sink(ADDR_IRIS_ENABLE.into(), vec![OscType::Bool(true)])?;
        self.enabled.store(true, Ordering::SeqCst);
        *self.net.sink.lock().unwrap() = Some(sink);
        if !was_enabled {
            std::thread::sleep(Duration::from_millis(50));
        }

        *self.anchor_position.lock().unwrap() = Some(anchor.eye);
        *self.anchor_yaw.lock().unwrap() = anchor.yaw;
        *self.anchor_nudge.lock().unwrap() = ([0.0; 3], 0.0);
        *self.last_cam_pose.lock().unwrap() = None;

        Ok(anchor)
    }

    /// Enciende el rig. `Iris_World_Root` vive en el origen del mundo y `Iris_Region` se desplaza al centro de la región:
    /// la pose de mundo de la cámara de VRChat es directamente la del gizmo, sin calibrar y con la misma referencia para
    /// todos los clientes. El ajuste fino (`nudge`) se conserva.
    /// Es una acción explícita: SIEMPRE reenvía (si el avatar se recargó, `iris_enable` volvió a false en VRChat
    /// aunque la app siga creyendo que el rig está encendido), incluidos el path y el modo de los remotos.
    pub fn activate(&self, sink: NetSink) -> Result<(), String> {
        sink(ADDR_IRIS_ENABLE.into(), vec![OscType::Bool(true)])?;
        self.enabled.store(true, Ordering::SeqCst);
        *self.net.sink.lock().unwrap() = Some(sink.clone());
        let region = {
            let mut a = self.anchor_position.lock().unwrap();
            *a.get_or_insert([0.0; 3])
        };
        *self.anchor_yaw.lock().unwrap() = 0.0;
        // Sin esto el siguiente eco de pose caería en el deadband y el gizmo se quedaría en 0 hasta que la cámara se mueva.
        *self.last_cam_pose.lock().unwrap() = None;
        let mut writes: Vec<(String, f32)> = [ADDR_IRIS_CAM_X, ADDR_IRIS_CAM_Y, ADDR_IRIS_CAM_Z, ADDR_IRIS_CAM_RX, ADDR_IRIS_CAM_RY, ADDR_IRIS_CAM_RZ]
            .iter()
            .map(|a| (a.to_string(), 0.0))
            .collect();
        let path = self.net.path.lock().unwrap().clone();
        match &path {
            Some(p) => writes.extend(p.owner_writes()),
            None => writes.extend(net::region_writes(region)),
        }
        Self::send_owner_writes(&sink, &writes)?;
        // Los remotos pierden todo al recargar el avatar: reenviar región + path y volver a mostrar la pose.
        self.net.hand.lock().unwrap().sent = None;
        match path {
            Some(p) => self.net.queue(NetJob::Path(p.slots())),
            None => self.net.queue(NetJob::Path(vec![SlotWrite { slot: slot::REGION, data: net::region_bus(region) }])),
        }
        if *self.net.mode.lock().unwrap() == NetMode::Path {
            self.net.queue(NetJob::Mode(vec![SlotWrite { slot: slot::MODE_PATH, data: [0.0; 3] }]));
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

    /// Ancla confirmada (centro de región + yaw), sin ajuste fino.
    pub fn raw_anchor(&self) -> Option<Anchor> {
        let eye = (*self.anchor_position.lock().unwrap())?;
        Some(Anchor { eye, yaw: *self.anchor_yaw.lock().unwrap() })
    }

    /// Ancla EFECTIVA: la confirmada más el ajuste fino. Es la que usan la cámara en vivo y el snapshot.
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

    /// Apaga el rig (`iris_enable = false`); el path de la app se conserva para volver a mostrarlo.
    pub fn reset_anchor(
        &self,
        sink: &(dyn Fn(String, Vec<OscType>) -> Result<(), String> + Send + Sync),
    ) -> Result<(), String> {
        let _ = sink(ADDR_IRIS_ENABLE.into(), vec![OscType::Bool(false)]);
        self.enabled.store(false, Ordering::SeqCst);
        *self.anchor_position.lock().unwrap() = None;
        *self.anchor_yaw.lock().unwrap() = 0.0;
        *self.anchor_nudge.lock().unwrap() = ([0.0; 3], 0.0);
        *self.last_cam_pose.lock().unwrap() = None;
        Ok(())
    }

    /// VRChat avisa con `/avatar/change` al cargar un avatar: los `iris_*` del avatar nuevo arrancan en sus defaults
    /// (rig apagado, sin región), así que el estado de la app debe volver a "sin mostrar" para no creerlo encendido.
    pub fn on_avatar_changed(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.hide_cam.store(false, Ordering::SeqCst);
        *self.anchor_position.lock().unwrap() = None;
        *self.anchor_yaw.lock().unwrap() = 0.0;
        *self.anchor_nudge.lock().unwrap() = ([0.0; 3], 0.0);
        *self.last_cam_pose.lock().unwrap() = None;
        self.diag.lock().unwrap().avatar_echo.clear();
        *self.net.t.lock().unwrap() = TState::default();
        self.net.hand.lock().unwrap().sent = None;
    }

    /// Elige los 8 puntos de control del path a partir de los keyframes del proyecto (ver `net::sample_control_points`).
    pub fn sample_control_points(
        project: &SequenceProject,
        active_anchor: Option<&Anchor>,
        fallback_pos: [f32; 3],
    ) -> [(f32, Pose); NET_POINTS] {
        net::sample_control_points(project, active_anchor, fallback_pos)
    }

    /// Encola la sincronización del path: el dueño lo ve al instante (parámetros locales) y los remotos lo reciben por el
    /// bus (`NetJob::Path`, ~0,5 s por slot). El centro de la región se elige alrededor del path.
    pub fn queue_sync_path_mesh(
        &self,
        project: &SequenceProject,
        active_anchor: Option<&Anchor>,
        sink: NetSink,
    ) -> Result<(), String> {
        let current = self
            .raw_anchor()
            .ok_or("El rig está apagado; pulsa «Mostrar gizmo» antes de sincronizar el Path")?;
        let samples = net::sample_control_points(project, active_anchor, current.eye);
        let path = NetPath::new(samples);

        *self.last_wp_samples.lock().unwrap() = Some(path.points.iter().map(|p| p.pos).collect());
        *self.anchor_position.lock().unwrap() = Some(path.region);
        *self.last_cam_pose.lock().unwrap() = None; // la pose en vivo ahora es relativa a otro centro
        *self.net.sink.lock().unwrap() = Some(sink.clone());
        *self.net.path.lock().unwrap() = Some(path.clone());
        self.net.hand.lock().unwrap().sent = None;

        Self::send_owner_writes(&sink, &path.owner_writes())?;
        self.net.queue(NetJob::Path(path.slots()));
        Ok(())
    }

    /// Recentra la región cuando la cámara se aleja de ella (el dueño solo cubre ±50 m alrededor del centro).
    fn maybe_recenter(&self, pose: &Pose) {
        let Some(c) = *self.anchor_position.lock().unwrap() else { return };
        let far = (0..3).any(|a| (pose.pos[a] - c[a]).abs() > RECENTER_METERS);
        if !far || self.net.last_recenter.lock().unwrap().is_some_and(|t| t.elapsed() < RECENTER_COOLDOWN) {
            return;
        }
        let Some(sink) = self.net.sink.lock().unwrap().clone() else { return };
        let region = snap_region(pose.pos);
        if region == c {
            return;
        }
        *self.net.last_recenter.lock().unwrap() = Some(Instant::now());
        *self.anchor_position.lock().unwrap() = Some(region);
        *self.last_cam_pose.lock().unwrap() = None;
        self.net.hand.lock().unwrap().sent = None;
        let slots = {
            let mut p = self.net.path.lock().unwrap();
            match p.as_mut() {
                Some(path) => {
                    path.region = region;
                    let _ = Self::send_owner_writes(&sink, &path.owner_writes());
                    path.slots()
                }
                None => {
                    let _ = Self::send_owner_writes(&sink, &net::region_writes(region));
                    vec![SlotWrite { slot: slot::REGION, data: net::region_bus(region) }]
                }
            }
        };
        self.net.queue(NetJob::Path(slots));
    }

    /// FLUJO C: Actualización en Tiempo Real del Visualizador de Cámara (`SyncCameraVisualizerFrame`).
    /// Manda al dueño los 6 parámetros de la pose (local, exacta) y, para los remotos, la pose retenida cuando la cámara
    /// se queda quieta (mano o scrub).
    pub fn sync_camera_frame(
        &self,
        pose: &Pose,
        frame_sink: &(dyn Fn(Vec<OscMessage>) -> Result<(), String> + Send + Sync),
    ) -> Result<SyncOutcome, String> {
        self.net.note_hand(pose);
        self.sync_inner(pose, frame_sink, false)
    }

    /// Igual que `sync_camera_frame` durante la reproducción de un recorrido: además manda a los remotos el progreso
    /// `iris_path_t` (≤ 10 Hz) calculado con la pose base `path_pos` y el instante `time` del proyecto.
    pub fn sync_playback(
        &self,
        pose: &Pose,
        time: f32,
        path_pos: [f32; 3],
        frame_sink: &(dyn Fn(Vec<OscMessage>) -> Result<(), String> + Send + Sync),
    ) -> Result<SyncOutcome, String> {
        if self.enabled.load(Ordering::Relaxed) {
            *self.net.last_playback.lock().unwrap() = Some(Instant::now());
            let t = self.net.path.lock().unwrap().as_ref().map(|p| p.path_t(time, path_pos));
            match t {
                Some(t) => {
                    self.net.set_mode_path();
                    let t = self.net.lead(t);
                    self.net.send_t(t);
                }
                None => self.net.note_hand(pose),
            }
        }
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
        self.maybe_recenter(pose);
        // Sin ancla el rig sigue apagado: no hay referencia contra la que expresar la pose.
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

        let messages = [
            (ADDR_IRIS_CAM_X, norm_pos[0]),
            (ADDR_IRIS_CAM_Y, norm_pos[1]),
            (ADDR_IRIS_CAM_Z, norm_pos[2]),
            (ADDR_IRIS_CAM_RX, norm_rot[0]),
            (ADDR_IRIS_CAM_RY, norm_rot[1]),
            (ADDR_IRIS_CAM_RZ, norm_rot[2]),
        ]
        .into_iter()
        .map(|(addr, v)| OscMessage { addr: addr.into(), args: vec![OscType::Float(v)] })
        .collect();

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
