//! Matemática 3D determinista para Iris Studio: Quaternions (SLERP), Catmull-Rom centrípeta y vectores de cámara.
//! Convención VRChat/Unity medida en M0 (`docs/osc-findings.md`):
//! - Mano izquierda: +X derecha, +Y arriba, +Z frente (con yaw = 0°).
//! - Rotación Euler `[pitch, yaw, roll]` en grados:
//!   - `pitch` (+ mira hacia abajo, limitado a ±89.9° al enviar a OSC para evitar reinterpretación de Unity).
//!   - `yaw` (+ gira hacia la derecha visto desde arriba, es decir de +Z hacia +X).
//!   - `roll` (+ inclina hacia la izquierda/derecha alrededor del eje forward).

/// Normaliza un ángulo en grados al intervalo `(-180, 180]`.
pub fn wrap180(deg: f32) -> f32 {
    let r = deg.rem_euclid(360.0);
    if r > 180.0 { r - 360.0 } else { r }
}

/// Diferencia angular mínima en grados `b - a` dentro de `(-180, 180]`.
pub fn angle_diff_deg(a: f32, b: f32) -> f32 {
    wrap180(b - a)
}

/// Interpolación lineal de un escalar.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Interpolación lineal de un vector 3D.
pub fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// Distancia euclidiana entre dos puntos 3D.
pub fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Quaternion unitario `[x, y, z, w]` para interpolar rotaciones 3D sin gimbal lock ni saltos en 0°/360°.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat(pub [f32; 4]);

impl Quat {
    /// Convierte Euler `[pitch, yaw, roll]` (en grados, orden intrínseco Y-X-Z de cámara Unity) a Quaternion unitario.
    pub fn from_euler_deg(rot: [f32; 3]) -> Quat {
        let half = 0.5_f32.to_radians();
        let (sp, cp) = (rot[0] * half).sin_cos();
        let (sy, cy) = (rot[1] * half).sin_cos();
        let (sr, cr) = (rot[2] * half).sin_cos();

        // Orden intrínseco Y (yaw) -> X (pitch) -> Z (roll): Q = Qy * Qx * Qz
        let x = cy * sp * cr + sy * cp * sr;
        let y = sy * cp * cr - cy * sp * sr;
        let z = cy * cp * sr - sy * sp * cr;
        let w = cy * cp * cr + sy * sp * sr;
        Quat([x, y, z, w]).normalize()
    }

    /// Convierte el Quaternion unitario de vuelta a Euler `[pitch, yaw, roll]` en grados.
    /// `pitch` se recorta a `[-89.9, 89.9]` para evitar la singularidad de ±90° en VRChat OSC.
    pub fn to_euler_deg(&self) -> [f32; 3] {
        let [x, y, z, w] = self.normalize().0;
        // Para Q = Qy * Qx * Qz:
        // sin(pitch) = 2 * (w*x - y*z)
        let sinp = (2.0 * (w * x - y * z)).clamp(-1.0, 1.0);
        let pitch_rad = sinp.asin();

        let (yaw_rad, roll_rad) = if sinp.abs() < 0.9999 {
            let yaw = (2.0 * (w * y + x * z)).atan2(1.0 - 2.0 * (x * x + y * y));
            let roll = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (x * x + z * z));
            (yaw, roll)
        } else {
            // Cerca del bloqueo de cardán (±90° pitch), fijamos roll = 0 y resolvemos yaw
            let yaw = (-2.0 * (x * z - w * y)).atan2(1.0 - 2.0 * (y * y + z * z));
            (yaw, 0.0)
        };

        [
            pitch_rad.to_degrees().clamp(-89.9, 89.9),
            wrap180(yaw_rad.to_degrees()),
            wrap180(roll_rad.to_degrees()),
        ]
    }

    pub fn normalize(&self) -> Quat {
        let [x, y, z, w] = self.0;
        let len = (x * x + y * y + z * z + w * w).sqrt();
        if len > 1e-7 {
            Quat([x / len, y / len, z / len, w / len])
        } else {
            Quat([0.0, 0.0, 0.0, 1.0])
        }
    }

    pub fn dot(&self, other: &Quat) -> f32 {
        self.0[0] * other.0[0] + self.0[1] * other.0[1] + self.0[2] * other.0[2] + self.0[3] * other.0[3]
    }

    /// Interpolación esférica (SLERP) por el camino más corto en SO(3).
    pub fn slerp(a: Quat, mut b: Quat, t: f32) -> Quat {
        let t = t.clamp(0.0, 1.0);
        let mut cos_theta = a.dot(&b);
        if cos_theta < 0.0 {
            b = Quat([-b.0[0], -b.0[1], -b.0[2], -b.0[3]]);
            cos_theta = -cos_theta;
        }
        if cos_theta > 0.9995 {
            // NLERP para ángulos muy pequeños
            return Quat([
                lerp(a.0[0], b.0[0], t),
                lerp(a.0[1], b.0[1], t),
                lerp(a.0[2], b.0[2], t),
                lerp(a.0[3], b.0[3], t),
            ])
            .normalize();
        }
        let theta = cos_theta.clamp(-1.0, 1.0).acos();
        let sin_theta = theta.sin();
        let wa = ((1.0 - t) * theta).sin() / sin_theta;
        let wb = (t * theta).sin() / sin_theta;
        Quat([
            a.0[0] * wa + b.0[0] * wb,
            a.0[1] * wa + b.0[1] * wb,
            a.0[2] * wa + b.0[2] * wb,
            a.0[3] * wa + b.0[3] * wb,
        ])
        .normalize()
    }

    /// Ángulo en grados entre dos orientaciones `[0, 180]`.
    pub fn angle_deg(&self, other: &Quat) -> f32 {
        let d = self.dot(other).abs().clamp(0.0, 1.0);
        (2.0 * d.acos()).to_degrees()
    }

    /// Rota desde `self` hacia `target` limitando el paso angular máximo a `max_deg` grados.
    pub fn rotate_towards(self, target: Quat, max_deg: f32) -> Quat {
        if max_deg <= 0.0 {
            return self;
        }
        let ang = self.angle_deg(&target);
        if ang <= max_deg || ang < 1e-4 {
            return target;
        }
        Self::slerp(self, target, (max_deg / ang).clamp(0.0, 1.0))
    }
}

/// Norma euclidiana de un vector 3D.
pub fn norm3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Limita la magnitud de un vector 3D a `max_len`.
pub fn clamp_vec3_len(v: [f32; 3], max_len: f32) -> [f32; 3] {
    let m = norm3(v);
    if m > max_len && m > 1e-6 {
        let s = max_len / m;
        [v[0] * s, v[1] * s, v[2] * s]
    } else {
        v
    }
}

/// Interpola dos rotaciones Euler `[pitch, yaw, roll]` usando SLERP de Quaternions.
pub fn slerp_euler(rot_a: [f32; 3], rot_b: [f32; 3], t: f32) -> [f32; 3] {
    let qa = Quat::from_euler_deg(rot_a);
    let qb = Quat::from_euler_deg(rot_b);
    Quat::slerp(qa, qb, t).to_euler_deg()
}

/// Spline Catmull-Rom Centrípeta (alpha = 0.5) entre `p1` y `p2` con puntos de control vecinos `p0` y `p3`.
/// Garantiza paso exacto por `p1` (u=0) y `p2` (u=1) sin sobreimpulsos (overshoot) ni lazos.
pub fn catmull_rom_centripetal(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], p3: [f32; 3], u: f32) -> [f32; 3] {
    let u = u.clamp(0.0, 1.0);
    if dist3(p1, p2) < 1e-5 {
        return p1;
    }
    let knot_interval = |a: [f32; 3], b: [f32; 3]| dist3(a, b).sqrt().max(1e-3);
    let t0 = 0.0_f32;
    let t1 = t0 + knot_interval(p0, p1);
    let t2 = t1 + knot_interval(p1, p2);
    let t3 = t2 + knot_interval(p2, p3);

    let t = lerp(t1, t2, u);
    let eval_axis = |axis: usize| {
        let a1 = (t1 - t) / (t1 - t0) * p0[axis] + (t - t0) / (t1 - t0) * p1[axis];
        let a2 = (t2 - t) / (t2 - t1) * p1[axis] + (t - t1) / (t2 - t1) * p2[axis];
        let a3 = (t3 - t) / (t3 - t2) * p2[axis] + (t - t2) / (t3 - t2) * p3[axis];

        let b1 = (t2 - t) / (t2 - t0) * a1 + (t - t0) / (t2 - t0) * a2;
        let b2 = (t3 - t) / (t3 - t1) * a2 + (t - t1) / (t3 - t1) * a3;

        (t2 - t) / (t2 - t1) * b1 + (t - t1) / (t2 - t1) * b2
    };

    [eval_axis(0), eval_axis(1), eval_axis(2)]
}

const ARC_SAMPLES: usize = 32;

/// Tabla de parametrización por longitud de arco sobre un segmento Catmull-Rom centrípeto.
/// Convierte una fracción de distancia recorrida `s in [0, 1]` en el parámetro `u in [0, 1]`
/// para garantizar rapidez constante cuando la curva no tiene longitud de cuerda uniforme.
#[derive(Clone, Copy, Debug)]
pub struct ArcLengthSegment {
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    cum_len: [f32; ARC_SAMPLES + 1],
    pub total_length: f32,
}

impl ArcLengthSegment {
    pub fn new(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], p3: [f32; 3]) -> Self {
        let mut cum_len = [0.0_f32; ARC_SAMPLES + 1];
        let mut prev = p1;
        let mut acc = 0.0_f32;
        for (i, len) in cum_len.iter_mut().enumerate().skip(1) {
            let u = i as f32 / ARC_SAMPLES as f32;
            let pt = catmull_rom_centripetal(p0, p1, p2, p3, u);
            acc += dist3(prev, pt);
            *len = acc;
            prev = pt;
        }
        ArcLengthSegment {
            p0,
            p1,
            p2,
            p3,
            cum_len,
            total_length: acc,
        }
    }

    /// Invierte la tabla de longitud de arco para obtener `u` a partir de la fracción de arco `s in [0, 1]`.
    pub fn u_from_arc_fraction(&self, s: f32) -> f32 {
        let s = s.clamp(0.0, 1.0);
        if self.total_length < 1e-5 {
            return s;
        }
        let target = s * self.total_length;
        let mut lo = 0usize;
        let mut hi = ARC_SAMPLES;
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.cum_len[mid] <= target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let l0 = self.cum_len[lo];
        let l1 = self.cum_len[hi];
        let frac = if (l1 - l0).abs() < 1e-6 { 0.0 } else { (target - l0) / (l1 - l0) };
        ((lo as f32 + frac) / ARC_SAMPLES as f32).clamp(0.0, 1.0)
    }

    /// Evalúa la posición 3D en la fracción de longitud de arco `s in [0, 1]`.
    pub fn eval_at_arc_fraction(&self, s: f32) -> [f32; 3] {
        let u = self.u_from_arc_fraction(s);
        catmull_rom_centripetal(self.p0, self.p1, self.p2, self.p3, u)
    }
}

/// Vector unitario `forward` de la cámara según su rotación `[pitch, yaw, roll]` en grados.
pub fn forward_dir(rot: [f32; 3]) -> [f32; 3] {
    let p = rot[0].to_radians();
    let y = rot[1].to_radians();
    // pitch positivo = mirar hacia abajo (-Y)
    let cos_p = p.cos();
    [y.sin() * cos_p, -p.sin(), y.cos() * cos_p]
}

/// Vector unitario `right` horizontal de la cámara según su `yaw` en grados.
pub fn right_dir(rot: [f32; 3]) -> [f32; 3] {
    let y = rot[1].to_radians();
    [y.cos(), 0.0, -y.sin()]
}

/// Calcula `[pitch, yaw]` en grados para que la cámara en `from` mire hacia el punto `target`.
pub fn look_at_angles(from: [f32; 3], target: [f32; 3]) -> [f32; 2] {
    let dx = target[0] - from[0];
    let dy = target[1] - from[1];
    let dz = target[2] - from[2];
    let horiz = (dx * dx + dz * dz).sqrt();
    if horiz < 1e-5 && dy.abs() < 1e-5 {
        return [0.0, 0.0];
    }
    let yaw = dx.atan2(dz).to_degrees();
    // Si el objetivo está más abajo (dy < 0), pitch es positivo (mirar abajo)
    let pitch = (-dy).atan2(horiz).to_degrees().clamp(-89.9, 89.9);
    [pitch, wrap180(yaw)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-2
    }

    #[test]
    fn quat_euler_roundtrip_and_shortest_path_slerp() {
        let angles = [[-15.0, 40.0, 8.0], [25.0, -135.0, -12.0], [0.0, 180.0, 0.0]];
        for r in angles {
            let q = Quat::from_euler_deg(r);
            let out = q.to_euler_deg();
            assert!(near(r[0], out[0]) && near(wrap180(r[1]), out[1]) && near(r[2], out[2]), "{r:?} -> {out:?}");
        }

        // Cruce por la frontera 170° -> -170° (debe pasar por 180°, no por 0°)
        let mid = slerp_euler([0.0, 170.0, 0.0], [0.0, -170.0, 0.0], 0.5);
        assert!(mid[1].abs() > 179.0, "esperaba ~180°, obtuve {:?}", mid);
    }

    #[test]
    fn catmull_rom_passes_through_endpoints_and_stays_bounded() {
        let p0 = [0.0, 1.5, 0.0];
        let p1 = [1.0, 1.6, 2.0];
        let p2 = [1.05, 1.6, 2.05]; // muy cerca de p1 (prueba anti-overshoot centrípeta)
        let p3 = [5.0, 2.0, -3.0];

        assert!(dist3(catmull_rom_centripetal(p0, p1, p2, p3, 0.0), p1) < 1e-5);
        assert!(dist3(catmull_rom_centripetal(p0, p1, p2, p3, 1.0), p2) < 1e-5);
        let mid = catmull_rom_centripetal(p0, p1, p2, p3, 0.5);
        assert!(dist3(mid, p1) < 0.1, "sin sobreimpulso en tramo corto: {mid:?}");
    }

    #[test]
    fn look_at_points_correctly() {
        // Cámara en (0, 1.6, 2.0) mirando al origen (0, 1.6, 0.0) -> yaw 180°, pitch 0°
        let [pitch, yaw] = look_at_angles([0.0, 1.6, 2.0], [0.0, 1.6, 0.0]);
        assert!(near(pitch, 0.0) && near(yaw.abs(), 180.0));
    }

    #[test]
    fn arc_length_segment_preserves_constant_speed_along_curve() {
        let p0 = [-2.0, 1.5, 0.0];
        let p1 = [0.0, 1.6, 2.0];
        let p2 = [3.0, 2.4, 1.0];
        let p3 = [4.0, 1.2, -3.0];
        let seg = ArcLengthSegment::new(p0, p1, p2, p3);
        assert!(seg.total_length > dist3(p1, p2));

        // Al muestrear en 10 pasos iguales de fracción de arco s, las distancias entre puntos sucesivos deben ser casi uniformes
        let mut step_lengths = Vec::new();
        for i in 0..10 {
            let a = seg.eval_at_arc_fraction(i as f32 / 10.0);
            let b = seg.eval_at_arc_fraction((i + 1) as f32 / 10.0);
            step_lengths.push(dist3(a, b));
        }
        let avg: f32 = step_lengths.iter().sum::<f32>() / step_lengths.len() as f32;
        for d in step_lengths {
            assert!(((d - avg) / avg).abs() < 0.03, "variación de rapidez > 3%: d={d}, avg={avg}");
        }
    }
}
