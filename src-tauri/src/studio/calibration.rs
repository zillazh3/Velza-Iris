//! Sistema de calibración técnica y visual adaptativa + telemetría de lazo cerrado/abierto.
//!
//! Principios de diseño:
//! 1. Totalmente independiente del motor cinematográfico: puede desactivarse (`enabled = false`) sin afectar a Iris.
//! 2. No destructivo: jamás modifica las coordenadas originales (`raw_pose` / `local_pose`) de los keyframes.
//! 3. Clasificación honesta de datos: distingue `Measured` (eco real reciente), `Estimated` (inferido/compensado)
//!    y `Unknown` (cámara cerrada, puerto 9001 ocupado, o intervención externa).
//! 4. Anti-oscilación: línea de retardo (`DelayLine` para compensar ~85 ms de eco de VRChat), banda muerta,
//!    detector de inversión de signo, límite de velocidad de ajuste (*slew-rate*) y tope máximo de magnitud.

use super::math3d::{angle_diff_deg, clamp_vec3_len, dist3, lerp3, norm3, wrap180, Quat};
use super::model::CalibrationConfig;
use crate::anchor::AnchorLifecycleState;
use crate::camera::CameraSnapshot;
use crate::osc::protocol::Pose;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Banda muerta de posición (5 mm): errores menores no disparan correcciones repetitivas.
pub const POS_DEADBAND_M: f32 = 0.005;
/// Banda muerta angular (0.25°).
pub const ROT_DEADBAND_DEG: f32 = 0.25;
/// Magnitud máxima de corrección automática de posición (15 cm).
pub const MAX_POS_CORRECTION_M: f32 = 0.15;
/// Magnitud máxima de corrección automática de rotación (5°).
pub const MAX_ROT_CORRECTION_DEG: f32 = 5.0;
/// Velocidad máxima de cambio de la corrección de posición (5 cm/s).
pub const MAX_POS_SLEW_MPS: f32 = 0.05;
/// Velocidad máxima de cambio de la corrección angular (2 °/s).
pub const MAX_ROT_SLEW_DPS: f32 = 2.0;
/// Umbral de anomalía (50 cm): si la cámara en VRChat difiere más que esto, el usuario la movió a mano
/// o el Anchor de VRChat está en modo World; se pausa la corrección y se pasa a lazo abierto.
pub const ANOMALY_POS_THRESHOLD_M: f32 = 0.50;
/// Latencia típica de eco de `/usercamera/Pose` medida en M0 (`docs/osc-findings.md`).
pub const DEFAULT_ECHO_LATENCY_MS: u64 = 85;

/// Clasificación explícita de la fuente de datos observada.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DataCertainty {
    /// Lectura real de VRChat recibida por UDP con antigüedad `< 500 ms`.
    #[serde(rename_all = "camelCase")]
    Measured { seq: u64, age_ms: u64 },
    /// Estimación basada en la última lectura conocida (cámara abierta pero quieta `>= 500 ms` o predicción).
    #[serde(rename_all = "camelCase")]
    Estimated { confidence: f32, reason: String },
    /// Estado desconocido (sin conexión, puerto 9001 ocupado, cámara cerrada o sin primera lectura).
    #[serde(rename_all = "camelCase")]
    Unknown { reason: String },
}

/// Estado operativo del lazo de realimentación.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum ControlLoopMode {
    /// Realimentación activa y verificada con evidencia suficiente.
    ClosedLoop,
    /// Control abierto: sin realimentación suficiente; no simula calibración real.
    #[serde(rename_all = "camelCase")]
    OpenLoop { reason: String },
}

/// Entrada auditable del registro de calibración.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationLogEntry {
    pub timestamp_ms: u64,
    pub category: String,
    pub delta_pos: [f32; 3],
    pub delta_rot: [f32; 3],
    pub confidence: f32,
    pub reason: String,
}

/// Informe completo de telemetría y calibración para el diagnóstico de la UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryReport {
    pub certainty: DataCertainty,
    pub loop_mode: ControlLoopMode,
    pub confidence: f32,
    pub estimated_latency_ms: u64,
    pub pos_error_m: Option<f32>,
    pub rot_error_deg: Option<f32>,
    pub active_pos_correction: [f32; 3],
    pub active_rot_correction: [f32; 3],
    pub visual_framing_offset_deg: [f32; 2],
    pub log: Vec<CalibrationLogEntry>,
}

impl Default for TelemetryReport {
    fn default() -> Self {
        TelemetryReport {
            certainty: DataCertainty::Unknown {
                reason: "Esperando conexión y apertura de cámara en VRChat".into(),
            },
            loop_mode: ControlLoopMode::OpenLoop {
                reason: "Sin lecturas recientes de /usercamera/Pose".into(),
            },
            confidence: 0.0,
            estimated_latency_ms: DEFAULT_ECHO_LATENCY_MS,
            pos_error_m: None,
            rot_error_deg: None,
            active_pos_correction: [0.0; 3],
            active_rot_correction: [0.0; 3],
            visual_framing_offset_deg: [0.0; 2],
            log: Vec::new(),
        }
    }
}

/// Calcula los offsets angulares `[yaw_deg, pitch_deg]` de Calibración Visual geométrica
/// a partir de la focal activa `zoom_mm` (20–300 mm), composición de tercios, headroom y lead room.
pub fn compute_visual_framing_angles(
    cfg: &CalibrationConfig,
    zoom_mm: f32,
    lateral_velocity_mps: f32,
) -> [f32; 2] {
    if !cfg.enabled {
        return [0.0, 0.0];
    }
    if cfg.composition_x.abs() < 1e-5 && cfg.composition_y.abs() < 1e-5 && cfg.lead_room.abs() < 1e-5 {
        return [0.0, 0.0];
    }
    let safe_zoom = if zoom_mm.is_finite() { zoom_mm } else { 45.0 };
    let safe_lat_vel = if lateral_velocity_mps.is_finite() { lateral_velocity_mps } else { 0.0 };
    let f_mm = safe_zoom.clamp(20.0, 300.0);
    // Según M0 (`docs/osc-findings.md` §5): sensor vertical efectivo = 24 mm (mitad = 12 mm), 16:9 horizontal mitad = 21.333 mm
    let tan_half_v = 12.0 / f_mm;
    let tan_half_h = 21.333 / f_mm;

    let comp_x = if cfg.composition_x.is_finite() { cfg.composition_x.clamp(-1.0, 1.0) } else { 0.0 };
    let comp_y = if cfg.composition_y.is_finite() { cfg.composition_y.clamp(-1.0, 1.0) } else { 0.0 };
    let lead_cfg = if cfg.lead_room.is_finite() { cfg.lead_room.clamp(0.0, 1.0) } else { 0.0 };
    let lead = lead_cfg * (safe_lat_vel / 1.5).clamp(-0.5, 0.5);

    // Para colocar al sujeto en `comp_x` dentro del encuadre, rotamos la cámara en sentido opuesto
    let yaw_offset = -((comp_x - lead) * tan_half_h).atan().to_degrees();
    // En convención VRChat/Unity, pitch positivo mira hacia abajo (sube al sujeto hacia la parte superior del cuadro)
    let pitch_offset = (comp_y * tan_half_v).atan().to_degrees();

    [yaw_offset.clamp(-25.0, 25.0), pitch_offset.clamp(-25.0, 25.0)]
}

/// Aplica de forma pura y no destructiva los ajustes técnicos, visuales y adaptativos sobre una `Pose` deseada.
pub fn apply_calibration_to_pose(
    desired: Pose,
    cfg: &CalibrationConfig,
    zoom_mm: f32,
    lateral_velocity_mps: f32,
    adaptive_pos_corr: [f32; 3],
    adaptive_rot_corr: [f32; 3],
) -> Pose {
    if !cfg.enabled {
        return desired;
    }
    let [vis_yaw, vis_pitch] = compute_visual_framing_angles(cfg, zoom_mm, lateral_velocity_mps);
    let safe = |v: f32, limit: f32| if v.is_finite() { v.clamp(-limit, limit) } else { 0.0 };

    let dx = safe(cfg.pos_offset[0], 2.0) + safe(adaptive_pos_corr[0], MAX_POS_CORRECTION_M);
    let dy = safe(cfg.pos_offset[1], 2.0) + safe(adaptive_pos_corr[1], MAX_POS_CORRECTION_M);
    let dz = safe(cfg.pos_offset[2], 2.0) + safe(adaptive_pos_corr[2], MAX_POS_CORRECTION_M);

    let d_pitch = safe(cfg.pitch_offset_deg, 25.0)
        + vis_pitch
        + safe(adaptive_rot_corr[0], MAX_ROT_CORRECTION_DEG);
    let d_yaw = safe(cfg.yaw_offset_deg, 180.0)
        + vis_yaw
        + safe(adaptive_rot_corr[1], MAX_ROT_CORRECTION_DEG);
    let d_roll = safe(adaptive_rot_corr[2], MAX_ROT_CORRECTION_DEG);

    if dx.abs() < 1e-6
        && dy.abs() < 1e-6
        && dz.abs() < 1e-6
        && d_pitch.abs() < 1e-5
        && d_yaw.abs() < 1e-5
        && d_roll.abs() < 1e-5
    {
        return desired;
    }

    Pose {
        pos: [desired.pos[0] + dx, desired.pos[1] + dy, desired.pos[2] + dz],
        rot: [
            (desired.rot[0] + d_pitch).clamp(-89.9, 89.9),
            wrap180(desired.rot[1] + d_yaw),
            wrap180(desired.rot[2] + d_roll),
        ],
    }
}

/// Motor en memoria de telemetría, línea de retardo y corrección adaptativa acotada.
pub struct CalibrationRuntime {
    /// Línea de retardo `(timestamp_ms, pose_enviada)` para comparar contra ecos con latencia.
    history: VecDeque<(u64, Pose)>,
    pub active_pos_correction: [f32; 3],
    pub active_rot_correction: [f32; 3],
    prev_error_sign: [i8; 3],
    pub estimated_latency_ms: u64,
    last_processed_seq: u64,
    pub log: Vec<CalibrationLogEntry>,
}

impl Default for CalibrationRuntime {
    fn default() -> Self {
        CalibrationRuntime {
            history: VecDeque::with_capacity(128),
            active_pos_correction: [0.0; 3],
            active_rot_correction: [0.0; 3],
            prev_error_sign: [0; 3],
            estimated_latency_ms: DEFAULT_ECHO_LATENCY_MS,
            last_processed_seq: 0,
            log: Vec::new(),
        }
    }
}

impl CalibrationRuntime {
    pub fn record_sent_pose(&mut self, timestamp_ms: u64, pose: Pose) {
        if self.history.len() >= 120 {
            self.history.pop_front();
        }
        self.history.push_back((timestamp_ms, pose));
    }

    /// Restaura a cero todas las correcciones dinámicas y registra la causa.
    pub fn reset(&mut self, timestamp_ms: u64, reason: &str) {
        self.active_pos_correction = [0.0; 3];
        self.active_rot_correction = [0.0; 3];
        self.prev_error_sign = [0; 3];
        self.history.clear();
        self.push_log(CalibrationLogEntry {
            timestamp_ms,
            category: "reset".into(),
            delta_pos: [0.0; 3],
            delta_rot: [0.0; 3],
            confidence: 1.0,
            reason: reason.into(),
        });
    }

    fn push_log(&mut self, entry: CalibrationLogEntry) {
        if self.log.len() >= 40 {
            self.log.remove(0);
        }
        self.log.push(entry);
    }

    /// Busca la Pose enviada en `target_ms = now_ms - estimated_latency_ms` interpolando en el buffer histórico.
    fn expected_pose_at(&self, target_ms: u64) -> Option<Pose> {
        if self.history.is_empty() {
            return None;
        }
        if target_ms <= self.history.front()?.0 {
            return Some(self.history.front()?.1);
        }
        if target_ms >= self.history.back()?.0 {
            return Some(self.history.back()?.1);
        }
        for i in 0..self.history.len().saturating_sub(1) {
            let (t0, p0) = self.history[i];
            let (t1, p1) = self.history[i + 1];
            if target_ms >= t0 && target_ms <= t1 {
                let span = (t1 - t0).max(1) as f32;
                let u = ((target_ms - t0) as f32 / span).clamp(0.0, 1.0);
                let pos = lerp3(p0.pos, p1.pos, u);
                let rot = Quat::slerp(Quat::from_euler_deg(p0.rot), Quat::from_euler_deg(p1.rot), u).to_euler_deg();
                return Some(Pose { pos, rot });
            }
        }
        self.history.back().map(|(_, p)| *p)
    }

    /// Evalúa la realimentación recibida de VRChat, clasifica la certidumbre y actualiza la corrección acotada.
    pub fn observe_and_update(
        &mut self,
        snap: Option<&CameraSnapshot>,
        now_ms: u64,
        dt: f32,
        cfg: &CalibrationConfig,
        zoom_mm: f32,
        lateral_vel_mps: f32,
    ) -> TelemetryReport {
        self.observe_and_update_with_anchor(snap, None, now_ms, dt, cfg, zoom_mm, lateral_vel_mps)
    }

    /// Evalúa la realimentación coordinando con el estado del `AnchorManager` para evitar deriva acumulativa
    /// durante cambios de modo de cámara o pérdidas temporales de sincronización (`SyncLost` / `Recovering`).
    pub fn observe_and_update_with_anchor(
        &mut self,
        snap: Option<&CameraSnapshot>,
        anchor_state: Option<AnchorLifecycleState>,
        now_ms: u64,
        dt: f32,
        cfg: &CalibrationConfig,
        zoom_mm: f32,
        lateral_vel_mps: f32,
    ) -> TelemetryReport {
        let vis_angles = compute_visual_framing_angles(cfg, zoom_mm, lateral_vel_mps);

        if matches!(
            anchor_state,
            Some(
                AnchorLifecycleState::SyncLost
                    | AnchorLifecycleState::Recovering
                    | AnchorLifecycleState::Acquiring
            )
        ) {
            self.prev_error_sign = [0; 3];
            return TelemetryReport {
                certainty: DataCertainty::Estimated {
                    confidence: 0.5,
                    reason: "Ancla en transición o resincronización temporal".into(),
                },
                loop_mode: ControlLoopMode::OpenLoop {
                    reason: "Corrección adaptativa en pausa durante sincronización/cambio de modo del ancla".into(),
                },
                confidence: 0.5,
                estimated_latency_ms: self.estimated_latency_ms,
                pos_error_m: None,
                rot_error_deg: None,
                active_pos_correction: self.active_pos_correction,
                active_rot_correction: self.active_rot_correction,
                visual_framing_offset_deg: vis_angles,
                log: self.log.clone(),
            };
        }

        let Some(cam) = snap else {
            return TelemetryReport {
                certainty: DataCertainty::Unknown {
                    reason: "Sin conexión activa con VRChat".into(),
                },
                loop_mode: ControlLoopMode::OpenLoop {
                    reason: "Desconectado de VRChat: operando en modo de vista previa matemática".into(),
                },
                confidence: 0.0,
                estimated_latency_ms: self.estimated_latency_ms,
                pos_error_m: None,
                rot_error_deg: None,
                active_pos_correction: self.active_pos_correction,
                active_rot_correction: self.active_rot_correction,
                visual_framing_offset_deg: vis_angles,
                log: self.log.clone(),
            };
        };

        if !cam.rx_available {
            return TelemetryReport {
                certainty: DataCertainty::Unknown {
                    reason: "Puerto UDP 9001 ocupado por otra aplicación OSC".into(),
                },
                loop_mode: ControlLoopMode::OpenLoop {
                    reason: "Modo Control Abierto: se envían comandos a VRChat sin recibir ecos en UDP 9001".into(),
                },
                confidence: 0.0,
                estimated_latency_ms: self.estimated_latency_ms,
                pos_error_m: None,
                rot_error_deg: None,
                active_pos_correction: self.active_pos_correction,
                active_rot_correction: self.active_rot_correction,
                visual_framing_offset_deg: vis_angles,
                log: self.log.clone(),
            };
        }

        if !cam.open {
            return TelemetryReport {
                certainty: DataCertainty::Unknown {
                    reason: "Cámara cerrada en VRChat".into(),
                },
                loop_mode: ControlLoopMode::OpenLoop {
                    reason: "Cámara cerrada: VRChat no procesa ni reporta /usercamera/Pose".into(),
                },
                confidence: 0.0,
                estimated_latency_ms: self.estimated_latency_ms,
                pos_error_m: None,
                rot_error_deg: None,
                active_pos_correction: self.active_pos_correction,
                active_rot_correction: self.active_rot_correction,
                visual_framing_offset_deg: vis_angles,
                log: self.log.clone(),
            };
        }

        let (Some(observed_pose), true) = (cam.pose, cam.pose_fresh) else {
            return TelemetryReport {
                certainty: DataCertainty::Unknown {
                    reason: "Cámara abierta, pero VRChat aún no ha emitido el primer paquete Pose".into(),
                },
                loop_mode: ControlLoopMode::OpenLoop {
                    reason: "Sin lectura inicial de Pose: mueve ligeramente la cámara en VRChat".into(),
                },
                confidence: 0.2,
                estimated_latency_ms: self.estimated_latency_ms,
                pos_error_m: None,
                rot_error_deg: None,
                active_pos_correction: self.active_pos_correction,
                active_rot_correction: self.active_rot_correction,
                visual_framing_offset_deg: vis_angles,
                log: self.log.clone(),
            };
        };

        let age_ms = now_ms.saturating_sub(cam.pose_epoch_ms.unwrap_or(now_ms));
        let (certainty, base_confidence) = if age_ms <= 500 {
            (
                DataCertainty::Measured {
                    seq: cam.pose_seq,
                    age_ms,
                },
                1.0_f32,
            )
        } else {
            // VRChat solo emite Pose cuando cambia: si está en reposo, la pose sigue siendo válida como estimación
            let decay = (1.0 - (age_ms as f32 - 500.0) / 10_000.0).clamp(0.55, 0.90);
            (
                DataCertainty::Estimated {
                    confidence: decay,
                    reason: format!("Cámara estacionaria (último eco hace {:.1} s)", age_ms as f32 / 1000.0),
                },
                decay,
            )
        };

        let delayed_target_ms = now_ms.saturating_sub(self.estimated_latency_ms);
        let Some(expected_pose) = self.expected_pose_at(delayed_target_ms) else {
            return TelemetryReport {
                certainty,
                loop_mode: ControlLoopMode::OpenLoop {
                    reason: "En espera de trayectoria activa para comparar estado deseado vs observado".into(),
                },
                confidence: base_confidence,
                estimated_latency_ms: self.estimated_latency_ms,
                pos_error_m: None,
                rot_error_deg: None,
                active_pos_correction: self.active_pos_correction,
                active_rot_correction: self.active_rot_correction,
                visual_framing_offset_deg: vis_angles,
                log: self.log.clone(),
            };
        };

        let err_vec = [
            expected_pose.pos[0] - observed_pose.pos[0],
            expected_pose.pos[1] - observed_pose.pos[1],
            expected_pose.pos[2] - observed_pose.pos[2],
        ];
        let pos_err = dist3(expected_pose.pos, observed_pose.pos);
        let rot_err = Quat::from_euler_deg(expected_pose.rot)
            .angle_deg(&Quat::from_euler_deg(observed_pose.rot));

        // Detector de intervención externa o Anchor bloqueado en World (> 50 cm de discrepancia)
        if pos_err > ANOMALY_POS_THRESHOLD_M {
            if cam.pose_seq != self.last_processed_seq {
                self.last_processed_seq = cam.pose_seq;
                self.push_log(CalibrationLogEntry {
                    timestamp_ms: now_ms,
                    category: "anomaly".into(),
                    delta_pos: [0.0; 3],
                    delta_rot: [0.0; 3],
                    confidence: 0.15,
                    reason: format!(
                        "Discrepancia alta ({pos_err:.2} m): corrección pausada (¿cámara movida a mano o Anchor en World?)"
                    ),
                });
            }
            return TelemetryReport {
                certainty: DataCertainty::Estimated {
                    confidence: 0.15,
                    reason: "Discrepancia externa detectada (> 0.50 m)".into(),
                },
                loop_mode: ControlLoopMode::OpenLoop {
                    reason: "Corrección congelada por discrepancia > 0.50 m (verifica que el Anchor de VRChat no esté en World)".into(),
                },
                confidence: 0.15,
                estimated_latency_ms: self.estimated_latency_ms,
                pos_error_m: Some(pos_err),
                rot_error_deg: Some(rot_err),
                active_pos_correction: self.active_pos_correction,
                active_rot_correction: self.active_rot_correction,
                visual_framing_offset_deg: vis_angles,
                log: self.log.clone(),
            };
        }

        // Aplicar corrección acotada únicamente cuando:
        // 1. La calibración y auto_correct están activos
        // 2. Llegó una medición real nueva (`Measured`)
        // 3. Supera la banda muerta (5 mm / 0.25°)
        if cfg.enabled
            && cfg.auto_correct
            && matches!(certainty, DataCertainty::Measured { .. })
            && cam.pose_seq != self.last_processed_seq
        {
            self.last_processed_seq = cam.pose_seq;
            let safe_dt = dt.clamp(0.005, 0.1);
            let mut applied_any = false;
            let mut step_pos = [0.0_f32; 3];
            let mut step_rot = [0.0_f32; 3];

            if pos_err > POS_DEADBAND_M {
                // Ganancia proporcional conservadora con amortiguación anti-oscilación si hay inversión de signo
                for i in 0..3 {
                    let sign = if err_vec[i] > 1e-4 { 1 } else if err_vec[i] < -1e-4 { -1 } else { 0 };
                    let flip_damp = if self.prev_error_sign[i] != 0 && sign != 0 && self.prev_error_sign[i] != sign {
                        0.35
                    } else {
                        1.0
                    };
                    if sign != 0 {
                        self.prev_error_sign[i] = sign;
                    }
                    step_pos[i] = err_vec[i] * 0.12 * flip_damp;
                }
                step_pos = clamp_vec3_len(step_pos, MAX_POS_SLEW_MPS * safe_dt);
                for (corr, &step) in self.active_pos_correction.iter_mut().zip(&step_pos) {
                    *corr = (*corr + step).clamp(-MAX_POS_CORRECTION_M, MAX_POS_CORRECTION_M);
                }
                applied_any = true;
            }

            if rot_err > ROT_DEADBAND_DEG {
                for (i, slot) in step_rot.iter_mut().enumerate() {
                    let diff = angle_diff_deg(observed_pose.rot[i], expected_pose.rot[i]);
                    let max_step = MAX_ROT_SLEW_DPS * safe_dt;
                    let delta = (diff * 0.10).clamp(-max_step, max_step);
                    *slot = delta;
                    self.active_rot_correction[i] = (self.active_rot_correction[i] + delta)
                        .clamp(-MAX_ROT_CORRECTION_DEG, MAX_ROT_CORRECTION_DEG);
                }
                applied_any = true;
            }

            if applied_any && (norm3(step_pos) > 0.001 || norm3(step_rot) > 0.05) {
                self.push_log(CalibrationLogEntry {
                    timestamp_ms: now_ms,
                    category: "adaptive".into(),
                    delta_pos: self.active_pos_correction,
                    delta_rot: self.active_rot_correction,
                    confidence: base_confidence,
                    reason: format!(
                        "Ajuste suave por residuo medido ({:.1} mm, {:.2}°)",
                        pos_err * 1000.0,
                        rot_err
                    ),
                });
            }
        }

        let loop_mode = if cfg.enabled && cfg.auto_correct && base_confidence >= 0.65 {
            ControlLoopMode::ClosedLoop
        } else if !cfg.enabled {
            ControlLoopMode::OpenLoop {
                reason: "Calibración desactivada por el usuario (motor en directo)".into(),
            }
        } else {
            ControlLoopMode::OpenLoop {
                reason: "Offsets técnicos/visuales activos; auto-corrección en espera".into(),
            }
        };

        TelemetryReport {
            certainty,
            loop_mode,
            confidence: base_confidence,
            estimated_latency_ms: self.estimated_latency_ms,
            pos_error_m: Some(pos_err),
            rot_error_deg: Some(rot_err),
            active_pos_correction: self.active_pos_correction,
            active_rot_correction: self.active_rot_correction,
            visual_framing_offset_deg: vis_angles,
            log: self.log.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_cfg() -> CalibrationConfig {
        CalibrationConfig {
            enabled: true,
            auto_correct: true,
            pos_offset: [0.0, 0.0, 0.0],
            yaw_offset_deg: 0.0,
            pitch_offset_deg: 0.0,
            composition_x: 0.0,
            composition_y: 0.0,
            lead_room: 0.0,
        }
    }

    #[test]
    fn disabled_calibration_is_strictly_identity() {
        let mut cfg = active_cfg();
        cfg.enabled = false;
        cfg.pos_offset = [1.0, -0.5, 0.8];
        cfg.yaw_offset_deg = 45.0;
        let orig = Pose { pos: [1.23, 1.65, -2.4], rot: [-5.0, 120.0, 3.0] };
        let out = apply_calibration_to_pose(orig, &cfg, 50.0, 1.0, [0.1, 0.1, 0.1], [2.0, 2.0, 2.0]);
        assert_eq!(out, orig);
    }

    #[test]
    fn open_loop_reported_when_no_feedback_or_camera_closed() {
        let mut rt = CalibrationRuntime::default();
        let cfg = active_cfg();

        let rep_none = rt.observe_and_update(None, 1000, 0.016, &cfg, 45.0, 0.0);
        assert!(matches!(rep_none.certainty, DataCertainty::Unknown { .. }));
        assert!(matches!(rep_none.loop_mode, ControlLoopMode::OpenLoop { .. }));

        let closed_snap = CameraSnapshot {
            open: false,
            zoom: Some(45.0),
            ..Default::default()
        };
        let rep_closed = rt.observe_and_update(Some(&closed_snap), 1000, 0.016, &cfg, 45.0, 0.0);
        assert!(matches!(rep_closed.certainty, DataCertainty::Unknown { .. }));
        assert_eq!(rt.active_pos_correction, [0.0; 3]);
    }

    #[test]
    fn bounded_corrector_converges_without_oscillation_and_respects_caps_and_reset() {
        let mut rt = CalibrationRuntime::default();
        let cfg = active_cfg();
        let desired = Pose { pos: [0.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] };

        // Simulamos un offset sistemático en VRChat de -0.04 m en X con 85 ms de latencia
        for step in 0..40 {
            let now_ms = 1000 + step * 16;
            let sent = apply_calibration_to_pose(
                desired,
                &cfg,
                45.0,
                0.0,
                rt.active_pos_correction,
                rt.active_rot_correction,
            );
            rt.record_sent_pose(now_ms, sent);

            let observed = Pose {
                pos: [sent.pos[0] - 0.04, sent.pos[1], sent.pos[2]],
                rot: sent.rot,
            };
            let snap = CameraSnapshot {
                open: true,
                pose: Some(observed),
                pose_fresh: true,
                pose_seq: step + 1,
                pose_epoch_ms: Some(now_ms),
                zoom: Some(45.0),
                ..Default::default()
            };
            let rep = rt.observe_and_update(Some(&snap), now_ms, 0.016, &cfg, 45.0, 0.0);
            assert!(matches!(rep.certainty, DataCertainty::Measured { .. }));
            // Debe mantenerse siempre acotada dentro del límite máximo (0.15 m) y positiva (sin sobreimpulso negativo)
            assert!(rt.active_pos_correction[0] >= 0.0 && rt.active_pos_correction[0] <= MAX_POS_CORRECTION_M);
        }
        assert!(rt.active_pos_correction[0] > 0.01, "debe haber corregido parte del error sistemático");

        // Restaurar calibración debe devolver todo exactamente a cero
        rt.reset(2000, "Restauración manual por el usuario");
        assert_eq!(rt.active_pos_correction, [0.0; 3]);
        assert_eq!(rt.active_rot_correction, [0.0; 3]);
    }

    #[test]
    fn freezes_correction_on_external_manual_grab_anomaly() {
        let mut rt = CalibrationRuntime::default();
        let cfg = active_cfg();
        let desired = Pose { pos: [0.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] };
        rt.record_sent_pose(1000, desired);

        // El usuario agarró la cámara en VR y la movió a 3 metros de distancia
        let grabbed_snap = CameraSnapshot {
            open: true,
            pose: Some(Pose { pos: [3.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] }),
            pose_fresh: true,
            pose_seq: 1,
            pose_epoch_ms: Some(1085),
            zoom: Some(45.0),
            ..Default::default()
        };
        let rep = rt.observe_and_update(Some(&grabbed_snap), 1085, 0.016, &cfg, 45.0, 0.0);
        assert!(matches!(rep.loop_mode, ControlLoopMode::OpenLoop { .. }));
        assert_eq!(rt.active_pos_correction, [0.0; 3], "no debe aplicar corrección ante anomalía externa");
    }

    #[test]
    fn freezes_adaptive_correction_during_anchor_sync_loss_and_recovery() {
        let mut rt = CalibrationRuntime::default();
        let cfg = active_cfg();
        let desired = Pose { pos: [0.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] };
        rt.record_sent_pose(1000, desired);

        let snap = CameraSnapshot {
            open: true,
            mode: 2,
            pose: Some(Pose { pos: [0.08, 1.6, 2.0], rot: [0.0, 180.0, 0.0] }),
            pose_fresh: true,
            pose_seq: 1,
            pose_epoch_ms: Some(1085),
            zoom: Some(45.0),
            ..Default::default()
        };

        // Mientras el ancla está en SyncLost o Recovering, no debe acumular deriva en active_pos_correction
        let rep_lost = rt.observe_and_update_with_anchor(
            Some(&snap),
            Some(AnchorLifecycleState::SyncLost),
            1085,
            0.016,
            &cfg,
            45.0,
            0.0,
        );
        assert!(matches!(rep_lost.loop_mode, ControlLoopMode::OpenLoop { .. }));
        assert_eq!(rt.active_pos_correction, [0.0; 3]);

        let rep_rec = rt.observe_and_update_with_anchor(
            Some(&snap),
            Some(AnchorLifecycleState::Recovering),
            1100,
            0.016,
            &cfg,
            45.0,
            0.0,
        );
        assert!(matches!(rep_rec.loop_mode, ControlLoopMode::OpenLoop { .. }));
        assert_eq!(rt.active_pos_correction, [0.0; 3]);
    }
}
