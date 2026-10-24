//! Evaluador matemático determinista de secuencias de Iris Studio.
//! Separa estrictamente la trayectoria base (Puntos A -> B -> C...) de las capas no destructivas de efectos,
//! y genera el mapeo espacial para el visor 3D del editor.

use super::calibration::apply_calibration_to_pose;
use super::kinematics::apply_organic_variation;
use super::math3d::{
    dist3, forward_dir, lerp, lerp3, look_at_angles, right_dir, slerp_euler, wrap180, ArcLengthSegment, Quat,
};
use super::model::{EffectClip, EffectKind, Keyframe, KnotMode, SequenceProject, TargetSourceKind, Transition};
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Estado completo evaluado en un instante `t` (puramente determinista).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedFrame {
    pub time: f32,
    /// Pose interpolada únicamente a partir de los keyframes principales (sin efectos temporales).
    pub base_pose: Pose,
    /// Pose final con todos los movimientos aditivos, microefectos, variación orgánica y calibración aplicados.
    pub final_pose: Pose,
    pub zoom: Option<f32>,
    pub exposure: Option<f32>,
    pub focal_distance: Option<f32>,
    pub aperture: Option<f32>,
    /// Rapidez lineal instantánea estimada (m/s).
    #[serde(default)]
    pub linear_speed_mps: f32,
    /// Rapidez angular instantánea estimada (°/s).
    #[serde(default)]
    pub angular_speed_dps: f32,
}

/// Nodo visualizable en el Visor Espacial 3D.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpatialNode {
    pub id: String,
    pub label: String,
    pub time: f32,
    pub world_pos: [f32; 3],
    pub world_rot: [f32; 3],
    pub forward: [f32; 3],
}

/// Representación espacial completa para el Visor 3D del Modo Captura y Modo Editor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryPreview {
    pub nodes: Vec<SpatialNode>,
    /// Curva interpolada entre los keyframes (sin microefectos).
    pub base_path: Vec<[f32; 3]>,
    /// Curva resultante con movimientos y efectos aplicados.
    pub final_path: Vec<[f32; 3]>,
    /// Distancia en metros de cada tramo consecutivo (A->B, B->C...).
    pub segment_distances: Vec<f32>,
    pub total_distance: f32,
    pub anchor_pos: Option<[f32; 3]>,
    pub anchor_yaw: Option<f32>,
    pub target_pos: Option<[f32; 3]>,
}

/// Ruido determinista 1D suave en `[-1, 1]` para microefectos sin estado mutable.
fn hash_noise(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let s = 0.5 - 0.5 * (PI * f).cos();
    let h = |n: i32| -> f32 {
        let mut v = (n as u32).wrapping_add(seed.wrapping_mul(0x9E37_79B9));
        v = (v ^ (v >> 16)).wrapping_mul(0x85EB_CA6B);
        v = (v ^ (v >> 13)).wrapping_mul(0xC2B2_AE35);
        v ^= v >> 16;
        (v as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let n0 = i as i32;
    lerp(h(n0), h(n0 + 1), s)
}

/// Interpola un canal óptico opcional a lo largo de los keyframes que lo definen.
fn eval_optical_channel(kfs: &[Keyframe], t: f32, extract: impl Fn(&Keyframe) -> Option<f32>) -> Option<f32> {
    let defined: Vec<(f32, f32, super::model::Transition)> = kfs
        .iter()
        .filter_map(|k| extract(k).map(|v| (k.time, v, k.transition)))
        .collect();
    if defined.is_empty() {
        return None;
    }
    if defined.len() == 1 || t <= defined[0].0 {
        return Some(defined[0].1);
    }
    let last = defined[defined.len() - 1];
    if t >= last.0 {
        return Some(last.1);
    }
    for w in defined.windows(2) {
        let (t0, v0, _) = w[0];
        let (t1, v1, trans) = w[1];
        if t >= t0 && t <= t1 {
            let span = (t1 - t0).max(1e-5);
            let p = trans.apply((t - t0) / span);
            return Some(lerp(v0, v1, p));
        }
    }
    Some(last.1)
}

/// Calcula el progreso de longitud de arco `s in [0, 1]` dentro del tramo `[seg, seg+1]`
/// respetando si los nodos extremos son de parada (`KnotMode::Stop`) o de paso continuo (`KnotMode::PassThrough`).
fn eval_segment_progress(kfs: &[Keyframe], seg: usize, raw_u: f32) -> f32 {
    let raw_u = raw_u.clamp(0.0, 1.0);
    let n = kfs.len();
    let next_kf = &kfs[seg + 1];

    if let Some([x1, y1, x2, y2]) = next_kf.custom_curve {
        return crate::studio::model::sample_cubic_bezier(raw_u, x1, y1, x2, y2);
    }

    let trans = next_kf.transition;
    if trans == Transition::Linear {
        return raw_u;
    }
    let start_stops = seg == 0 || kfs[seg].knot_mode == KnotMode::Stop;
    let end_stops = seg + 1 >= n - 1 || kfs[seg + 1].knot_mode == KnotMode::Stop;

    match (start_stops, end_stops) {
        (true, true) => trans.apply(raw_u),
        // Acelera desde reposo en `seg` y cruza `seg + 1` con velocidad continua sin detenerse
        (true, false) => 1.0 - (0.5 * PI * raw_u).cos(),
        // Entra con velocidad continua desde `seg` y desacelera suavemente hasta reposo en `seg + 1`
        (false, true) => (0.5 * PI * raw_u).sin(),
        // Crucero continuo entre dos nodos PassThrough
        (false, false) => raw_u,
    }
}

/// Evalúa la Pose base interpolando los keyframes ordenados (`A -> B -> C...`) con parametrización por longitud de arco.
fn eval_base_pose(kfs: &[Keyframe], world_poses: &[Pose], t: f32) -> Pose {
    if world_poses.is_empty() {
        return Pose { pos: [0.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] };
    }
    if world_poses.len() == 1 || t <= kfs[0].time {
        return world_poses[0];
    }
    let n = world_poses.len();
    if t >= kfs[n - 1].time {
        return world_poses[n - 1];
    }

    // Buscar el segmento activo [i, i+1]
    let mut seg = 0;
    for i in 0..(n - 1) {
        if t >= kfs[i].time && t <= kfs[i + 1].time {
            seg = i;
            break;
        }
    }

    let t0 = kfs[seg].time;
    let t1 = kfs[seg + 1].time;
    let raw_u = if (t1 - t0).abs() < 1e-5 { 1.0 } else { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) };
    let s = eval_segment_progress(kfs, seg, raw_u);

    let pos = if n == 2 {
        lerp3(world_poses[0].pos, world_poses[1].pos, s)
    } else {
        // Puntos fantasma en los extremos reflejando la cuerda para continuidad C1 natural
        let p1 = world_poses[seg].pos;
        let p2 = world_poses[seg + 1].pos;
        let p0 = if seg == 0 {
            [2.0 * p1[0] - p2[0], 2.0 * p1[1] - p2[1], 2.0 * p1[2] - p2[2]]
        } else {
            world_poses[seg - 1].pos
        };
        let p3 = if seg + 2 >= n {
            [2.0 * p2[0] - p1[0], 2.0 * p2[1] - p1[1], 2.0 * p2[2] - p1[2]]
        } else {
            world_poses[seg + 2].pos
        };
        let arc_seg = ArcLengthSegment::new(p0, p1, p2, p3);
        arc_seg.eval_at_arc_fraction(s)
    };

    let rot = slerp_euler(world_poses[seg].rot, world_poses[seg + 1].rot, s);
    Pose { pos, rot }
}

/// Aplica un clip de efecto no destructivo sobre la pose y parámetros ópticos en el instante `t`.
fn apply_effect_clip(
    clip: &EffectClip,
    t: f32,
    pivot: [f32; 3],
    pose: &mut Pose,
    zoom: &mut Option<f32>,
    focal_distance: &mut Option<f32>,
    aperture: &mut Option<f32>,
) {
    if !clip.enabled || clip.duration <= 0.0 {
        return;
    }
    let end_time = clip.start_time + clip.duration;
    if t < clip.start_time || t > end_time {
        return;
    }

    let local_t = t - clip.start_time;
    let raw_u = (local_t / clip.duration).clamp(0.0, 1.0);
    let u = if let Some([x1, y1, x2, y2]) = clip.custom_curve {
        crate::studio::model::sample_cubic_bezier(raw_u, x1, y1, x2, y2)
    } else {
        clip.easing.apply(raw_u)
    };
    // Envolvente suave para microefectos temporales (evita saltos al entrar/salir del clip).
    // Nota: en f32, sin(PI * 1.0) es ~ -8.74e-8, por lo que .max(0.0) es indispensable antes de .powf(0.35) para evitar NaN.
    let env = (PI * raw_u).sin().max(0.0).powf(0.35);
    let k = clip.intensity.clamp(0.0, 5.0);
    let spd = clip.speed.clamp(0.1, 10.0);

    let fwd = forward_dir(pose.rot);
    let rgt = right_dir(pose.rot);

    match clip.kind {
        EffectKind::DollyIn => {
            let dist = clip.param_a.unwrap_or(0.8) * k * u;
            for i in 0..3 {
                pose.pos[i] += fwd[i] * dist;
            }
        }
        EffectKind::DollyOut => {
            let dist = clip.param_a.unwrap_or(1.2) * k * u;
            for i in 0..3 {
                pose.pos[i] -= fwd[i] * dist;
            }
        }
        EffectKind::Truck => {
            let dist = clip.param_a.unwrap_or(0.8) * k * u;
            for i in 0..3 {
                pose.pos[i] += rgt[i] * dist;
            }
        }
        EffectKind::Crane => {
            let h = clip.param_a.unwrap_or(0.6) * k * u;
            pose.pos[1] += h;
            // Compensación suave de inclinación para mantener encuadre
            pose.rot[0] = (pose.rot[0] + h * 8.0).clamp(-89.9, 89.9);
        }
        EffectKind::Orbit => {
            let deg = clip.param_a.unwrap_or(45.0) * k * u;
            let rad = deg.to_radians();
            let (s, c) = rad.sin_cos();
            let dx = pose.pos[0] - pivot[0];
            let dz = pose.pos[2] - pivot[2];
            pose.pos[0] = pivot[0] + dx * c + dz * s;
            pose.pos[2] = pivot[2] - dx * s + dz * c;
            pose.rot[1] = wrap180(pose.rot[1] + deg);
        }
        EffectKind::Pan => {
            let deg = clip.param_a.unwrap_or(25.0) * k * u;
            pose.rot[1] = wrap180(pose.rot[1] + deg);
        }
        EffectKind::Tilt => {
            let deg = clip.param_a.unwrap_or(-15.0) * k * u;
            pose.rot[0] = (pose.rot[0] + deg).clamp(-89.9, 89.9);
        }
        EffectKind::Reveal => {
            let dist = clip.param_a.unwrap_or(1.2) * k * u;
            let side = 0.4 * k * u;
            for i in 0..3 {
                pose.pos[i] += -fwd[i] * dist + rgt[i] * side;
            }
            pose.rot[1] = wrap180(pose.rot[1] - 12.0 * k * u);
        }
        EffectKind::MicroShake => {
            let freq = 4.5 * spd;
            for i in 0..3 {
                pose.pos[i] += hash_noise(local_t * freq, 10 + i as u32) * 0.006 * k * env;
            }
            pose.rot[0] = (pose.rot[0] + hash_noise(local_t * freq, 20) * 0.35 * k * env).clamp(-89.9, 89.9);
            pose.rot[1] = wrap180(pose.rot[1] + hash_noise(local_t * freq, 21) * 0.35 * k * env);
            pose.rot[2] = wrap180(pose.rot[2] + hash_noise(local_t * freq, 22) * 0.45 * k * env);
        }
        EffectKind::ImpactShake => {
            let decay = (-4.5 * raw_u).exp();
            let w = local_t * 28.0 * spd;
            pose.pos[1] += w.sin() * 0.025 * k * decay;
            pose.rot[0] = (pose.rot[0] + (w * 1.1).cos() * 1.8 * k * decay).clamp(-89.9, 89.9);
            pose.rot[2] = wrap180(pose.rot[2] + (w * 0.85).sin() * 2.4 * k * decay);
        }
        EffectKind::Handheld => {
            let freq = 0.45 * spd;
            for i in 0..3 {
                pose.pos[i] += hash_noise(local_t * freq, 100 + i as u32) * 0.015 * k * env;
            }
            pose.rot[0] = (pose.rot[0] + hash_noise(local_t * freq, 110) * 0.45 * k * env).clamp(-89.9, 89.9);
            pose.rot[1] = wrap180(pose.rot[1] + hash_noise(local_t * freq, 111) * 0.45 * k * env);
            pose.rot[2] = wrap180(pose.rot[2] + hash_noise(local_t * freq, 112) * 0.35 * k * env);
        }
        EffectKind::CameraDrift => {
            let w = local_t * 0.8 * spd;
            pose.pos[0] += w.sin() * 0.03 * k * env;
            pose.pos[1] += (w * 0.67 + 1.1).cos() * 0.02 * k * env;
            pose.rot[1] = wrap180(pose.rot[1] + (w * 0.5).sin() * 0.8 * k * env);
        }
        EffectKind::CameraBreathing => {
            // ~0.22 Hz nominal (respiración humana tranquila)
            let phase = local_t * 0.22 * 2.0 * PI * spd;
            let breath = phase.sin() * env;
            pose.pos[1] += breath * 0.012 * k;
            pose.rot[0] = (pose.rot[0] - breath * 0.35 * k).clamp(-89.9, 89.9);
        }
        EffectKind::SmoothZoom => {
            let z0 = clip.param_a.filter(|v| v.is_finite()).unwrap_or_else(|| zoom.unwrap_or(45.0)).clamp(20.0, 300.0);
            let z1 = clip.param_b.filter(|v| v.is_finite()).unwrap_or((z0 + 25.0 * k).clamp(20.0, 300.0)).clamp(20.0, 300.0);
            let res = lerp(z0, z1, u);
            *zoom = Some(if res.is_finite() { res.clamp(20.0, 300.0) } else { 45.0 });
        }
        EffectKind::CrashZoom => {
            let z0 = clip.param_a.filter(|v| v.is_finite()).unwrap_or_else(|| zoom.unwrap_or(35.0)).clamp(20.0, 300.0);
            let z1 = clip.param_b.filter(|v| v.is_finite()).unwrap_or((z0 + 65.0 * k).clamp(20.0, 300.0)).clamp(20.0, 300.0);
            let expo = if raw_u < 0.5 { 8.0 * raw_u.powi(4) } else { 1.0 - (-2.0 * raw_u + 2.0).powi(4) / 2.0 };
            let res = lerp(z0, z1, expo);
            *zoom = Some(if res.is_finite() { res.clamp(20.0, 300.0) } else { 45.0 });
        }
        EffectKind::FocusPull => {
            let f0 = clip.param_a.unwrap_or(focal_distance.unwrap_or(2.5));
            let f1 = clip.param_b.unwrap_or(1.0);
            *focal_distance = Some(lerp(f0, f1, u).clamp(0.0, 10.0));
        }
        EffectKind::ApertureShift => {
            let a0 = clip.param_a.unwrap_or(aperture.unwrap_or(8.0));
            let a1 = clip.param_b.unwrap_or(2.0);
            *aperture = Some(lerp(a0, a1, u).clamp(1.4, 32.0));
        }
        EffectKind::DollyZoom => {
            let dist_delta = clip.param_a.filter(|v| v.is_finite()).unwrap_or(1.5) * k * u;
            for i in 0..3 {
                pose.pos[i] -= fwd[i] * dist_delta;
            }
            let d = dist3(pose.pos, pivot).max(0.3);
            let per_meter = clip.param_b.filter(|v| v.is_finite()).unwrap_or(22.0);
            let res = d * per_meter;
            *zoom = Some(if res.is_finite() { res.clamp(20.0, 300.0) } else { 45.0 });
        }
    }
}

fn evaluate_uncalibrated_at(
    project: &SequenceProject,
    kfs: &[Keyframe],
    world_poses: &[Pose],
    effects: &[EffectClip],
    pivot: [f32; 3],
    effective_anchor: Option<&Anchor>,
    t: f32,
) -> (Pose, Pose, Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
    let base_pose = eval_base_pose(kfs, world_poses, t);
    let mut final_pose = base_pose;

    let mut zoom = eval_optical_channel(kfs, t, |k| k.look.zoom);
    let exposure = eval_optical_channel(kfs, t, |k| k.look.exposure);
    let mut focal_distance = eval_optical_channel(kfs, t, |k| k.look.focal_distance);
    let mut aperture = eval_optical_channel(kfs, t, |k| k.look.aperture);

    for clip in effects {
        apply_effect_clip(
            clip,
            t,
            pivot,
            &mut final_pose,
            &mut zoom,
            &mut focal_distance,
            &mut aperture,
        );
    }

    // Variación orgánica reproducible del perfil cinematográfico (si aplica)
    apply_organic_variation(&mut final_pose, t, &project.cinematic);

    // Seguimiento de objetivo opcional (`TargetProvider` calibrado).
    // Si la fuente es `CalibratedAnchor`, resuelve dinámicamente hacia el ancla activa del operador local.
    if let Some(tg) = project.target.filter(|tg| tg.enabled) {
        let target_pos = match tg.source {
            TargetSourceKind::CalibratedAnchor => effective_anchor.map(|a| a.eye).unwrap_or(tg.world_pos),
            _ => tg.world_pos,
        };
        let [target_pitch, target_yaw] = look_at_angles(final_pose.pos, target_pos);
        let target_rot = [target_pitch, target_yaw, final_pose.rot[2]];
        final_pose.rot = slerp_euler(final_pose.rot, target_rot, tg.blend.clamp(0.0, 1.0));
    }

    (base_pose, final_pose, zoom, exposure, focal_distance, aperture)
}

/// Evalúa la secuencia completa en el instante `t` (en segundos).
pub fn evaluate(project: &SequenceProject, active_anchor: Option<&Anchor>, t: f32) -> EvaluatedFrame {
    let dur = project.duration.max(0.001);
    let t = t.clamp(0.0, dur);
    let mut kfs = project.keyframes.clone();
    kfs.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

    let scale = project.trajectory_scale;
    let fallback_anchor = project.anchor_snapshot.as_ref();
    let eff_anchor = active_anchor.or(fallback_anchor);
    let world_poses: Vec<Pose> = kfs
        .iter()
        .map(|k| k.world_pose(project.space, active_anchor, fallback_anchor, scale))
        .collect();

    let pivot = eff_anchor
        .map(|a| a.eye)
        .or_else(|| project.target.map(|tg| tg.world_pos))
        .unwrap_or([0.0, 1.6, 0.0]);

    // Ordenar clips por pista e inicio para evaluación determinista
    let mut effects = project.effects.clone();
    effects.sort_by(|a, b| {
        a.track
            .cmp(&b.track)
            .then_with(|| a.start_time.partial_cmp(&b.start_time).unwrap_or(std::cmp::Ordering::Equal))
    });

    let (base_pose, uncal_final, zoom, exposure, focal_distance, aperture) =
        evaluate_uncalibrated_at(project, &kfs, &world_poses, &effects, pivot, eff_anchor, t);

    // Derivada numérica centrada para rapidez lineal, angular y velocidad lateral (lead room)
    let eps = 0.01_f32;
    let t_prev = (t - eps).clamp(0.0, dur);
    let t_next = (t + eps).clamp(0.0, dur);
    let dt_span = (t_next - t_prev).max(1e-4);
    let (_, pose_prev, _, _, _, _) =
        evaluate_uncalibrated_at(project, &kfs, &world_poses, &effects, pivot, eff_anchor, t_prev);
    let (_, pose_next, _, _, _, _) =
        evaluate_uncalibrated_at(project, &kfs, &world_poses, &effects, pivot, eff_anchor, t_next);

    let linear_speed_mps = {
        let s = dist3(pose_prev.pos, pose_next.pos) / dt_span;
        if s.is_finite() { s } else { 0.0 }
    };
    let angular_speed_dps = {
        let w = Quat::from_euler_deg(pose_prev.rot).angle_deg(&Quat::from_euler_deg(pose_next.rot)) / dt_span;
        if w.is_finite() { w } else { 0.0 }
    };

    let vel_vec = [
        (pose_next.pos[0] - pose_prev.pos[0]) / dt_span,
        (pose_next.pos[1] - pose_prev.pos[1]) / dt_span,
        (pose_next.pos[2] - pose_prev.pos[2]) / dt_span,
    ];
    let rgt = right_dir(uncal_final.rot);
    let raw_lat_vel = vel_vec[0] * rgt[0] + vel_vec[1] * rgt[1] + vel_vec[2] * rgt[2];
    let lateral_vel = if raw_lat_vel.is_finite() { raw_lat_vel } else { 0.0 };

    let mut final_pose = apply_calibration_to_pose(
        uncal_final,
        &project.calibration,
        zoom.unwrap_or(45.0),
        lateral_vel,
        [0.0; 3],
        [0.0; 3],
    );
    // Garantizar que ningún valor no finito pueda propagarse hacia OSC
    if !final_pose.pos.iter().chain(&final_pose.rot).all(|x| x.is_finite()) {
        final_pose = base_pose;
    }

    EvaluatedFrame {
        time: t,
        base_pose,
        final_pose,
        zoom,
        exposure,
        focal_distance,
        aperture,
        linear_speed_mps,
        angular_speed_dps,
    }
}

/// Genera los datos de mapeo espacial para el Visor 3D de Iris Studio.
pub fn sample_trajectory(project: &SequenceProject, active_anchor: Option<&Anchor>, steps: usize) -> TrajectoryPreview {
    let steps = steps.clamp(16, 300);
    let mut kfs = project.keyframes.clone();
    kfs.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

    let scale = project.trajectory_scale;
    let fallback_anchor = project.anchor_snapshot.as_ref();
    let nodes: Vec<SpatialNode> = kfs
        .iter()
        .map(|k| {
            let wp = k.world_pose(project.space, active_anchor, fallback_anchor, scale);
            SpatialNode {
                id: k.id.clone(),
                label: k.label.clone(),
                time: k.time,
                world_pos: wp.pos,
                world_rot: wp.rot,
                forward: forward_dir(wp.rot),
            }
        })
        .collect();

    let mut segment_distances = Vec::new();
    for w in nodes.windows(2) {
        segment_distances.push(dist3(w[0].world_pos, w[1].world_pos));
    }
    let total_distance: f32 = segment_distances.iter().sum();

    let mut base_path = Vec::with_capacity(steps + 1);
    let mut final_path = Vec::with_capacity(steps + 1);
    if !nodes.is_empty() {
        for i in 0..=steps {
            let t = project.duration * (i as f32 / steps as f32);
            let ev = evaluate(project, active_anchor, t);
            base_path.push(ev.base_pose.pos);
            final_path.push(ev.final_pose.pos);
        }
    }

    let eff_anchor = active_anchor.or(fallback_anchor);
    TrajectoryPreview {
        nodes,
        base_path,
        final_path,
        segment_distances,
        total_distance,
        anchor_pos: eff_anchor.map(|a| a.eye),
        anchor_yaw: eff_anchor.map(|a| a.yaw),
        target_pos: project.target.filter(|t| t.enabled).map(|t| match t.source {
            TargetSourceKind::CalibratedAnchor => eff_anchor.map(|a| a.eye).unwrap_or(t.world_pos),
            _ => t.world_pos,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::model::{LookTrackPoint, ReferenceSpace, Transition};

    fn make_kf(id: &str, time: f32, pos: [f32; 3], yaw: f32) -> Keyframe {
        Keyframe {
            id: id.into(),
            label: format!("Punto {id}"),
            time,
            raw_pose: Pose { pos, rot: [0.0, yaw, 0.0] },
            local_pose: None,
            captured_at_ms: 0,
            transition: Transition::Linear,
            custom_curve: None,
            knot_mode: KnotMode::PassThrough,
            look: LookTrackPoint::default(),
        }
    }

    #[test]
    fn pass_through_keyframe_maintains_continuous_velocity_without_stopping() {
        let mut p = SequenceProject::new("test-pass", "Continuidad", 10.0);
        p.space = ReferenceSpace::World;
        let mut a = make_kf("A", 0.0, [0.0, 1.6, 2.0], 180.0);
        a.transition = Transition::Smooth;
        a.knot_mode = KnotMode::Stop;

        let mut b = make_kf("B", 5.0, [5.0, 1.6, 2.0], 180.0);
        b.transition = Transition::Smooth;
        b.knot_mode = KnotMode::PassThrough;

        let mut c = make_kf("C", 10.0, [10.0, 1.6, 2.0], 180.0);
        c.transition = Transition::Smooth;
        c.knot_mode = KnotMode::Stop;

        p.keyframes = vec![a, b, c];
        let mid_ev = evaluate(&p, None, 5.0);
        // En el Punto B (PassThrough), la rapidez lineal debe ser positiva (>= 0.85 * velocidad media de 1.0 m/s),
        // nunca detenerse en cero.
        assert!(
            mid_ev.linear_speed_mps >= 0.85,
            "el nodo PassThrough frenó la cámara: speed = {}",
            mid_ev.linear_speed_mps
        );
    }

    #[test]
    fn evaluates_deterministically_and_hits_exact_keyframes() {
        let mut p = SequenceProject::new("test", "Demo", 10.0);
        p.space = ReferenceSpace::World;
        p.keyframes.push(make_kf("A", 0.0, [0.0, 1.6, 2.0], 180.0));
        p.keyframes.push(make_kf("B", 5.0, [2.0, 1.8, 1.0], -90.0));
        p.keyframes.push(make_kf("C", 10.0, [0.0, 1.6, -2.0], 0.0));

        let ev0 = evaluate(&p, None, 0.0);
        let ev5 = evaluate(&p, None, 5.0);
        let ev10 = evaluate(&p, None, 10.0);

        assert!(dist3(ev0.final_pose.pos, [0.0, 1.6, 2.0]) < 1e-4);
        assert!(dist3(ev5.final_pose.pos, [2.0, 1.8, 1.0]) < 1e-4);
        assert!(dist3(ev10.final_pose.pos, [0.0, 1.6, -2.0]) < 1e-4);

        // Determinismo: evaluar en cualquier orden da exactamente el mismo valor al bit
        let sample_a = evaluate(&p, None, 3.37);
        let _ = evaluate(&p, None, 8.91);
        let sample_b = evaluate(&p, None, 3.37);
        assert_eq!(sample_a, sample_b);
    }

    #[test]
    fn effects_are_non_destructive_and_coexist() {
        let mut p = SequenceProject::new("test", "Demo", 15.0);
        p.space = ReferenceSpace::World;
        p.keyframes.push(make_kf("A", 0.0, [0.0, 1.6, 2.0], 180.0));
        p.keyframes.push(make_kf("B", 15.0, [0.0, 1.6, 4.0], 180.0));

        p.effects.push(EffectClip {
            id: "e1".into(),
            kind: EffectKind::SmoothZoom,
            track: 0,
            start_time: 3.0,
            duration: 6.0, // 3..9 s
            intensity: 1.0,
            speed: 1.0,
            param_a: Some(40.0),
            param_b: Some(80.0),
            easing: Transition::Linear,
            custom_curve: None,
            enabled: true,
        });
        p.effects.push(EffectClip {
            id: "e2".into(),
            kind: EffectKind::ImpactShake,
            track: 1,
            start_time: 11.0,
            duration: 0.5, // 11..11.5 s
            intensity: 1.0,
            speed: 1.0,
            param_a: None,
            param_b: None,
            easing: Transition::Linear,
            custom_curve: None,
            enabled: true,
        });

        // En t=6s (mitad del zoom): zoom debe ser 60mm, base_pose intacta
        let ev6 = evaluate(&p, None, 6.0);
        assert!((ev6.zoom.unwrap() - 60.0).abs() < 1e-3);
        assert_eq!(ev6.base_pose.pos, ev6.final_pose.pos);

        // En t=11.1s (durante ImpactShake): base_pose sigue en su línea recta, final_pose tiene el shake
        let ev11 = evaluate(&p, None, 11.1);
        assert_ne!(ev11.base_pose.pos[1], ev11.final_pose.pos[1]);

        // En t=14s (después del ImpactShake): base_pose == final_pose
        let ev14 = evaluate(&p, None, 14.0);
        assert_eq!(ev14.base_pose, ev14.final_pose);
    }

    #[test]
    fn all_effect_kinds_on_fx_layers_produce_finite_valid_osc_poses_at_all_times() {
        let all_kinds = [
            EffectKind::DollyIn,
            EffectKind::DollyOut,
            EffectKind::Orbit,
            EffectKind::Truck,
            EffectKind::Crane,
            EffectKind::Pan,
            EffectKind::Tilt,
            EffectKind::Reveal,
            EffectKind::MicroShake,
            EffectKind::ImpactShake,
            EffectKind::Handheld,
            EffectKind::CameraDrift,
            EffectKind::CameraBreathing,
            EffectKind::SmoothZoom,
            EffectKind::CrashZoom,
            EffectKind::FocusPull,
            EffectKind::ApertureShift,
            EffectKind::DollyZoom,
        ];

        for (idx, kind) in all_kinds.into_iter().enumerate() {
            let mut p = SequenceProject::new("test-fx", "FX Finite Test", 10.0);
            p.space = ReferenceSpace::World;
            p.calibration.enabled = true;
            p.keyframes.push(make_kf("A", 0.0, [0.0, 1.6, 2.0], 180.0));
            p.keyframes.push(make_kf("B", 10.0, [1.0, 1.7, 2.0], 160.0));
            p.effects.push(EffectClip {
                id: format!("fx-{idx}"),
                kind,
                track: (idx % 3) as u8,
                start_time: 0.0,
                duration: 10.0,
                intensity: 1.2,
                speed: 1.0,
                param_a: None,
                param_b: None,
                easing: Transition::Smooth,
                custom_curve: None,
                enabled: true,
            });

            // Evaluar incluyendo los bordes exactos (t = 0.0, t = 9.99, t = 10.0 donde raw_u == 1.0)
            for step in 0..=100 {
                let t = step as f32 * 0.1;
                let ev = evaluate(&p, None, t);
                assert!(
                    ev.final_pose.to_args().is_ok(),
                    "Efecto {:?} en t={t} produjo Pose no finita: {:?}",
                    kind,
                    ev.final_pose
                );
            }
        }
    }

    #[test]
    fn calibrated_anchor_target_tracks_operator_anchor_dynamically() {
        use crate::studio::model::TargetConfig;

        let mut p = SequenceProject::new("test-anchor-target", "Anchor Tracking", 5.0);
        p.space = ReferenceSpace::World;
        p.keyframes.push(make_kf("A", 0.0, [0.0, 1.65, 2.0], 0.0));
        // Aunque `world_pos` tenga una coordenada residual antigua de otro lugar ([-50, 0, 50]),
        // cuando `source == CalibratedAnchor`, debe mirar siempre al `active_anchor` del operador local.
        p.target = Some(TargetConfig {
            enabled: true,
            source: TargetSourceKind::CalibratedAnchor,
            world_pos: [-50.0, 0.0, 50.0],
            blend: 1.0,
            confidence: 1.0,
            captured_at_ms: 0,
        });

        let operator_anchor = Anchor {
            eye: [0.0, 1.65, 0.0],
            yaw: 0.0,
        };
        let ev = evaluate(&p, Some(&operator_anchor), 0.0);
        // La cámara en (0, 1.65, 2) mirando al operador en (0, 1.65, 0) debe tener yaw = 180° (o -180°) y pitch = 0°
        assert!(ev.final_pose.rot[0].abs() < 1e-3);
        assert!((ev.final_pose.rot[1].abs() - 180.0).abs() < 1e-2);

        let prev = sample_trajectory(&p, Some(&operator_anchor), 16);
        assert_eq!(prev.target_pos, Some(operator_anchor.eye));
    }
}
