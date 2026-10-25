//! Servicio de integración principal y máquina de estados (FSM) para el asset de avatar de VRChat.

use super::config::AvatarIntegrationConfig;
use super::protocol::*;
use super::rig_adapter::{AvatarPathAdapter, AvatarPathRigSnapshot};
use super::validator::ParameterValidator;
use crate::anchor::Anchor;
use crate::camera::CameraSnapshot;
use crate::studio::model::{
    Keyframe, KnotMode, LookTrackPoint, ReferenceSpace, SequenceProject, Transition,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub enum AvatarExecutionRequest {
    StartPlayback { live_osc: bool },
    StopPlayback,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarIntegrationSnapshot {
    pub protocol_version: String,
    pub state: AvatarIntegrationState,
    pub last_action: AvatarAction,
    pub action_armed: bool,
    pub current_avatar_id: Option<String>,
    pub duration_seconds: f32,
    pub duration_normalized: f32,
    pub anchor_mode: AvatarAnchorMode,
    pub path_type: AvatarPathType,
    pub looping: bool,
    pub asset_visible: bool,
    pub has_point_a: bool,
    pub has_point_b: bool,
    pub last_error: Option<String>,
    pub rig_snapshot: AvatarPathRigSnapshot,
}

pub struct IrisIntegrationService {
    pub config: AvatarIntegrationConfig,
    pub validator: ParameterValidator,
    pub rig_adapter: AvatarPathAdapter,
    pub state: AvatarIntegrationState,
    pub project: SequenceProject,
    pub current_avatar_id: Option<String>,
    pub anchor_mode: AvatarAnchorMode,
    pub path_type: AvatarPathType,
    pub looping: bool,
    pub asset_visible: bool,
    pub last_error: Option<String>,
}

impl IrisIntegrationService {
    pub fn new(config: AvatarIntegrationConfig) -> Self {
        let rig_adapter = AvatarPathAdapter::new(config.clone());
        let duration = config.min_duration + 0.5 * (config.max_duration - config.min_duration);
        let project = SequenceProject::new("avatar-sequence", "Avatar Trajectory", duration);

        IrisIntegrationService {
            config,
            validator: ParameterValidator::new(),
            rig_adapter,
            state: AvatarIntegrationState::Idle,
            project,
            current_avatar_id: None,
            anchor_mode: AvatarAnchorMode::Auto,
            path_type: AvatarPathType::Bezier,
            looping: false,
            asset_visible: true,
            last_error: None,
        }
    }

    /// Actualiza la configuración interna y los submódulos.
    pub fn update_config(&mut self, config: AvatarIntegrationConfig) {
        self.rig_adapter.update_config(config.clone());
        self.config = config;
    }

    /// Comprueba si existen los puntos clave A y B.
    pub fn has_point_a(&self) -> bool {
        self.project.keyframes.iter().any(|k| k.time <= 0.05)
    }

    pub fn has_point_b(&self) -> bool {
        self.project.keyframes.iter().any(|k| k.time >= self.project.duration - 0.05)
    }

    /// Actualiza el estado FSM según la presencia de puntos A y B.
    fn refresh_ready_state(&mut self) {
        if matches!(self.state, AvatarIntegrationState::Previewing | AvatarIntegrationState::Playing) {
            return;
        }
        if self.has_point_a() && self.has_point_b() {
            self.state = AvatarIntegrationState::Ready;
            self.last_error = None;
        } else if self.has_point_a() || self.has_point_b() {
            self.state = AvatarIntegrationState::Configuring;
            self.last_error = None;
        } else {
            self.state = AvatarIntegrationState::Idle;
            self.last_error = None;
        }
    }

    /// Maneja el parámetro `Iris_Duration` (normalizado en `[0.0, 1.0]`).
    pub fn handle_duration(&mut self, norm: f32) -> Result<(), String> {
        let sec = normalized_to_duration(norm, self.config.min_duration, self.config.max_duration);
        let sec = (sec * 10.0).round() / 10.0;
        self.project.duration = sec;

        // Si existe el punto B, actualizar su timestamp a la nueva duración
        if let Some(pos) = self.project.keyframes.iter().position(|k| k.label == "Punto B") {
            self.project.keyframes[pos].time = sec;
        }

        self.refresh_ready_state();
        Ok(())
    }

    /// Maneja el parámetro `Iris_AnchorMode`.
    pub fn handle_anchor_mode(&mut self, mode: AvatarAnchorMode) -> Result<(), String> {
        self.anchor_mode = mode;
        match mode {
            AvatarAnchorMode::Auto | AvatarAnchorMode::Player => {
                self.project.space = ReferenceSpace::Anchor;
            }
            AvatarAnchorMode::Manual => {
                self.project.space = ReferenceSpace::World;
            }
        }
        Ok(())
    }

    /// Maneja el parámetro `Iris_PathType`.
    pub fn handle_path_type(&mut self, pt: AvatarPathType) -> Result<(), String> {
        self.path_type = pt;
        let transition = match pt {
            AvatarPathType::Linear => Transition::Linear,
            AvatarPathType::Bezier | AvatarPathType::Orbital => Transition::Smooth,
        };
        for k in self.project.keyframes.iter_mut() {
            k.transition = transition;
        }
        Ok(())
    }

    /// Maneja el parámetro `Iris_Loop`.
    pub fn handle_loop(&mut self, b: bool) -> Result<(), String> {
        self.looping = b;
        Ok(())
    }

    /// Maneja el parámetro `Iris_AssetVisible`.
    pub fn handle_asset_visibility(&mut self, b: bool) -> Result<(), String> {
        self.asset_visible = b;
        Ok(())
    }

    /// Maneja la detección de cambio de avatar (`/avatar/change`).
    pub fn handle_avatar_change(&mut self, avatar_id: String) -> Result<(), String> {
        self.current_avatar_id = Some(avatar_id);
        self.last_error = None;
        self.validator.force_rearm();
        Ok(())
    }

    /// Despacha una orden directa sin contexto de cámara (p. ej. Detener o Limpiar).
    pub fn handle_action(&mut self, action: AvatarAction) -> Result<(), String> {
        self.handle_action_with_camera(action, None, None).map(|_| ())
    }

    /// Ejecuta una acción de control con la información de cámara y anclaje disponible.
    pub fn handle_action_with_camera(
        &mut self,
        action: AvatarAction,
        cam_snap: Option<&CameraSnapshot>,
        anchor: Option<&Anchor>,
    ) -> Result<Option<AvatarExecutionRequest>, String> {
        match action {
            AvatarAction::Inactive => {
                self.validator.force_rearm();
                Ok(None)
            }
            AvatarAction::CaptureA => {
                let snap = cam_snap.ok_or("No hay datos de cámara disponibles para capturar el Punto A")?;
                if !snap.open {
                    self.state = AvatarIntegrationState::Error;
                    let err = "La cámara de VRChat está cerrada al intentar capturar el Punto A".to_string();
                    self.last_error = Some(err.clone());
                    return Err(err);
                }
                let raw_pose = snap.pose.ok_or_else(|| {
                    let err = "Aún no se recibe la pose de la cámara de VRChat para el Punto A".to_string();
                    self.state = AvatarIntegrationState::Error;
                    self.last_error = Some(err.clone());
                    err
                })?;
                let local_pose = anchor.map(|a| a.world_to_local(&raw_pose));

                let trans = match self.path_type {
                    AvatarPathType::Linear => Transition::Linear,
                    _ => Transition::Smooth,
                };

                let kf_a = Keyframe {
                    id: "avatar-kf-a".into(),
                    label: "Punto A".into(),
                    time: 0.0,
                    raw_pose,
                    local_pose,
                    captured_at_ms: snap.pose_epoch_ms.unwrap_or(0),
                    transition: trans,
                    custom_curve: None,
                    knot_mode: KnotMode::Stop,
                    look: LookTrackPoint {
                        zoom: snap.zoom,
                        exposure: snap.exposure,
                        focal_distance: snap.focal_distance,
                        aperture: snap.aperture,
                    },
                };

                // Reemplazar o insertar Punto A
                self.project.keyframes.retain(|k| k.label != "Punto A");
                self.project.keyframes.insert(0, kf_a);
                self.project.anchor_snapshot = anchor.cloned();

                self.refresh_ready_state();
                Ok(None)
            }
            AvatarAction::CaptureB => {
                let snap = cam_snap.ok_or("No hay datos de cámara disponibles para capturar el Punto B")?;
                if !snap.open {
                    self.state = AvatarIntegrationState::Error;
                    let err = "La cámara de VRChat está cerrada al intentar capturar el Punto B".to_string();
                    self.last_error = Some(err.clone());
                    return Err(err);
                }
                let raw_pose = snap.pose.ok_or_else(|| {
                    let err = "Aún no se recibe la pose de la cámara de VRChat para el Punto B".to_string();
                    self.state = AvatarIntegrationState::Error;
                    self.last_error = Some(err.clone());
                    err
                })?;
                let local_pose = anchor.map(|a| a.world_to_local(&raw_pose));

                let trans = match self.path_type {
                    AvatarPathType::Linear => Transition::Linear,
                    _ => Transition::Smooth,
                };

                let kf_b = Keyframe {
                    id: "avatar-kf-b".into(),
                    label: "Punto B".into(),
                    time: self.project.duration,
                    raw_pose,
                    local_pose,
                    captured_at_ms: snap.pose_epoch_ms.unwrap_or(0),
                    transition: trans,
                    custom_curve: None,
                    knot_mode: KnotMode::Stop,
                    look: LookTrackPoint {
                        zoom: snap.zoom,
                        exposure: snap.exposure,
                        focal_distance: snap.focal_distance,
                        aperture: snap.aperture,
                    },
                };

                // Reemplazar o insertar Punto B
                self.project.keyframes.retain(|k| k.label != "Punto B");
                self.project.keyframes.push(kf_b);
                self.project.keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
                self.project.anchor_snapshot = anchor.cloned();

                self.refresh_ready_state();
                Ok(None)
            }
            AvatarAction::Preview => {
                if !self.has_point_a() || !self.has_point_b() {
                    self.state = AvatarIntegrationState::Error;
                    let err = "No se puede previsualizar: la trayectoria requiere capturar Puntos A y B".to_string();
                    self.last_error = Some(err.clone());
                    return Err(err);
                }
                self.state = AvatarIntegrationState::Previewing;
                self.last_error = None;
                Ok(Some(AvatarExecutionRequest::StartPlayback { live_osc: false }))
            }
            AvatarAction::Execute => {
                if !self.has_point_a() || !self.has_point_b() {
                    self.state = AvatarIntegrationState::Error;
                    let err = "No se puede ejecutar en VRChat: la trayectoria requiere capturar Puntos A y B".to_string();
                    self.last_error = Some(err.clone());
                    return Err(err);
                }
                self.state = AvatarIntegrationState::Playing;
                self.last_error = None;
                Ok(Some(AvatarExecutionRequest::StartPlayback { live_osc: true }))
            }
            AvatarAction::Stop => {
                self.state = AvatarIntegrationState::Stopped;
                self.last_error = None;
                Ok(Some(AvatarExecutionRequest::StopPlayback))
            }
            AvatarAction::Clear => {
                self.project.keyframes.clear();
                self.state = AvatarIntegrationState::Idle;
                self.last_error = None;
                Ok(Some(AvatarExecutionRequest::StopPlayback))
            }
        }
    }

    /// Notifica al servicio que la reproducción ha finalizado (naturalmente o por error).
    pub fn notify_playback_finished(&mut self, reached_end: bool, error: Option<String>) {
        if let Some(err) = error {
            self.state = AvatarIntegrationState::Error;
            self.last_error = Some(err);
        } else if reached_end && self.looping {
            self.state = AvatarIntegrationState::Playing;
        } else {
            self.state = AvatarIntegrationState::Stopped;
        }
    }

    /// Genera la instantánea completa para la interfaz y diagnóstico.
    pub fn snapshot(&self, current_time: f32, anchor: Option<&Anchor>) -> AvatarIntegrationSnapshot {
        let rig_snap = self.rig_adapter.compute_rig_samples(&self.project, anchor, current_time);
        let norm_d = duration_to_normalized(self.project.duration, self.config.min_duration, self.config.max_duration);

        AvatarIntegrationSnapshot {
            protocol_version: PROTOCOL_VERSION.into(),
            state: self.state,
            last_action: self.validator.last_action,
            action_armed: self.validator.action_armed,
            current_avatar_id: self.current_avatar_id.clone(),
            duration_seconds: self.project.duration,
            duration_normalized: norm_d,
            anchor_mode: self.anchor_mode,
            path_type: self.path_type,
            looping: self.looping,
            asset_visible: self.asset_visible,
            has_point_a: self.has_point_a(),
            has_point_b: self.has_point_b(),
            last_error: self.last_error.clone(),
            rig_snapshot: rig_snap,
        }
    }
}
