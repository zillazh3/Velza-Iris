//! Modelo del path para el canal sincronizado: puntos de control, región, slots del bus y progreso `Path_T`.
//!
//! Todo aquí es puro (sin hilos ni OSC) para poder probar la codificación de ida y vuelta contra un emulador del remoto.

use super::protocol::*;
use crate::anchor::Anchor;
use crate::osc::protocol::Pose;
use crate::studio::evaluator::sample_base_pose_at;
use crate::studio::model::SequenceProject;

/// Un escalar a escribir en el bus: `slot` selecciona qué dato lleva `data` (ya pasado por `net_bin`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotWrite {
    pub slot: i32,
    pub data: [f32; 3],
}

/// Path del proyecto tal como lo ven los clientes: 8 puntos de control en mundo + centro de región.
#[derive(Clone, Debug, PartialEq)]
pub struct NetPath {
    /// Centro del cubo de trabajo (mundo, m), en la rejilla que el remoto sabe representar.
    pub region: [f32; 3],
    /// Instante del proyecto (s) de cada punto de control (no decreciente).
    pub times: [f32; NET_POINTS],
    /// Poses de mundo de los puntos (rotación desenvuelta a lo largo del path, grados).
    pub points: [Pose; NET_POINTS],
}

fn wrap180(deg: f32) -> f32 {
    let r = deg.rem_euclid(360.0);
    if r > 180.0 {
        r - 360.0
    } else {
        r
    }
}

impl NetPath {
    /// Construye el path a partir de las muestras `(tiempo, pose)`; fija el centro de región y desenvuelve las rotaciones.
    pub fn new(samples: [(f32, Pose); NET_POINTS]) -> NetPath {
        let mut times = [0.0; NET_POINTS];
        let mut points = [Pose { pos: [0.0; 3], rot: [0.0; 3] }; NET_POINTS];
        for (i, (t, p)) in samples.iter().enumerate() {
            times[i] = *t;
            points[i] = *p;
        }
        for axis in 0..3 {
            let mut prev = wrap180(points[0].rot[axis]);
            points[0].rot[axis] = prev;
            for p in points.iter_mut().skip(1) {
                prev += wrap180(p.rot[axis] - prev);
                p.rot[axis] = prev;
            }
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &points {
            for a in 0..3 {
                lo[a] = lo[a].min(p.pos[a]);
                hi[a] = hi[a].max(p.pos[a]);
            }
        }
        let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        NetPath { region: snap_region(center), times, points }
    }

    /// Posición del punto `i` relativa al centro de la región, normalizada a ±50 m.
    pub fn rel_norm(&self, i: usize) -> [f32; 3] {
        normalize_position(self.points[i].pos, self.region)
    }

    /// Rotación del punto `i` normalizada a ±360° (el overflow se recorta).
    pub fn rot_norm(&self, i: usize) -> [f32; 3] {
        let r = self.points[i].rot;
        [0, 1, 2].map(|a| (r[a] / ROT_RANGE_DEGREES).clamp(-1.0, 1.0))
    }

    /// Escrituras de los parámetros LOCALES del dueño (float32 exacto, sin bus): `(dirección, valor)`.
    pub fn owner_writes(&self) -> Vec<(String, f32)> {
        let mut v = region_writes(self.region);
        for i in 0..NET_POINTS {
            let (p, r) = (self.rel_norm(i), self.rot_norm(i));
            for a in 0..3 {
                v.push((addr_pc(i, a), p[a]));
                v.push((addr_pr(i, a), r[a]));
            }
        }
        v
    }

    /// Slots para los remotos, en orden de prioridad: región, posiciones coarse (el path aparece rápido), rotaciones y
    /// por último las posiciones fine.
    pub fn slots(&self) -> Vec<SlotWrite> {
        let mut out = Vec::with_capacity(1 + 3 * NET_POINTS);
        out.push(SlotWrite { slot: slot::REGION, data: region_bus(self.region) });
        for i in 0..NET_POINTS {
            out.push(SlotWrite { slot: slot::pos_c(i), data: self.rel_norm(i).map(|v| net_bin(split_coarse_fine(v).0)) });
        }
        for i in 0..NET_POINTS {
            out.push(SlotWrite { slot: slot::rot_c(i), data: self.rot_norm(i).map(net_bin) });
        }
        for i in 0..NET_POINTS {
            out.push(SlotWrite { slot: slot::pos_f(i), data: self.rel_norm(i).map(|v| net_bin(split_coarse_fine(v).1)) });
        }
        out
    }

    /// `Path_T ∈ [0, 1]` para el instante `time` del proyecto y la pose real `pos`: segmento por tiempo y avance dentro
    /// del segmento por proyección de la posición (así el easing entre dos puntos se refleja en el gizmo remoto).
    pub fn path_t(&self, time: f32, pos: [f32; 3]) -> f32 {
        let n = NET_POINTS;
        let t = if time.is_finite() { time } else { 0.0 };
        let mut i = 0;
        while i + 2 < n && t >= self.times[i + 1] {
            i += 1;
        }
        let (t0, t1) = (self.times[i], self.times[i + 1]);
        let u_time = if t1 - t0 > 1e-4 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else if t >= t1 { 1.0 } else { 0.0 };
        let (a, b) = (self.points[i].pos, self.points[i + 1].pos);
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let u = if len2 > 1e-4 && pos.iter().all(|x| x.is_finite()) {
            (((pos[0] - a[0]) * d[0] + (pos[1] - a[1]) * d[1] + (pos[2] - a[2]) * d[2]) / len2).clamp(0.0, 1.0)
        } else {
            u_time
        };
        ((i as f32 + u) / (n as f32 - 1.0)).clamp(0.0, 1.0)
    }
}

/// Región local del dueño: `iris_reg_*` en unidades de 1000 m.
pub fn region_writes(region: [f32; 3]) -> Vec<(String, f32)> {
    let r = region.map(|v| (v / REGION_RANGE_METERS).clamp(-1.0, 1.0));
    vec![
        (ADDR_IRIS_REG_X.to_string(), r[0]),
        (ADDR_IRIS_REG_Y.to_string(), r[1]),
        (ADDR_IRIS_REG_Z.to_string(), r[2]),
    ]
}

pub fn region_bus(region: [f32; 3]) -> [f32; 3] {
    region.map(|v| net_bin((v / REGION_RANGE_METERS).clamp(-1.0, 1.0)))
}

/// Slots de la pose retenida (cámara a mano): posición coarse + fine y rotación, relativas al centro de la región.
pub fn hold_slots(region: [f32; 3], pose: &Pose) -> Vec<SlotWrite> {
    let p = normalize_position(pose.pos, region);
    vec![
        SlotWrite { slot: slot::POSE_C, data: p.map(|v| net_bin(split_coarse_fine(v).0)) },
        SlotWrite { slot: slot::POSE_F, data: p.map(|v| net_bin(split_coarse_fine(v).1)) },
        SlotWrite {
            slot: slot::POSE_R,
            data: pose.rot.map(|d| net_bin((wrap180(d) / ROT_RANGE_DEGREES).clamp(-1.0, 1.0))),
        },
    ]
}

/// Elige los 8 puntos de control del path a partir de los keyframes del proyecto: los keyframes mismos si hay ≤ 8
/// (subdividiendo el tramo más largo hasta completar), o primero, último y keyframes repartidos si hay más.
/// Un proyecto en blanco no tiene recorrido: los 8 puntos colapsan en `fallback_pos`.
pub fn sample_control_points(
    project: &SequenceProject,
    active_anchor: Option<&Anchor>,
    fallback_pos: [f32; 3],
) -> [(f32, Pose); NET_POINTS] {
    if project.keyframes.is_empty() {
        return [(0.0, Pose { pos: fallback_pos, rot: [0.0; 3] }); NET_POINTS];
    }
    let duration = project.duration.max(0.1);
    let at = |t: f32| sample_base_pose_at(project, active_anchor, t.clamp(0.0, duration));

    let mut times: Vec<f32> = project.keyframes.iter().map(|k| k.time.clamp(0.0, duration)).collect();
    times.sort_by(|a, b| a.total_cmp(b));
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    let n = times.len();
    let mut pts: Vec<(f32, Pose)> = if n <= 1 {
        (0..NET_POINTS)
            .map(|i| {
                let t = duration * i as f32 / (NET_POINTS - 1) as f32;
                (t, at(t))
            })
            .collect()
    } else if n > NET_POINTS {
        (0..NET_POINTS)
            .map(|i| {
                let t = times[((i * (n - 1)) as f32 / (NET_POINTS - 1) as f32).round() as usize];
                (t, at(t))
            })
            .collect()
    } else {
        times.iter().map(|&t| (t, at(t))).collect()
    };
    let dist = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    while pts.len() < NET_POINTS {
        let j = (0..pts.len() - 1)
            .max_by(|&a, &b| {
                dist(pts[a].1.pos, pts[a + 1].1.pos).total_cmp(&dist(pts[b].1.pos, pts[b + 1].1.pos)).then(b.cmp(&a))
            })
            .unwrap_or(0);
        let t = (pts[j].0 + pts[j + 1].0) * 0.5;
        pts.insert(j + 1, (t, at(t)));
    }
    let mut out = [(0.0, Pose { pos: [0.0; 3], rot: [0.0; 3] }); NET_POINTS];
    for (o, p) in out.iter_mut().zip(pts) {
        *o = p;
    }
    out
}
