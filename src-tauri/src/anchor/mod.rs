//! Sistema de anclaje dinámico, persistente y autorrecuperable del operador local de cámara.
//!
//! Principios de ingeniería y restricciones reales de VRChat OSC (`docs/osc-findings.md`):
//! 1. En VRChat OSC, `/usercamera/Pose` reporta la pose en mundo de la **cámara**, no la del avatar.
//!    VRChat no expone `/player/pose` ni identificadores de jugadores remotos.
//! 2. `/usercamera/Mode` (`0` = Off, `1..=6` = Photo, Stream, Emoji, Multilayer, Print, Drone) indica
//!    el estado operativo de la cámara. Al conmutar entre modos activos (p. ej. `Photo 1 <-> Stream 2`),
//!    VRChat puede emitir un `Mode 0` transitorio (`1 -> 0 -> 2` en ~0.2–2.0 s).
//! 3. `/usercamera/LookAtMe` apunta exclusivamente al operador local (nunca a jugadores remotos).
//! 4. `AnchorManager` centraliza la fuente de verdad del ancla del operador local con una máquina de
//!    estados explícita (`Inactive`, `Acquiring`, `Active`, `SyncLost`, `Recovering`, `Released`),
//!    protegiendo el ancla contra sobrescrituras por ecos de poses comandadas, movimientos de cámara
//!    hacia jugadores remotos, paquetes duplicados/retrasados y cortes temporales de comunicación.
//!
//! Ejes de Unity/VRChat (medidos en M0): y arriba; adelante = (sin yaw, cos yaw) en (x, z); pitch + = mirar abajo.
use crate::osc::protocol::Pose;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Ventana de gracia (ms) para distinguir un cambio de modo en VRChat (`1 -> 0 -> 2`) de un cierre definitivo.
pub const MODE_SWITCH_GRACE_MS: u64 = 2500;
/// Ventana temporal (ms) durante la cual un eco de `/usercamera/Pose` puede corresponder a una pose enviada por Velza Iris.
pub const COMMANDED_ECHO_WINDOW_MS: u64 = 350;
/// Ventana temporal (ms) para deduplicar ecos dobles idénticos emitidos por VRChat (~5 ms en M0).
pub const DUPLICATE_POSE_WINDOW_MS: u64 = 8;
/// Tiempo máximo sin paquetes (ms) durante control activo antes de declarar pérdida temporal de sincronización.
pub const ACTIVE_SYNC_TIMEOUT_MS: u64 = 3000;
/// Capacidad máxima del historial auditable de eventos del ancla.
const MAX_AUDIT_LOG_ENTRIES: usize = 64;
/// Capacidad máxima del historial de poses comandadas para el filtro anti-eco.
const MAX_COMMANDED_HISTORY: usize = 48;

/// Origen de los presets: posición de los ojos del jugador y hacia dónde mira (yaw en grados).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub eye: [f32; 3],
    pub yaw: f32,
}

/// Encuadre relativo al ancla. La cámara siempre mira a los ojos del jugador; `pitch`/`roll` se suman.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Distancia horizontal a los ojos (m).
    pub distance: f32,
    /// Altura respecto a los ojos (m).
    pub height: f32,
    /// Posición alrededor del jugador (°): 0 = frente a él, + = hacia su derecha.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
}

impl Anchor {
    /// "Calibrar": el usuario acerca la cámara a su cara mirando al frente; esa pose es el origen.
    pub fn from_pose(p: &Pose) -> Anchor {
        Anchor { eye: p.pos, yaw: p.rot[1] }
    }

    /// Pose de mundo de la cámara para `f`.
    pub fn pose(&self, f: &Frame) -> Pose {
        let a = (self.yaw + f.yaw).to_radians();
        let [x, y, z] = self.eye;
        Pose {
            pos: [x + f.distance * a.sin(), y + f.height, z + f.distance * a.cos()],
            rot: [
                f.height.atan2(f.distance).to_degrees() + f.pitch,
                (self.yaw + f.yaw + 180.0).rem_euclid(360.0),
                f.roll,
            ],
        }
    }

    /// Transforma una Pose arbitraria 6DoF de mundo al espacio relativo del ancla (Anchor Space).
    /// En Anchor Space: origen = `self.eye`, eje +Z = frente del ancla (`self.yaw`), +X = derecha, +Y = arriba.
    pub fn world_to_local(&self, world: &Pose) -> Pose {
        let dx = world.pos[0] - self.eye[0];
        let dy = world.pos[1] - self.eye[1];
        let dz = world.pos[2] - self.eye[2];
        let a = self.yaw.to_radians();
        let (s, c) = a.sin_cos();
        // Base del ancla en mundo: forward = (sin a, 0, cos a), right = (cos a, 0, -sin a)
        let local_x = dx * c - dz * s;
        let local_z = dx * s + dz * c;
        let wrap180 = |deg: f32| {
            let r = deg.rem_euclid(360.0);
            if r > 180.0 { r - 360.0 } else { r }
        };
        Pose {
            pos: [local_x, dy, local_z],
            rot: [
                wrap180(world.rot[0]),
                wrap180(world.rot[1] - self.yaw),
                wrap180(world.rot[2]),
            ],
        }
    }

    /// Reconstruye una Pose de mundo a partir de una Pose en espacio relativo del ancla, con factor de escala espacial.
    pub fn local_to_world(&self, local: &Pose, scale: f32) -> Pose {
        let sc = if scale.is_finite() && scale > 0.01 { scale.clamp(0.1, 10.0) } else { 1.0 };
        let lx = local.pos[0] * sc;
        let ly = local.pos[1] * sc;
        let lz = local.pos[2] * sc;
        let a = self.yaw.to_radians();
        let (s, c) = a.sin_cos();
        let wx = self.eye[0] + lx * c + lz * s;
        let wy = self.eye[1] + ly;
        let wz = self.eye[2] - lx * s + lz * c;
        let wrap180 = |deg: f32| {
            let r = deg.rem_euclid(360.0);
            if r > 180.0 { r - 360.0 } else { r }
        };
        Pose {
            pos: [wx, wy, wz],
            rot: [
                wrap180(local.rot[0]).clamp(-90.0, 90.0),
                wrap180(local.rot[1] + self.yaw),
                wrap180(local.rot[2]),
            ],
        }
    }
}

/// Estados explícitos del ciclo de vida del ancla del operador local.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorLifecycleState {
    /// Cámara inactiva (`Mode == 0`) y sin sesión de anclaje iniciada.
    #[default]
    Inactive,
    /// Cámara recién activada (`Mode: 0 -> 1..=6`), esperando la primera lectura válida del operador local.
    Acquiring,
    /// Ancla adquirida, validada y bloqueada sobre el operador local.
    Active,
    /// Pérdida temporal de sincronización OSC o transición `Mode == 0` dentro de la ventana de cambio de modo.
    /// Conserva el ancla del operador local en memoria para recuperación sin reinicialización.
    SyncLost,
    /// Validando el restablecimiento de sincronización OSC antes de volver a `Active`.
    Recovering,
    /// Ancla liberada limpiamente tras cierre confirmado de la cámara o acción explícita del operador.
    Released,
}

/// Procedencia verificada del ancla activa.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorProvenance {
    /// Establecida determinísticamente al detectar la activación de la cámara del operador local.
    AutoOnActivation,
    /// Calibrada explícitamente por el operador local frente a su rostro (máxima prioridad).
    OperatorCalibrated,
}

/// Causa de una pérdida temporal de sincronización.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncLossCause {
    /// `Mode == 0` recibido mientras el ancla estaba activa (en espera para distinguir cambio de modo vs cierre).
    TransientModeZero,
    /// Interrupción o desconexión del transporte OSC.
    OscDisconnected,
    /// Silencio prolongado de paquetes OSC durante operación activa en lazo cerrado.
    PacketTimeout,
}

/// Registro auditable de transiciones y eventos del sistema de anclaje.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorAuditEntry {
    pub timestamp_ms: u64,
    pub from_state: AnchorLifecycleState,
    pub to_state: AnchorLifecycleState,
    pub camera_mode: u8,
    pub provenance: Option<AnchorProvenance>,
    pub reason: String,
}

/// Instantánea completa e inmutable del estado del `AnchorManager` para la UI y telemetría.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorSnapshot {
    pub state: AnchorLifecycleState,
    pub anchor: Option<Anchor>,
    pub provenance: Option<AnchorProvenance>,
    pub camera_mode: u8,
    pub session_id: u64,
    pub acquired_at_ms: Option<u64>,
    pub last_verified_ms: Option<u64>,
    pub locked_to_operator: bool,
    pub look_at_me_active: Option<bool>,
    pub mode_switch_count: u32,
    pub recovery_count: u32,
    pub rejected_packets_count: u32,
    pub audit_log: Vec<AnchorAuditEntry>,
}

fn is_valid_pose(p: &Pose) -> bool {
    p.pos.iter().chain(&p.rot).all(|v| v.is_finite())
}

fn pose_distance(a: &Pose, b: &Pose) -> f32 {
    let dx = a.pos[0] - b.pos[0];
    let dy = a.pos[1] - b.pos[1];
    let dz = a.pos[2] - b.pos[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn angle_diff_deg(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

fn pose_rot_max_diff(a: &Pose, b: &Pose) -> f32 {
    (0..3)
        .map(|i| angle_diff_deg(a.rot[i], b.rot[i]).abs())
        .fold(0.0_f32, f32::max)
}

/// Gestor centralizado del ancla del operador local.
pub struct AnchorManager {
    state: AnchorLifecycleState,
    anchor: Option<Anchor>,
    provenance: Option<AnchorProvenance>,
    camera_mode: u8,
    /// Último modo activo no nulo (`1..=6`) visto antes de una transición a `0`.
    last_nonzero_mode: u8,
    session_id: u64,
    acquired_at_ms: Option<u64>,
    last_verified_ms: Option<u64>,
    last_event_ms: u64,
    last_pose_seq: u64,
    last_observed_pose: Option<(u64, Pose)>,
    locked_to_operator: bool,
    look_at_me_active: Option<bool>,
    look_at_me_offsets: [f32; 2],
    sync_loss_cause: Option<SyncLossCause>,
    mode_zero_since_ms: Option<u64>,
    /// Indica si un motor de Velza Iris (`Runner`, `StudioRunner`, `Turnaround`) está conduciendo la cámara.
    external_control_active: bool,
    /// Historial de poses enviadas por Velza Iris para filtrar ecos propios.
    commanded_poses: VecDeque<(u64, Pose)>,
    mode_switch_count: u32,
    recovery_count: u32,
    rejected_packets_count: u32,
    audit_log: Vec<AnchorAuditEntry>,
}

impl Default for AnchorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AnchorManager {
    pub fn new() -> Self {
        AnchorManager {
            state: AnchorLifecycleState::Inactive,
            anchor: None,
            provenance: None,
            camera_mode: 0,
            last_nonzero_mode: 0,
            session_id: 0,
            acquired_at_ms: None,
            last_verified_ms: None,
            last_event_ms: 0,
            last_pose_seq: 0,
            last_observed_pose: None,
            locked_to_operator: false,
            look_at_me_active: None,
            look_at_me_offsets: [0.0, 0.0],
            sync_loss_cause: None,
            mode_zero_since_ms: None,
            external_control_active: false,
            commanded_poses: VecDeque::with_capacity(MAX_COMMANDED_HISTORY),
            mode_switch_count: 0,
            recovery_count: 0,
            rejected_packets_count: 0,
            audit_log: Vec::new(),
        }
    }

    pub fn state(&self) -> AnchorLifecycleState {
        self.state
    }

    /// Devuelve el ancla activa del operador local (incluyendo cuando se conserva en memoria
    /// durante `SyncLost` o `Recovering` para no interrumpir ni desincronizar los motores en curso).
    pub fn active_anchor(&self) -> Option<Anchor> {
        match self.state {
            AnchorLifecycleState::Active
            | AnchorLifecycleState::SyncLost
            | AnchorLifecycleState::Recovering
            | AnchorLifecycleState::Acquiring => self.anchor,
            AnchorLifecycleState::Inactive | AnchorLifecycleState::Released => None,
        }
    }

    pub fn snapshot(&self) -> AnchorSnapshot {
        AnchorSnapshot {
            state: self.state,
            anchor: self.active_anchor(),
            provenance: self.provenance,
            camera_mode: self.camera_mode,
            session_id: self.session_id,
            acquired_at_ms: self.acquired_at_ms,
            last_verified_ms: self.last_verified_ms,
            locked_to_operator: self.locked_to_operator,
            look_at_me_active: self.look_at_me_active,
            mode_switch_count: self.mode_switch_count,
            recovery_count: self.recovery_count,
            rejected_packets_count: self.rejected_packets_count,
            audit_log: self.audit_log.clone(),
        }
    }

    /// Indica si la calibración adaptativa debe pausarse para evitar deriva acumulativa (p. ej. en `SyncLost` o `Recovering`).
    pub fn is_sync_healthy(&self) -> bool {
        self.state == AnchorLifecycleState::Active
    }

    /// Registra si un motor de movimiento de Velza Iris está conduciendo activamente la cámara.
    pub fn set_external_control_active(&mut self, active: bool) {
        self.external_control_active = active;
    }

    pub fn is_external_control_active(&self) -> bool {
        self.external_control_active
    }

    /// Registra una `Pose` enviada por Velza Iris hacia VRChat para no confundir su eco con el operador local.
    pub fn record_commanded_pose(&mut self, pose: Pose, timestamp_ms: u64) {
        if !is_valid_pose(&pose) {
            return;
        }
        while let Some((t, _)) = self.commanded_poses.front() {
            if timestamp_ms.saturating_sub(*t) > COMMANDED_ECHO_WINDOW_MS * 3 {
                self.commanded_poses.pop_front();
            } else {
                break;
            }
        }
        if self.commanded_poses.len() >= MAX_COMMANDED_HISTORY {
            self.commanded_poses.pop_front();
        }
        self.commanded_poses.push_back((timestamp_ms, pose));
    }

    /// Comprueba si una `Pose` recibida por OSC coincide con el eco esperado de una pose comandada por Velza Iris.
    pub fn is_commanded_echo(&self, observed: &Pose, timestamp_ms: u64) -> bool {
        self.commanded_poses.iter().rev().any(|(sent_ms, sent_pose)| {
            timestamp_ms >= *sent_ms
                && timestamp_ms.saturating_sub(*sent_ms) <= COMMANDED_ECHO_WINDOW_MS
                && pose_distance(observed, sent_pose) <= 0.025
                && pose_rot_max_diff(observed, sent_pose) <= 1.5
        })
    }

    fn transition_to(&mut self, next: AnchorLifecycleState, timestamp_ms: u64, reason: impl Into<String>) {
        let from = self.state;
        self.state = next;
        let reason: String = reason.into();
        crate::logbus::engine("engine/anchor", format!("{from:?} → {next:?}: {reason}"));
        if self.audit_log.len() >= MAX_AUDIT_LOG_ENTRIES {
            self.audit_log.remove(0);
        }
        self.audit_log.push(AnchorAuditEntry {
            timestamp_ms,
            from_state: from,
            to_state: next,
            camera_mode: self.camera_mode,
            provenance: self.provenance,
            reason,
        });
    }

    /// Evalúa expiración de ventanas temporales (cierre confirmado tras `MODE_SWITCH_GRACE_MS` o timeout OSC).
    pub fn check_timeouts(&mut self, now_ms: u64) {
        if self.state == AnchorLifecycleState::SyncLost
            && self.sync_loss_cause == Some(SyncLossCause::TransientModeZero)
            && let Some(zero_since) = self.mode_zero_since_ms
            && now_ms.saturating_sub(zero_since) > MODE_SWITCH_GRACE_MS
        {
            let release_ms = zero_since + MODE_SWITCH_GRACE_MS;
            self.anchor = None;
            self.provenance = None;
            self.locked_to_operator = false;
            self.mode_zero_since_ms = None;
            self.sync_loss_cause = None;
            self.transition_to(
                AnchorLifecycleState::Released,
                release_ms,
                "Cierre de cámara confirmado tras expirar ventana de cambio de modo",
            );
        } else if self.state == AnchorLifecycleState::Active
            && self.external_control_active
            && let Some(last_ver) = self.last_verified_ms
            && now_ms.saturating_sub(last_ver) > ACTIVE_SYNC_TIMEOUT_MS
        {
            self.sync_loss_cause = Some(SyncLossCause::PacketTimeout);
            self.transition_to(
                AnchorLifecycleState::SyncLost,
                now_ms,
                "Tiempo de espera de sincronización OSC excedido; ancla conservada en memoria",
            );
        }
    }

    /// Procesa un cambio o eco de `/usercamera/Mode` (`0..=6`).
    pub fn on_mode(&mut self, raw_mode: i32, timestamp_ms: u64) {
        if timestamp_ms < self.last_event_ms {
            self.rejected_packets_count = self.rejected_packets_count.saturating_add(1);
            return;
        }
        self.check_timeouts(timestamp_ms);
        self.last_event_ms = timestamp_ms;

        if !(0..=6).contains(&raw_mode) {
            self.rejected_packets_count = self.rejected_packets_count.saturating_add(1);
            return;
        }
        let mode = raw_mode as u8;

        if mode == 0 {
            match self.state {
                AnchorLifecycleState::Inactive | AnchorLifecycleState::Released => {
                    self.camera_mode = 0;
                }
                AnchorLifecycleState::Acquiring => {
                    // Se cerró antes de adquirir pose inicial
                    self.camera_mode = 0;
                    self.anchor = None;
                    self.locked_to_operator = false;
                    self.transition_to(
                        AnchorLifecycleState::Released,
                        timestamp_ms,
                        "Cámara desactivada durante fase de adquisición",
                    );
                }
                AnchorLifecycleState::Active | AnchorLifecycleState::Recovering => {
                    // En VRChat, cambiar de modo (ej. Photo 1 -> Stream 2) emite Mode 0 brevemente.
                    // Pasamos a SyncLost conservando intacta el ancla del operador local.
                    self.camera_mode = 0;
                    self.sync_loss_cause = Some(SyncLossCause::TransientModeZero);
                    self.mode_zero_since_ms = Some(timestamp_ms);
                    self.transition_to(
                        AnchorLifecycleState::SyncLost,
                        timestamp_ms,
                        "Modo 0 recibido: ancla retenida en ventana de conmutación de modo",
                    );
                }
                AnchorLifecycleState::SyncLost => {
                    self.camera_mode = 0;
                }
            }
            return;
        }

        // mode in 1..=6 (cámara activa)
        let prev_nonzero = self.last_nonzero_mode;
        self.last_nonzero_mode = mode;

        match self.state {
            AnchorLifecycleState::Inactive | AnchorLifecycleState::Released => {
                self.camera_mode = mode;
                self.session_id = self.session_id.wrapping_add(1);
                self.mode_zero_since_ms = None;
                self.sync_loss_cause = None;
                self.transition_to(
                    AnchorLifecycleState::Acquiring,
                    timestamp_ms,
                    format!("Transición a cámara activa (Mode {mode}); iniciando adquisición del operador local"),
                );
            }
            AnchorLifecycleState::Acquiring => {
                if self.camera_mode != mode {
                    let from_m = self.camera_mode;
                    self.camera_mode = mode;
                    self.mode_switch_count = self.mode_switch_count.saturating_add(1);
                    self.transition_to(
                        AnchorLifecycleState::Acquiring,
                        timestamp_ms,
                        format!("Cambio de modo durante adquisición ({from_m} -> {mode})"),
                    );
                }
            }
            AnchorLifecycleState::Active => {
                if self.camera_mode != mode {
                    let from_m = self.camera_mode;
                    self.camera_mode = mode;
                    self.mode_switch_count = self.mode_switch_count.saturating_add(1);
                    self.last_verified_ms = Some(timestamp_ms);
                    self.transition_to(
                        AnchorLifecycleState::Active,
                        timestamp_ms,
                        format!("Cambio directo de modo de cámara ({from_m} -> {mode}): ancla del operador conservada"),
                    );
                } else {
                    // Eco duplicado del mismo modo activo: ignorar sin alterar el estado
                    self.last_verified_ms = Some(timestamp_ms);
                }
            }
            AnchorLifecycleState::SyncLost => {
                let cause = self.sync_loss_cause;
                self.camera_mode = mode;
                self.mode_zero_since_ms = None;
                self.sync_loss_cause = None;

                self.transition_to(
                    AnchorLifecycleState::Recovering,
                    timestamp_ms,
                    format!("Modo activo {mode} recibido tras pérdida temporal de sincronización"),
                );

                // Si disponemos de un ancla válida del operador local y fue una transición de modo (ej. 1 -> 0 -> 2)
                // o reconexión con cámara abierta, restauramos el estado Active sin reinicializar el ancla.
                if self.anchor.is_some() {
                    if prev_nonzero != 0 && prev_nonzero != mode {
                        self.mode_switch_count = self.mode_switch_count.saturating_add(1);
                    }
                    self.recovery_count = self.recovery_count.saturating_add(1);
                    self.last_verified_ms = Some(timestamp_ms);
                    let reason = match cause {
                        Some(SyncLossCause::TransientModeZero) => format!(
                            "Recuperación completada tras conmutación de modo ({prev_nonzero} -> 0 -> {mode}): ancla conservada"
                        ),
                        _ => format!("Sincronización recuperada en modo {mode}: ancla del operador restaurada"),
                    };
                    self.transition_to(AnchorLifecycleState::Active, timestamp_ms, reason);
                } else {
                    self.transition_to(
                        AnchorLifecycleState::Acquiring,
                        timestamp_ms,
                        format!("Reanudando adquisición de ancla en modo {mode}"),
                    );
                }
            }
            AnchorLifecycleState::Recovering => {
                self.camera_mode = mode;
                if self.anchor.is_some() {
                    self.recovery_count = self.recovery_count.saturating_add(1);
                    self.last_verified_ms = Some(timestamp_ms);
                    self.transition_to(
                        AnchorLifecycleState::Active,
                        timestamp_ms,
                        format!("Sincronización confirmada en modo {mode}: ancla del operador activa"),
                    );
                }
            }
        }
    }

    /// Procesa una actualización de `/usercamera/LookAtMe` y sus offsets.
    pub fn on_look_at_me(&mut self, enabled: bool, offsets_xy: Option<[f32; 2]>, timestamp_ms: u64) {
        if timestamp_ms < self.last_event_ms {
            self.rejected_packets_count = self.rejected_packets_count.saturating_add(1);
            return;
        }
        self.last_event_ms = timestamp_ms;
        self.look_at_me_active = Some(enabled);
        if let Some([x, y]) = offsets_xy
            && x.is_finite() && y.is_finite()
        {
            self.look_at_me_offsets = [x, y];
        }
    }

    /// Procesa un paquete `/usercamera/Pose` recibido de VRChat.
    ///
    /// Regla fundamental:
    /// - Si el sistema está en `Inactive`/`Released`/`Acquiring`, adquiere determinísticamente el ancla
    ///   del operador local únicamente si la pose es válida, fresca y **no** es un eco de una pose
    ///   comandada por Velza Iris.
    /// - Si el sistema está en `Active`, la pose de la cámara **no** sobrescribe el ancla del operador,
    ///   evitando que mover la cámara hacia otro jugador o ejecutar una trayectoria desplace el ancla.
    pub fn on_pose(&mut self, pose: Pose, seq: u64, timestamp_ms: u64) {
        if !is_valid_pose(&pose) {
            self.rejected_packets_count = self.rejected_packets_count.saturating_add(1);
            return;
        }
        if timestamp_ms < self.last_event_ms || (seq != 0 && seq < self.last_pose_seq) {
            self.rejected_packets_count = self.rejected_packets_count.saturating_add(1);
            return;
        }

        // Deduplicar ecos dobles idénticos en ráfaga (~5 ms)
        if let Some((prev_ms, prev_pose)) = self.last_observed_pose
            && timestamp_ms.saturating_sub(prev_ms) <= DUPLICATE_POSE_WINDOW_MS
            && pose_distance(&pose, &prev_pose) < 1e-4
            && pose_rot_max_diff(&pose, &prev_pose) < 1e-3
        {
            self.last_event_ms = timestamp_ms;
            self.last_pose_seq = seq.max(self.last_pose_seq);
            if self.state == AnchorLifecycleState::Active {
                self.last_verified_ms = Some(timestamp_ms);
            }
            return;
        }

        self.check_timeouts(timestamp_ms);
        self.last_event_ms = timestamp_ms;
        if seq != 0 {
            self.last_pose_seq = seq;
        }
        self.last_observed_pose = Some((timestamp_ms, pose));

        let is_echo = self.is_commanded_echo(&pose, timestamp_ms);

        match self.state {
            AnchorLifecycleState::Inactive | AnchorLifecycleState::Released => {
                // En VRChat, recibir Pose implica que la cámara está abierta.
                self.camera_mode = self.last_nonzero_mode.max(1);
                self.last_nonzero_mode = self.camera_mode;
                self.session_id = self.session_id.wrapping_add(1);
                self.transition_to(
                    AnchorLifecycleState::Acquiring,
                    timestamp_ms,
                    "Recepción de Pose con cámara activa; iniciando adquisición",
                );
                if !is_echo && !self.external_control_active {
                    self.anchor = Some(Anchor::from_pose(&pose));
                    self.provenance = Some(AnchorProvenance::AutoOnActivation);
                    self.locked_to_operator = true;
                    self.acquired_at_ms = Some(timestamp_ms);
                    self.last_verified_ms = Some(timestamp_ms);
                    self.transition_to(
                        AnchorLifecycleState::Active,
                        timestamp_ms,
                        "Ancla del operador local adquirida determinísticamente al abrir cámara",
                    );
                }
            }
            AnchorLifecycleState::Acquiring => {
                if is_echo || self.external_control_active {
                    // Ignorar ecos de movimientos enviados por la app para no anclar sobre un preset lejano
                    return;
                }
                self.anchor = Some(Anchor::from_pose(&pose));
                self.provenance = Some(AnchorProvenance::AutoOnActivation);
                self.locked_to_operator = true;
                self.acquired_at_ms = Some(timestamp_ms);
                self.last_verified_ms = Some(timestamp_ms);
                if self.camera_mode == 0 {
                    self.camera_mode = self.last_nonzero_mode.max(1);
                }
                self.transition_to(
                    AnchorLifecycleState::Active,
                    timestamp_ms,
                    "Ancla del operador local adquirida y bloqueada desde primera Pose válida",
                );
            }
            AnchorLifecycleState::Active => {
                // El ancla permanece asociada e inmutable al operador local; la Pose solo confirma salud de sincronización.
                self.last_verified_ms = Some(timestamp_ms);
            }
            AnchorLifecycleState::SyncLost => {
                if self.camera_mode == 0 {
                    self.camera_mode = self.last_nonzero_mode.max(1);
                }
                self.mode_zero_since_ms = None;
                self.sync_loss_cause = None;
                self.transition_to(
                    AnchorLifecycleState::Recovering,
                    timestamp_ms,
                    "Paquete Pose válido recibido durante pérdida de sincronización",
                );
                if self.anchor.is_some() {
                    self.recovery_count = self.recovery_count.saturating_add(1);
                    self.last_verified_ms = Some(timestamp_ms);
                    self.transition_to(
                        AnchorLifecycleState::Active,
                        timestamp_ms,
                        "Ancla del operador local recuperada intacta tras validación de Pose",
                    );
                } else if !is_echo && !self.external_control_active {
                    self.anchor = Some(Anchor::from_pose(&pose));
                    self.provenance = Some(AnchorProvenance::AutoOnActivation);
                    self.locked_to_operator = true;
                    self.acquired_at_ms = Some(timestamp_ms);
                    self.last_verified_ms = Some(timestamp_ms);
                    self.recovery_count = self.recovery_count.saturating_add(1);
                    self.transition_to(
                        AnchorLifecycleState::Active,
                        timestamp_ms,
                        "Ancla adquirida tras recuperación de sincronización",
                    );
                }
            }
            AnchorLifecycleState::Recovering => {
                if self.anchor.is_some() {
                    self.recovery_count = self.recovery_count.saturating_add(1);
                    self.last_verified_ms = Some(timestamp_ms);
                    self.transition_to(
                        AnchorLifecycleState::Active,
                        timestamp_ms,
                        "Recuperación verificada con Pose válida: ancla del operador conservada",
                    );
                } else if !is_echo && !self.external_control_active {
                    self.anchor = Some(Anchor::from_pose(&pose));
                    self.provenance = Some(AnchorProvenance::AutoOnActivation);
                    self.locked_to_operator = true;
                    self.acquired_at_ms = Some(timestamp_ms);
                    self.last_verified_ms = Some(timestamp_ms);
                    self.recovery_count = self.recovery_count.saturating_add(1);
                    self.transition_to(
                        AnchorLifecycleState::Active,
                        timestamp_ms,
                        "Ancla establecida y activada tras recuperación",
                    );
                }
            }
        }
    }

    /// Calibración explícita solicitada por el operador local (`Calibrar ancla`).
    /// Protege contra calibraciones accidentales sobre poses obsoletas o mientras un motor mueve la cámara.
    pub fn calibrate_manual(
        &mut self,
        pose: Pose,
        pose_fresh: bool,
        timestamp_ms: u64,
    ) -> Result<Anchor, String> {
        if !pose_fresh {
            return Err("VRChat aún no reporta una posición reciente de la cámara: muévela ligeramente frente a ti.".into());
        }
        if !is_valid_pose(&pose) {
            return Err("La pose recibida contiene valores no finitos.".into());
        }
        if self.external_control_active {
            return Err("Detén el movimiento o secuencia activa antes de recalibrar el ancla del operador.".into());
        }
        if self.is_commanded_echo(&pose, timestamp_ms) && self.anchor.is_some() {
            return Err("La posición actual corresponde a un encuadre aplicado por la app. Acerca la cámara a tu rostro en VRChat y muévela ligeramente para recalibrar tu ancla.".into());
        }

        let new_anchor = Anchor::from_pose(&pose);
        self.anchor = Some(new_anchor);
        self.provenance = Some(AnchorProvenance::OperatorCalibrated);
        self.locked_to_operator = true;
        self.acquired_at_ms = Some(timestamp_ms);
        self.last_verified_ms = Some(timestamp_ms);
        self.last_event_ms = timestamp_ms.max(self.last_event_ms);
        if self.camera_mode == 0 {
            self.camera_mode = self.last_nonzero_mode.max(1);
            self.last_nonzero_mode = self.camera_mode;
        }
        if matches!(
            self.state,
            AnchorLifecycleState::Inactive | AnchorLifecycleState::Released
        ) {
            self.session_id = self.session_id.wrapping_add(1);
        }
        self.mode_zero_since_ms = None;
        self.sync_loss_cause = None;
        self.transition_to(
            AnchorLifecycleState::Active,
            timestamp_ms,
            "Ancla calibrada explícitamente y bloqueada sobre el operador local",
        );
        Ok(new_anchor)
    }

    /// Notifica una interrupción de la conexión OSC conservando el ancla en `SyncLost` si estaba activa.
    pub fn on_osc_disconnected(&mut self, timestamp_ms: u64, reason: impl Into<String>) {
        self.last_event_ms = timestamp_ms.max(self.last_event_ms);
        match self.state {
            AnchorLifecycleState::Active
            | AnchorLifecycleState::Acquiring
            | AnchorLifecycleState::Recovering => {
                self.sync_loss_cause = Some(SyncLossCause::OscDisconnected);
                self.transition_to(AnchorLifecycleState::SyncLost, timestamp_ms, reason);
            }
            _ => {}
        }
    }

    /// Notifica que el transporte OSC se reconectó e inicia la fase `Recovering` si había un ancla en `SyncLost`.
    pub fn on_osc_reconnected(&mut self, timestamp_ms: u64) {
        self.last_event_ms = timestamp_ms.max(self.last_event_ms);
        if self.state == AnchorLifecycleState::SyncLost {
            self.transition_to(
                AnchorLifecycleState::Recovering,
                timestamp_ms,
                "Conexión OSC restablecida; validando coherencia del ancla",
            );
        }
    }

    /// Libera explícitamente el ancla actual (`Released`).
    pub fn release(&mut self, timestamp_ms: u64, reason: impl Into<String>) {
        self.last_event_ms = timestamp_ms.max(self.last_event_ms);
        self.anchor = None;
        self.provenance = None;
        self.locked_to_operator = false;
        self.mode_zero_since_ms = None;
        self.sync_loss_cause = None;
        self.external_control_active = false;
        self.transition_to(AnchorLifecycleState::Released, timestamp_ms, reason);
    }

    /// Restablece el gestor al estado inicial `Inactive` tras liberar el ancla.
    pub fn reset_to_inactive(&mut self, timestamp_ms: u64) {
        if self.state != AnchorLifecycleState::Released && self.state != AnchorLifecycleState::Inactive {
            self.release(timestamp_ms, "Liberación previa a reinicio a inactividad");
        }
        self.camera_mode = 0;
        self.transition_to(
            AnchorLifecycleState::Inactive,
            timestamp_ms,
            "Sistema de anclaje en reposo (Inactive)",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EYE: [f32; 3] = [0.0, 1.6, 0.0];

    fn frame(distance: f32, height: f32, yaw: f32) -> Frame {
        Frame { distance, height, yaw, pitch: 0.0, roll: 0.0 }
    }

    fn assert_pose(p: Pose, pos: [f32; 3], rot: [f32; 3]) {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert!(p.pos.iter().zip(pos).all(|(a, b)| near(*a, b)), "pos {:?} != {pos:?}", p.pos);
        assert!(p.rot.iter().zip(rot).all(|(a, b)| near(*a, b)), "rot {:?} != {rot:?}", p.rot);
    }

    #[test]
    fn front_and_right_of_player() {
        let a = Anchor { eye: EYE, yaw: 0.0 };
        // de frente, mirando de vuelta al jugador (yaw 180)
        assert_pose(a.pose(&frame(2.0, 0.0, 0.0)), [0.0, 1.6, 2.0], [0.0, 180.0, 0.0]);
        // a su derecha (+x) mirando hacia -x
        assert_pose(a.pose(&frame(2.0, 0.0, 90.0)), [2.0, 1.6, 0.0], [0.0, 270.0, 0.0]);
    }

    #[test]
    fn follows_anchor_yaw() {
        // jugador mirando a +x: "de frente" queda en +x
        let a = Anchor { eye: EYE, yaw: 90.0 };
        assert_pose(a.pose(&frame(2.0, 0.0, 0.0)), [2.0, 1.6, 0.0], [0.0, 270.0, 0.0]);
    }

    #[test]
    fn height_tilts_and_offsets_add() {
        let a = Anchor { eye: EYE, yaw: 0.0 };
        // arriba y a 1 m: mira hacia abajo 45°
        assert_pose(a.pose(&frame(1.0, 1.0, 0.0)), [0.0, 2.6, 1.0], [45.0, 180.0, 0.0]);
        // abajo: contrapicado (pitch negativo = mirar arriba)
        assert_pose(a.pose(&frame(1.8, -0.6, 0.0)), [0.0, 1.0, 1.8], [-18.434, 180.0, 0.0]);
        let f = Frame { pitch: 5.0, roll: 12.0, ..frame(1.0, 1.0, 0.0) };
        assert_pose(a.pose(&f), [0.0, 2.6, 1.0], [50.0, 180.0, 12.0]);
    }

    #[test]
    fn calibrates_from_pose() {
        let p = Pose { pos: [3.0, 1.62, -2.0], rot: [4.0, -88.0, 0.0] };
        assert_eq!(Anchor::from_pose(&p), Anchor { eye: [3.0, 1.62, -2.0], yaw: -88.0 });
    }

    #[test]
    fn world_local_roundtrip_and_relocation() {
        let a1 = Anchor { eye: [2.5, 1.65, -4.0], yaw: 37.0 };
        let world_orig = Pose { pos: [4.1, 2.05, -1.8], rot: [-12.5, -145.0, 8.0] };
        let local = a1.world_to_local(&world_orig);
        let back = a1.local_to_world(&local, 1.0);
        assert_pose(back, world_orig.pos, world_orig.rot);

        // Si el jugador se mueve y gira 90°, la distancia al ancla se preserva exactamente (o escala por `scale`)
        let a2 = Anchor { eye: [-10.0, 1.65, 5.0], yaw: 127.0 };
        let relocated = a2.local_to_world(&local, 1.0);
        let dist1 = ((world_orig.pos[0] - a1.eye[0]).powi(2) + (world_orig.pos[2] - a1.eye[2]).powi(2)).sqrt();
        let dist2 = ((relocated.pos[0] - a2.eye[0]).powi(2) + (relocated.pos[2] - a2.eye[2]).powi(2)).sqrt();
        assert!((dist1 - dist2).abs() < 1e-3);

        let scaled = a2.local_to_world(&local, 2.0);
        let dist_scaled = ((scaled.pos[0] - a2.eye[0]).powi(2) + (scaled.pos[2] - a2.eye[2]).powi(2)).sqrt();
        assert!((dist_scaled - dist1 * 2.0).abs() < 1e-3);
    }

    #[test]
    fn deterministic_acquisition_on_camera_open_and_release_on_confirmed_close() {
        let mut mgr = AnchorManager::new();
        assert_eq!(mgr.state(), AnchorLifecycleState::Inactive);
        assert_eq!(mgr.active_anchor(), None);

        // Transición cámara inactiva (0) -> activa (1: Photo)
        mgr.on_mode(1, 1000);
        assert_eq!(mgr.state(), AnchorLifecycleState::Acquiring);
        assert_eq!(mgr.snapshot().camera_mode, 1);

        // Llega la primera Pose fresca del operador local al abrir la cámara
        let operator_pose = Pose { pos: [-0.87, 1.65, 1.93], rot: [0.0, 117.6, 0.0] };
        mgr.on_pose(operator_pose, 1, 1020);
        assert_eq!(mgr.state(), AnchorLifecycleState::Active);
        let expected_anchor = Anchor { eye: [-0.87, 1.65, 1.93], yaw: 117.6 };
        assert_eq!(mgr.active_anchor(), Some(expected_anchor));
        assert_eq!(mgr.snapshot().provenance, Some(AnchorProvenance::AutoOnActivation));
        assert!(mgr.snapshot().locked_to_operator);

        // Cerrar la cámara entra primero en SyncLost (ventana de gracia por posible cambio de modo)
        mgr.on_mode(0, 5000);
        assert_eq!(mgr.state(), AnchorLifecycleState::SyncLost);
        // Durante la ventana de gracia el ancla aún se retiene
        assert_eq!(mgr.active_anchor(), Some(expected_anchor));

        // Al superar MODE_SWITCH_GRACE_MS (2500 ms), el cierre se confirma y el ancla pasa a Released
        mgr.check_timeouts(5000 + MODE_SWITCH_GRACE_MS + 10);
        assert_eq!(mgr.state(), AnchorLifecycleState::Released);
        assert_eq!(mgr.active_anchor(), None);
    }

    #[test]
    fn preserves_anchor_across_direct_and_transient_zero_mode_switches() {
        let mut mgr = AnchorManager::new();
        let operator_pose = Pose { pos: [2.48, 1.55, 1.37], rot: [-1.5, -88.0, 0.0] };
        mgr.on_mode(1, 1000);
        mgr.on_pose(operator_pose, 1, 1020);
        let orig_anchor = mgr.active_anchor().unwrap();
        let orig_session = mgr.snapshot().session_id;

        // 1. Cambio directo de modo: Photo (1) -> Stream (2) -> Drone (6)
        mgr.on_mode(2, 2000);
        assert_eq!(mgr.state(), AnchorLifecycleState::Active);
        assert_eq!(mgr.active_anchor(), Some(orig_anchor));
        assert_eq!(mgr.snapshot().camera_mode, 2);
        assert_eq!(mgr.snapshot().mode_switch_count, 1);

        mgr.on_mode(6, 3000);
        assert_eq!(mgr.state(), AnchorLifecycleState::Active);
        assert_eq!(mgr.active_anchor(), Some(orig_anchor));
        assert_eq!(mgr.snapshot().camera_mode, 6);
        assert_eq!(mgr.snapshot().mode_switch_count, 2);

        // 2. Cambio real de VRChat con paso transitorio por Mode 0 (según `1792687527.log`: 6 -> 0 -> 1 en 1.2s)
        // Aunque la cámara haya sido movida lejos (p. ej. a [10.0, 2.0, -5.0]), el ancla NO debe reinicializarse
        let far_camera_pose = Pose { pos: [10.0, 2.0, -5.0], rot: [10.0, 45.0, 0.0] };
        mgr.on_pose(far_camera_pose, 2, 3500);
        assert_eq!(mgr.active_anchor(), Some(orig_anchor));

        mgr.on_mode(0, 4000);
        assert_eq!(mgr.state(), AnchorLifecycleState::SyncLost);
        assert_eq!(mgr.active_anchor(), Some(orig_anchor));

        mgr.on_mode(1, 5200); // 1.2s después (< 2500ms)
        assert_eq!(mgr.state(), AnchorLifecycleState::Active);
        assert_eq!(mgr.active_anchor(), Some(orig_anchor));
        assert_eq!(mgr.snapshot().session_id, orig_session);
        assert_eq!(mgr.snapshot().mode_switch_count, 3);
        assert_eq!(mgr.snapshot().recovery_count, 1);

        // Verificar que la transición SyncLost -> Recovering -> Active quedó registrada en el audit_log
        let states: Vec<_> = mgr.snapshot().audit_log.iter().map(|e| (e.from_state, e.to_state)).collect();
        assert!(states.contains(&(AnchorLifecycleState::SyncLost, AnchorLifecycleState::Recovering)));
        assert!(states.contains(&(AnchorLifecycleState::Recovering, AnchorLifecycleState::Active)));
    }

    #[test]
    fn prevents_accidental_remote_player_or_commanded_pose_overwrite() {
        let mut mgr = AnchorManager::new();
        mgr.on_mode(1, 1000);

        // Supongamos que un preset automático envía una Pose lejos antes de que llegue la Pose natural del operador
        let preset_pose = Pose { pos: [15.0, 1.8, -12.0], rot: [0.0, 180.0, 0.0] };
        mgr.record_commanded_pose(preset_pose, 1010);

        // Llega el eco de esa Pose comandada 85 ms después: debe ser filtrado y NO adquirido como ancla
        mgr.on_pose(preset_pose, 1, 1095);
        assert_eq!(mgr.state(), AnchorLifecycleState::Acquiring);
        assert_eq!(mgr.active_anchor(), None);

        // Ahora llega la Pose real del operador local
        let local_operator = Pose { pos: [1.0, 1.62, 0.5], rot: [0.0, 90.0, 0.0] };
        mgr.on_pose(local_operator, 2, 1200);
        assert_eq!(mgr.state(), AnchorLifecycleState::Active);
        let locked_anchor = mgr.active_anchor().unwrap();
        assert_eq!(locked_anchor.eye, [1.0, 1.62, 0.5]);

        // El operador vuela la cámara hacia un jugador remoto a 20 metros de distancia
        let remote_player_cam_pose = Pose { pos: [-18.5, 1.65, 14.2], rot: [5.0, -30.0, 0.0] };
        mgr.on_pose(remote_player_cam_pose, 3, 2000);
        // El ancla permanece estrictamente en el operador local
        assert_eq!(mgr.active_anchor(), Some(locked_anchor));

        // Si un motor cinematográfico está activo conduciendo la cámara, `calibrate_manual` rechaza sobrescribir
        mgr.set_external_control_active(true);
        assert!(mgr.calibrate_manual(remote_player_cam_pose, true, 2100).is_err());
        assert_eq!(mgr.active_anchor(), Some(locked_anchor));
        mgr.set_external_control_active(false);
    }

    #[test]
    fn handles_duplicate_delayed_stale_packets_and_osc_disconnection_recovery() {
        let mut mgr = AnchorManager::new();
        let p0 = Pose { pos: [0.0, 1.65, 0.0], rot: [0.0, 0.0, 0.0] };
        mgr.on_mode(1, 1000);
        mgr.on_pose(p0, 10, 1010);
        assert_eq!(mgr.state(), AnchorLifecycleState::Active);
        let log_len_before = mgr.snapshot().audit_log.len();

        // 1. Eco doble idéntico de VRChat a los 5 ms: se deduplica sin ensuciar el log
        mgr.on_pose(p0, 10, 1015);
        assert_eq!(mgr.snapshot().audit_log.len(), log_len_before);

        // 2. Paquete retrasado / fuera de orden (timestamp anterior o seq menor)
        let old_pose = Pose { pos: [99.0, 99.0, 99.0], rot: [0.0, 0.0, 0.0] };
        mgr.on_pose(old_pose, 9, 950);
        assert_eq!(mgr.snapshot().rejected_packets_count, 1);

        // 3. Pose con NaN es rechazada
        let nan_pose = Pose { pos: [f32::NAN, 1.6, 0.0], rot: [0.0, 0.0, 0.0] };
        mgr.on_pose(nan_pose, 11, 1050);
        assert_eq!(mgr.snapshot().rejected_packets_count, 2);

        // 4. Calibración con pose obsoleta (`pose_fresh = false`) es rechazada
        assert!(mgr.calibrate_manual(p0, false, 1100).is_err());

        // 5. Interrupción de conexión OSC -> SyncLost -> Reconexión -> Recovering -> Pose -> Active
        mgr.on_osc_disconnected(2000, "Pérdida de socket UDP");
        assert_eq!(mgr.state(), AnchorLifecycleState::SyncLost);
        assert_eq!(mgr.active_anchor(), Some(Anchor::from_pose(&p0)));

        mgr.on_osc_reconnected(3000);
        assert_eq!(mgr.state(), AnchorLifecycleState::Recovering);
        assert_eq!(mgr.active_anchor(), Some(Anchor::from_pose(&p0)));

        mgr.on_pose(p0, 12, 3050);
        assert_eq!(mgr.state(), AnchorLifecycleState::Active);
        assert_eq!(mgr.active_anchor(), Some(Anchor::from_pose(&p0)));
        assert_eq!(mgr.snapshot().recovery_count, 1);
    }
}

