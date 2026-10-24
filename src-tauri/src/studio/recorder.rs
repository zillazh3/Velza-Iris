//! Grabación continua de recorrido y simplificación inteligente de keyframes 6DoF (Ramer-Douglas-Peucker).
//! Preserva siempre las muestras crudas originales (`raw_recording`) y genera puntos editables A, B, C...

use super::math3d::{dist3, lerp, lerp3, slerp_euler, Quat};
use super::model::{point_label, Keyframe, KnotMode, LookTrackPoint, RecordedSample, Transition};
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;

/// Suaviza el micro-temblor de mano en las muestras crudas mediante media ponderada simétrica `[0.25, 0.5, 0.25]`
/// manteniendo fijos el punto inicial y el punto final.
pub fn smooth_samples(samples: &[RecordedSample], passes: usize) -> Vec<RecordedSample> {
    if samples.len() < 3 || passes == 0 {
        return samples.to_vec();
    }
    let mut cur = samples.to_vec();
    for _ in 0..passes.min(5) {
        let mut next = cur.clone();
        for i in 1..(cur.len() - 1) {
            let p_prev = cur[i - 1].pose.pos;
            let p_curr = cur[i].pose.pos;
            let p_next = cur[i + 1].pose.pos;
            let pos = [
                0.25 * p_prev[0] + 0.5 * p_curr[0] + 0.25 * p_next[0],
                0.25 * p_prev[1] + 0.5 * p_curr[1] + 0.25 * p_next[1],
                0.25 * p_prev[2] + 0.5 * p_curr[2] + 0.25 * p_next[2],
            ];
            let r_mid = slerp_euler(cur[i - 1].pose.rot, cur[i + 1].pose.rot, 0.5);
            let rot = slerp_euler(cur[i].pose.rot, r_mid, 0.4);
            next[i].pose = Pose { pos, rot };
        }
        cur = next;
    }
    cur
}

/// Métrica conjunta 6DoF normalizada entre una muestra intermedia y la interpolación lineal entre los extremos.
/// Si el valor supera `1.0`, la desviación excede la tolerancia de posición (`pos_tol_m`) o rotación (`rot_tol_deg`).
fn normalized_error_6dof(
    sample: &RecordedSample,
    start: &RecordedSample,
    end: &RecordedSample,
    pos_tol_m: f32,
    rot_tol_deg: f32,
) -> f32 {
    let dt = (end.time - start.time).max(1e-5);
    let u = ((sample.time - start.time) / dt).clamp(0.0, 1.0);
    let interp_pos = lerp3(start.pose.pos, end.pose.pos, u);
    let pos_err = dist3(sample.pose.pos, interp_pos) / pos_tol_m.max(0.002);

    let qa = Quat::from_euler_deg(start.pose.rot);
    let qb = Quat::from_euler_deg(end.pose.rot);
    let q_interp = Quat::slerp(qa, qb, u);
    let q_sample = Quat::from_euler_deg(sample.pose.rot);
    let rot_err = q_sample.angle_deg(&q_interp) / rot_tol_deg.max(0.2);

    let zoom_err = match (start.zoom, end.zoom, sample.zoom) {
        (Some(z0), Some(z1), Some(zs)) => (zs - lerp(z0, z1, u)).abs() / 3.0,
        _ => 0.0,
    };

    pos_err.max(rot_err).max(zoom_err)
}

fn rdp_recurse(
    samples: &[RecordedSample],
    first: usize,
    last: usize,
    pos_tol_m: f32,
    rot_tol_deg: f32,
    keep: &mut [bool],
) {
    if last <= first + 1 {
        return;
    }
    let mut max_err = 0.0_f32;
    let mut split_idx = first;
    for i in (first + 1)..last {
        let err = normalized_error_6dof(&samples[i], &samples[first], &samples[last], pos_tol_m, rot_tol_deg);
        if err > max_err {
            max_err = err;
            split_idx = i;
        }
    }
    if max_err > 1.0 && split_idx > first && split_idx < last {
        keep[split_idx] = true;
        rdp_recurse(samples, first, split_idx, pos_tol_m, rot_tol_deg, keep);
        rdp_recurse(samples, split_idx, last, pos_tol_m, rot_tol_deg, keep);
    }
}

/// Convierte un recorrido grabado en una lista reducida de `Keyframe`s editables sin alterar las muestras crudas.
pub fn simplify_recording_to_keyframes(
    raw_samples: &[RecordedSample],
    anchor: Option<&Anchor>,
    pos_tol_m: f32,
    rot_tol_deg: f32,
    smoothing_passes: usize,
) -> Vec<Keyframe> {
    if raw_samples.is_empty() {
        return Vec::new();
    }
    let filtered = smooth_samples(raw_samples, smoothing_passes);
    let n = filtered.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    if n > 2 {
        rdp_recurse(&filtered, 0, n - 1, pos_tol_m, rot_tol_deg, &mut keep);
    }

    let total_kept = keep.iter().filter(|&&b| b).count();
    let mut keyframes = Vec::with_capacity(total_kept);
    for (idx, sample) in filtered.iter().enumerate() {
        if !keep[idx] {
            continue;
        }
        let k_idx = keyframes.len();
        let is_endpoint = k_idx == 0 || k_idx + 1 == total_kept;
        keyframes.push(Keyframe {
            id: format!("kf-rec-{}", k_idx + 1),
            label: point_label(k_idx),
            time: (sample.time * 100.0).round() / 100.0,
            raw_pose: sample.pose,
            local_pose: anchor.map(|a| a.world_to_local(&sample.pose)),
            captured_at_ms: 0,
            transition: Transition::Smooth,
            custom_curve: None,
            knot_mode: if is_endpoint { KnotMode::Stop } else { KnotMode::PassThrough },
            look: LookTrackPoint {
                zoom: sample.zoom,
                ..LookTrackPoint::default()
            },
        });
    }
    keyframes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdp_reduces_collinear_samples_and_preserves_corners() {
        // 61 muestras en 6 segundos: de 0..3s avanza en X (0 -> 3m), en t=3s dobla 90° en posición hacia Z (0 -> 3m)
        // mientras rota gradualmente de 90° a 0° en el segundo tramo.
        let mut raw = Vec::new();
        for i in 0..=60 {
            let t = i as f32 * 0.1;
            let (x, z, yaw) = if t <= 3.0 {
                (t, 0.0, 90.0)
            } else {
                let u = (t - 3.0) / 3.0;
                (3.0, t - 3.0, 90.0 * (1.0 - u))
            };
            raw.push(RecordedSample {
                time: t,
                pose: Pose { pos: [x, 1.6, z], rot: [0.0, yaw, 0.0] },
                zoom: None,
            });
        }

        let kfs = simplify_recording_to_keyframes(&raw, None, 0.02, 2.0, 0);
        // Debe reducir las 61 muestras a exactamente 3 puntos clave: inicio (0s), esquina (3s) y final (6s)
        assert_eq!(kfs.len(), 3, "obtuvo {} keyframes: {:?}", kfs.len(), kfs);
        assert_eq!(kfs[0].label, "Punto A");
        assert_eq!(kfs[1].label, "Punto B");
        assert!((kfs[1].time - 3.0).abs() < 1e-3);
        assert_eq!(kfs[2].label, "Punto C");
    }
}
