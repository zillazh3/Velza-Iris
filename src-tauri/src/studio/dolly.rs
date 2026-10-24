//! Adaptador de compatibilidad con el formato JSON de Camera Dolly de VRChat (`CameraPaths`).
//!
//! Permite:
//! 1. **Importar sin VRC+:** Leer trayectorias `.json` de Camera Dolly compartidas por la comunidad
//!    (soportando variantes de esquema objeto `{ "Points": [...] }` o arreglo directo `[...]`,
//!    claves `X/Y/Z` o `x/y/z`, rotaciones Euler o Quaternion, `FocalLength`/`Zoom` y `Duration`)
//!    y convertirlas en un `SequenceProject` reproducible mediante `/usercamera/Pose` a 60 Hz sin VRC+.
//! 2. **Exportar hacia Camera Dolly:** Exportar un `SequenceProject` al formato JSON de Camera Dolly
//!    (como puntos clave directos o como trayectoria horneada con todos los efectos de Iris Studio aplicados).

use super::evaluator::evaluate;
use super::math3d::{wrap180, Quat};
use super::model::{
    point_label, Keyframe, KnotMode, LookTrackPoint, ReferenceSpace, SequenceProject, Transition,
};
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VrcVec3 {
    #[serde(rename = "X", alias = "x")]
    pub x: f32,
    #[serde(rename = "Y", alias = "y")]
    pub y: f32,
    #[serde(rename = "Z", alias = "z")]
    pub z: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VrcDollyPoint {
    #[serde(default, alias = "index")]
    pub index: usize,
    #[serde(default, alias = "pathIndex")]
    pub path_index: usize,
    #[serde(alias = "position")]
    pub position: VrcVec3,
    #[serde(alias = "rotation")]
    pub rotation: Json,
    #[serde(default = "default_focal", alias = "focalLength", alias = "Zoom", alias = "zoom")]
    pub focal_length: f32,
    #[serde(default = "default_duration", alias = "duration")]
    pub duration: f32,
    #[serde(default, alias = "focalDistance", alias = "FocusDistance", alias = "focusDistance")]
    pub focal_distance: Option<f32>,
    #[serde(default, alias = "aperture")]
    pub aperture: Option<f32>,
    #[serde(default, alias = "lookAtMeXOffset")]
    pub look_at_me_x_offset: f32,
    #[serde(default, alias = "lookAtMeYOffset")]
    pub look_at_me_y_offset: f32,
}

fn default_focal() -> f32 {
    45.0
}

fn default_duration() -> f32 {
    2.0
}

fn num_field(obj: &serde_json::Map<String, Json>, upper: &str, lower: &str) -> Option<f32> {
    obj.get(upper)
        .or_else(|| obj.get(lower))
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
}

/// Extrae `[pitch, yaw, roll]` en grados desde el campo `Rotation` de un punto Camera Dolly,
/// aceptando tanto Euler (`{X, Y, Z}`) como Quaternion (`{X, Y, Z, W}`) o arreglo `[x, y, z]`.
fn parse_dolly_rotation(rot_val: &Json) -> Result<[f32; 3], String> {
    if let Some(arr) = rot_val.as_array() {
        if arr.len() == 3 {
            let p = arr[0].as_f64().ok_or("Rotation[0] inválido")? as f32;
            let y = arr[1].as_f64().ok_or("Rotation[1] inválido")? as f32;
            let r = arr[2].as_f64().ok_or("Rotation[2] inválido")? as f32;
            return Ok([wrap180(p).clamp(-89.9, 89.9), wrap180(y), wrap180(r)]);
        } else if arr.len() == 4 {
            let x = arr[0].as_f64().ok_or("Rotation[0] inválido")? as f32;
            let y = arr[1].as_f64().ok_or("Rotation[1] inválido")? as f32;
            let z = arr[2].as_f64().ok_or("Rotation[2] inválido")? as f32;
            let w = arr[3].as_f64().ok_or("Rotation[3] inválido")? as f32;
            return Ok(Quat([x, y, z, w]).normalize().to_euler_deg());
        }
    }
    if let Some(obj) = rot_val.as_object() {
        let x = num_field(obj, "X", "x").ok_or("Falta Rotation.X")?;
        let y = num_field(obj, "Y", "y").ok_or("Falta Rotation.Y")?;
        let z = num_field(obj, "Z", "z").ok_or("Falta Rotation.Z")?;
        if let Some(w) = num_field(obj, "W", "w") {
            // Si W existe y el vector forma un cuaternión unitario (norma ~ 1.0 y ángulos <= 1.001)
            let norm_sq = x * x + y * y + z * z + w * w;
            if (norm_sq - 1.0).abs() < 0.05 && x.abs() <= 1.01 && y.abs() <= 1.01 && z.abs() <= 1.01 {
                return Ok(Quat([x, y, z, w]).normalize().to_euler_deg());
            }
        }
        return Ok([wrap180(x).clamp(-89.9, 89.9), wrap180(y), wrap180(z)]);
    }
    Err("Formato de Rotation en Camera Dolly no reconocido".into())
}

/// Importa un JSON de VRChat Camera Dolly y lo convierte en un `SequenceProject` de Velza Iris.
pub fn import_vrc_dolly_json(raw: &str, active_anchor: Option<Anchor>) -> Result<SequenceProject, String> {
    let parsed: Json = serde_json::from_str(raw).map_err(|e| format!("JSON inválido: {e}"))?;
    let points_val = if parsed.is_array() {
        parsed
    } else if let Some(pts) = parsed.get("Points").or_else(|| parsed.get("points")).cloned() {
        pts
    } else {
        return Err("No se encontró la lista 'Points' ni un arreglo de puntos de Camera Dolly".into());
    };

    let points: Vec<VrcDollyPoint> =
        serde_json::from_value(points_val).map_err(|e| format!("Estructura de puntos Camera Dolly inválida: {e}"))?;
    if points.is_empty() {
        return Err("El archivo Camera Dolly no contiene puntos".into());
    }

    let mut keyframes = Vec::with_capacity(points.len());
    let mut accum_t = 0.0_f32;
    let last_idx = points.len() - 1;

    for (idx, pt) in points.into_iter().enumerate() {
        if idx > 0 {
            accum_t += pt.duration.clamp(0.1, 120.0);
        }
        let rot = parse_dolly_rotation(&pt.rotation)?;
        let raw_pose = Pose {
            pos: [pt.position.x, pt.position.y, pt.position.z],
            rot,
        };
        raw_pose.to_args()?;
        let local_pose = active_anchor.map(|a| a.world_to_local(&raw_pose));
        let zoom = Some(pt.focal_length.clamp(20.0, 300.0));
        let focal_distance = pt.focal_distance.map(|d| d.clamp(0.0, 10.0));
        let aperture = pt.aperture.map(|a| a.clamp(1.4, 32.0));

        keyframes.push(Keyframe {
            id: format!("kf-dolly-{}", idx + 1),
            label: point_label(idx),
            time: (accum_t * 100.0).round() / 100.0,
            raw_pose,
            local_pose,
            captured_at_ms: 0,
            transition: Transition::Smooth,
            custom_curve: None,
            knot_mode: if idx == 0 || idx == last_idx {
                KnotMode::Stop
            } else {
                KnotMode::PassThrough
            },
            look: LookTrackPoint {
                zoom,
                exposure: None,
                focal_distance,
                aperture,
            },
        });
    }

    let total_dur = accum_t.max(2.0);
    let mut proj = SequenceProject::new(
        format!("dolly-import-{}", keyframes.len()),
        "Trayectoria Camera Dolly Importada",
        total_dur,
    );
    proj.space = if active_anchor.is_some() {
        ReferenceSpace::Anchor
    } else {
        ReferenceSpace::World
    };
    proj.anchor_snapshot = active_anchor;
    proj.keyframes = keyframes;
    proj.validate()?;
    Ok(proj)
}

/// Exporta un `SequenceProject` al formato JSON compatible con `CameraPaths` de VRChat Camera Dolly.
/// Si `bake_effects` es `true`, muestrea la trayectoria evaluada con todos los efectos y calibraciones;
/// si es `false`, exporta directamente los keyframes principales.
pub fn export_vrc_dolly_json(
    project: &SequenceProject,
    active_anchor: Option<&Anchor>,
    bake_effects: bool,
) -> Result<String, String> {
    project.validate()?;
    if project.keyframes.is_empty() {
        return Err("La secuencia no tiene puntos para exportar".into());
    }

    let mut out_points = Vec::new();
    let eff_anchor = active_anchor.or(project.anchor_snapshot.as_ref());

    if bake_effects || !project.effects.is_empty() && bake_effects {
        // Muestrear a ~4 puntos por segundo (entre 8 y 600 puntos) para preservar efectos espaciales y ópticos
        // incluso en secuencias de varios minutos.
        let count = ((project.duration * 4.0).round() as usize).clamp(8, 600);
        let step_dt = project.duration / count as f32;
        for i in 0..=count {
            let t = (i as f32 * step_dt).min(project.duration);
            let ev = evaluate(project, eff_anchor, t);
            let pose = ev.final_pose;
            out_points.push(VrcDollyPoint {
                index: i,
                path_index: 0,
                position: VrcVec3 {
                    x: (pose.pos[0] * 10000.0).round() / 10000.0,
                    y: (pose.pos[1] * 10000.0).round() / 10000.0,
                    z: (pose.pos[2] * 10000.0).round() / 10000.0,
                },
                rotation: serde_json::json!({
                    "X": (pose.rot[0] * 100.0).round() / 100.0,
                    "Y": (pose.rot[1] * 100.0).round() / 100.0,
                    "Z": (pose.rot[2] * 100.0).round() / 100.0
                }),
                focal_length: ev.zoom.unwrap_or(45.0).clamp(20.0, 300.0),
                duration: if i == 0 { 0.0 } else { (step_dt * 100.0).round() / 100.0 },
                focal_distance: ev.focal_distance,
                aperture: ev.aperture,
                look_at_me_x_offset: 0.0,
                look_at_me_y_offset: 0.0,
            });
        }
    } else {
        let mut sorted = project.keyframes.clone();
        sorted.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        let mut prev_t = sorted[0].time;
        for (i, kf) in sorted.iter().enumerate() {
            let pose = kf.world_pose(project.space, active_anchor, project.anchor_snapshot.as_ref(), project.trajectory_scale);
            let dt = if i == 0 { 0.0 } else { (kf.time - prev_t).max(0.1) };
            prev_t = kf.time;
            out_points.push(VrcDollyPoint {
                index: i,
                path_index: 0,
                position: VrcVec3 {
                    x: pose.pos[0],
                    y: pose.pos[1],
                    z: pose.pos[2],
                },
                rotation: serde_json::json!({
                    "X": pose.rot[0],
                    "Y": pose.rot[1],
                    "Z": pose.rot[2]
                }),
                focal_length: kf.look.zoom.unwrap_or(45.0).clamp(20.0, 300.0),
                duration: (dt * 100.0).round() / 100.0,
                focal_distance: kf.look.focal_distance,
                aperture: kf.look.aperture,
                look_at_me_x_offset: 0.0,
                look_at_me_y_offset: 0.0,
            });
        }
    }

    let wrapper = serde_json::json!({
        "Version": 1,
        "Generator": "Velza Iris Studio",
        "Points": out_points
    });
    serde_json::to_string_pretty(&wrapper).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_and_exports_vrc_dolly_json_roundtrip() {
        let sample_dolly = r#"{
            "Version": 1,
            "Points": [
                {
                    "Index": 0,
                    "PathIndex": 0,
                    "Position": { "X": -1.2, "Y": 1.65, "Z": 2.0 },
                    "Rotation": { "X": 0.0, "Y": 150.0, "Z": 0.0 },
                    "FocalLength": 35.0,
                    "Duration": 0.0
                },
                {
                    "Index": 1,
                    "PathIndex": 0,
                    "Position": { "X": 0.0, "Y": 1.75, "Z": 1.5 },
                    "Rotation": { "X": 5.0, "Y": 180.0, "Z": 0.0 },
                    "FocalLength": 55.0,
                    "Duration": 4.0
                },
                {
                    "Index": 2,
                    "PathIndex": 0,
                    "Position": { "X": 1.2, "Y": 1.65, "Z": 2.0 },
                    "Rotation": { "X": 0.0, "Y": -150.0, "Z": 0.0 },
                    "FocalLength": 45.0,
                    "Duration": 4.0
                }
            ]
        }"#;

        let anchor = Anchor { eye: [0.0, 1.65, 0.0], yaw: 0.0 };
        let proj = import_vrc_dolly_json(sample_dolly, Some(anchor)).unwrap();
        assert_eq!(proj.keyframes.len(), 3);
        assert_eq!(proj.duration, 8.0);
        assert_eq!(proj.keyframes[0].knot_mode, KnotMode::Stop);
        assert_eq!(proj.keyframes[1].knot_mode, KnotMode::PassThrough);
        assert_eq!(proj.keyframes[2].knot_mode, KnotMode::Stop);
        assert_eq!(proj.keyframes[1].look.zoom, Some(55.0));

        // Exportar de vuelta como nodos clave y reimportar
        let exported = export_vrc_dolly_json(&proj, Some(&anchor), false).unwrap();
        let reimported = import_vrc_dolly_json(&exported, Some(anchor)).unwrap();
        assert_eq!(reimported.keyframes.len(), 3);
        assert!((reimported.keyframes[1].raw_pose.pos[1] - 1.75).abs() < 1e-3);
    }
}
