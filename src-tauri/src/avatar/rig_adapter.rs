//! Adaptador para el rig del avatar: muestreo por longitud de arco de la cadena de huesos,
//! cálculo de tangentes para flechas direccionales y desacoplamiento de la orientación de cámara.

use super::config::AvatarIntegrationConfig;
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use crate::studio::evaluator::{evaluate, sample_base_pose_at};
use crate::studio::math3d::{dist3, norm3};
use crate::studio::model::SequenceProject;
use serde::{Deserialize, Serialize};

/// Muestra espacial para un hueso o flecha del rig de avatar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoneRigSample {
    pub index: usize,
    pub arc_fraction: f32,
    pub position: [f32; 3],
    /// Vector tangente unitario del desplazamiento local en esa muestra.
    pub tangent: [f32; 3],
    /// Orientación Euler `[pitch, yaw, roll]` requerida para alinear la flecha con el avance.
    pub arrow_rot: [f32; 3],
    /// Orientación independiente de la cámara en ese instante.
    pub camera_rot: [f32; 3],
}

/// Estado de las capacidades de representación del rig frente a las restricciones de VRChat OSC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RigRepresentationStatus {
    Ready,
    /// VRChat no admite modificar matrices de transformación de huesos arbitrarios por OSC directamente.
    /// El adaptador calcula y expone las coordenadas exactas pero marca la sincronización dinámica como pendiente de VRCFury.
    PendingVrcLimitations(String),
}

/// Snapshot completo del rig de avatar para diagnóstico y observabilidad.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarPathRigSnapshot {
    pub bone_count: usize,
    pub total_distance_m: f32,
    pub samples: Vec<BoneRigSample>,
    pub status: RigRepresentationStatus,
    pub camera_pose_now: Option<Pose>,
    pub camera_forward_now: Option<[f32; 3]>,
}

pub struct AvatarPathAdapter {
    config: AvatarIntegrationConfig,
}

impl AvatarPathAdapter {
    pub fn new(config: AvatarIntegrationConfig) -> Self {
        AvatarPathAdapter { config }
    }

    pub fn update_config(&mut self, config: AvatarIntegrationConfig) {
        self.config = config;
    }

    /// Convierte un vector tangente 3D `[tx, ty, tz]` en ángulos de rotación Euler `[pitch_deg, yaw_deg, roll_deg]`.
    pub fn tangent_to_euler_angles(tangent: [f32; 3]) -> [f32; 3] {
        let len = norm3(tangent);
        if len < 1e-5 {
            return [0.0, 0.0, 0.0];
        }
        let tx = tangent[0] / len;
        let ty = tangent[1] / len;
        let tz = tangent[2] / len;

        // Yaw en convención VRChat/Unity: atan2(tx, tz) en grados
        let yaw = tx.atan2(tz).to_degrees();
        // Pitch: -asin(ty) mirando abajo positivo en convención Unity
        let pitch = (-ty.clamp(-1.0, 1.0)).asin().to_degrees();

        [pitch, yaw, 0.0]
    }

    /// Discretiza la trayectoria por longitud de arco generando las poses de los huesos y tangentes de las flechas.
    pub fn compute_rig_samples(
        &self,
        project: &SequenceProject,
        anchor: Option<&Anchor>,
        current_time: f32,
    ) -> AvatarPathRigSnapshot {
        let count = self.config.bone_count.clamp(4, 64);
        let duration = if project.duration.is_finite() { project.duration.max(0.1) } else { 5.0 };

        if project.keyframes.is_empty() {
            return AvatarPathRigSnapshot {
                bone_count: count,
                total_distance_m: 0.0,
                samples: Vec::new(),
                status: RigRepresentationStatus::PendingVrcLimitations(
                    "Sin keyframes definidos para la trayectoria del avatar".into(),
                ),
                camera_pose_now: None,
                camera_forward_now: None,
            };
        }

        // Obtener muestras de posición distribuidas uniformemente en el tiempo / longitud de arco
        let mut raw_positions = Vec::with_capacity(count);
        let mut camera_rots = Vec::with_capacity(count);

        for i in 0..count {
            let u = i as f32 / (count - 1) as f32;
            let t = u * duration;
            let base_p = sample_base_pose_at(project, anchor, t);
            let frame = evaluate(project, anchor, t);
            raw_positions.push(base_p.pos);
            camera_rots.push(frame.final_pose.rot);
        }

        // Calcular longitud acumulada
        let mut total_dist = 0.0_f32;
        for w in raw_positions.windows(2) {
            total_dist += dist3(w[0], w[1]);
        }

        // Calcular tangentes locales para orientar las flechas
        let mut samples = Vec::with_capacity(count);
        for i in 0..count {
            let u = i as f32 / (count - 1) as f32;
            let tangent = if count == 1 {
                [0.0, 0.0, 1.0]
            } else if i == 0 {
                let v = [
                    raw_positions[1][0] - raw_positions[0][0],
                    raw_positions[1][1] - raw_positions[0][1],
                    raw_positions[1][2] - raw_positions[0][2],
                ];
                let l = norm3(v);
                if l > 1e-6 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 0.0, 1.0] }
            } else if i + 1 == count {
                let v = [
                    raw_positions[count - 1][0] - raw_positions[count - 2][0],
                    raw_positions[count - 1][1] - raw_positions[count - 2][1],
                    raw_positions[count - 1][2] - raw_positions[count - 2][2],
                ];
                let l = norm3(v);
                if l > 1e-6 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 0.0, 1.0] }
            } else {
                let v = [
                    raw_positions[i + 1][0] - raw_positions[i - 1][0],
                    raw_positions[i + 1][1] - raw_positions[i - 1][1],
                    raw_positions[i + 1][2] - raw_positions[i - 1][2],
                ];
                let l = norm3(v);
                if l > 1e-6 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 0.0, 1.0] }
            };

            let arrow_rot = Self::tangent_to_euler_angles(tangent);

            samples.push(BoneRigSample {
                index: i,
                arc_fraction: u,
                position: raw_positions[i],
                tangent,
                arrow_rot,
                camera_rot: camera_rots[i],
            });
        }

        let current_frame = evaluate(project, anchor, current_time);
        let cam_fwd = crate::studio::math3d::forward_dir(current_frame.final_pose.rot);

        AvatarPathRigSnapshot {
            bone_count: count,
            total_distance_m: total_dist,
            samples,
            status: RigRepresentationStatus::PendingVrcLimitations(
                "Sincronización dinámica de rig de huesos pendiente de protocolo VRCFury/VRChat".into(),
            ),
            camera_pose_now: Some(current_frame.final_pose),
            camera_forward_now: Some(cam_fwd),
        }
    }
}
