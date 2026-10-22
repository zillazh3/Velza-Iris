//! Movimientos como datos: `sample(motion, t)` da el encuadre (en espacio del ancla) y el zoom en el instante `t`.
use crate::anchor::Frame;
use crate::osc::protocol::in_range;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

const MOTIONS: &str = include_str!("motions.json");

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Easing {
    Linear,
    Sine,
    Cubic,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Loop {
    Once,
    Repeat,
    PingPong,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Zoom {
    /// No toca el zoom.
    #[default]
    Keep,
    Range(f32, f32),
    /// Dolly zoom: el Zoom de VRChat es una focal (mm) y el tamaño del sujeto en cuadro ∝ focal / distancia,
    /// así que `zoom = perMeter · distancia` lo mantiene constante con cualquier sensor.
    Dolly {
        #[serde(rename = "perMeter")]
        per_meter: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Motion {
    pub name: String,
    /// Segundos por tramo (de `from` a `to`).
    pub duration: f32,
    pub easing: Easing,
    #[serde(rename = "loop")]
    pub looping: Loop,
    pub from: Frame,
    pub to: Frame,
    #[serde(default)]
    pub zoom: Zoom,
    /// Añade temblor de cámara en mano encima del movimiento.
    #[serde(default)]
    pub handheld: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub frame: Frame,
    pub zoom: Option<f32>,
}

fn ease(e: Easing, p: f32) -> f32 {
    match e {
        Easing::Linear => p,
        Easing::Sine => 0.5 - 0.5 * (PI * p).cos(),
        Easing::Cubic if p < 0.5 => 4.0 * p.powi(3),
        Easing::Cubic => 1.0 - (-2.0 * p + 2.0).powi(3) / 2.0,
    }
}

fn lerp(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}

impl Motion {
    /// Progreso 0..1 del tramo en el tiempo `t` (antes de aplicar easing).
    fn progress(&self, t: f32) -> f32 {
        let u = t / self.duration;
        match self.looping {
            Loop::Once => u.clamp(0.0, 1.0),
            Loop::Repeat => u.rem_euclid(1.0),
            Loop::PingPong => {
                let u = u.rem_euclid(2.0);
                if u < 1.0 { u } else { 2.0 - u }
            }
        }
    }

    /// Un movimiento `Once` termina al cumplirse su duración; los demás no terminan solos.
    pub fn finished(&self, t: f32) -> bool {
        self.looping == Loop::Once && t >= self.duration
    }

    pub fn sample(&self, t: f32) -> Sample {
        let p = ease(self.easing, self.progress(t));
        let (a, b) = (&self.from, &self.to);
        let frame = Frame {
            distance: lerp(a.distance, b.distance, p),
            height: lerp(a.height, b.height, p),
            yaw: lerp(a.yaw, b.yaw, p),
            pitch: lerp(a.pitch, b.pitch, p),
            roll: lerp(a.roll, b.roll, p),
        };
        let zoom = match self.zoom {
            Zoom::Keep => None,
            Zoom::Range(z0, z1) => Some(lerp(z0, z1, p)),
            Zoom::Dolly { per_meter } => Some(per_meter * frame.distance),
        };
        Sample { frame, zoom }
    }

    fn validate(&self) -> Result<(), String> {
        let name = &self.name;
        if !(0.5..=300.0).contains(&self.duration) {
            return Err(format!("{name}: duración fuera de rango"));
        }
        if self.from.distance <= 0.0 || self.to.distance <= 0.0 {
            return Err(format!("{name}: distancia debe ser > 0"));
        }
        // El zoom debe caber en el rango de VRChat a lo largo de todo el movimiento.
        let zooms = [0.0, 0.25, 0.5, 0.75, 1.0].map(|p| self.sample(p * self.duration).zoom);
        match zooms.iter().flatten().find(|z| !in_range("Zoom", **z)) {
            Some(z) => Err(format!("{name}: zoom {z} fuera de rango")),
            None => Ok(()),
        }
    }
}

/// Movimientos incluidos en el binario.
pub fn motions() -> Vec<Motion> {
    let all: Vec<Motion> = serde_json::from_str(MOTIONS).expect("motions.json inválido");
    all.iter().for_each(|m| m.validate().expect("movimiento inválido"));
    all
}

// ---------- handheld ----------

/// Valor pseudoaleatorio determinista en [-1, 1] para el nodo entero `n`.
fn hash(n: i32) -> f32 {
    let mut x = n as u32;
    x = (x ^ (x >> 16)).wrapping_mul(0x85EB_CA6B);
    x = (x ^ (x >> 13)).wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x as f32 / u32::MAX as f32 * 2.0 - 1.0
}

/// Ruido de valor 1D suave (interpolación coseno entre nodos), en [-1, 1].
fn noise(x: f32) -> f32 {
    let i = x.floor();
    let s = 0.5 - 0.5 * (PI * (x - i)).cos();
    lerp(hash(i as i32), hash(i as i32 + 1), s)
}

/// Temblor de cámara en mano: ±1.5 cm y ±0.4° con ~0.3 nodos/s. Devuelve (Δposición m, Δrotación °).
pub fn handheld(t: f32) -> ([f32; 3], [f32; 3]) {
    let n = |axis: i32| noise(t * 0.3 + axis as f32 * 100.0);
    ([0, 1, 2].map(|a| n(a) * 0.015), [3, 4, 5].map(|a| n(a) * 0.4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(distance: f32, height: f32, yaw: f32) -> Frame {
        Frame { distance, height, yaw, pitch: 0.0, roll: 0.0 }
    }

    fn motion(looping: Loop, easing: Easing) -> Motion {
        Motion {
            name: "t".into(),
            duration: 10.0,
            easing,
            looping,
            from: frame(1.0, 0.0, 0.0),
            to: frame(3.0, 1.0, 90.0),
            zoom: Zoom::Keep,
            handheld: false,
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn easing_endpoints_and_midpoint() {
        for e in [Easing::Linear, Easing::Sine, Easing::Cubic] {
            assert!(near(ease(e, 0.0), 0.0) && near(ease(e, 1.0), 1.0) && near(ease(e, 0.5), 0.5), "{e:?}");
        }
        assert!(ease(Easing::Sine, 0.25) < 0.25 && ease(Easing::Cubic, 0.25) < ease(Easing::Sine, 0.25));
    }

    #[test]
    fn once_clamps_and_finishes() {
        let m = motion(Loop::Once, Easing::Linear);
        assert_eq!(m.sample(0.0).frame, m.from);
        assert_eq!(m.sample(10.0).frame, m.to);
        assert_eq!(m.sample(99.0).frame, m.to);
        assert_eq!(m.sample(-5.0).frame, m.from);
        let mid = m.sample(5.0).frame;
        assert!(near(mid.distance, 2.0) && near(mid.height, 0.5) && near(mid.yaw, 45.0));
        assert!(!m.finished(9.9) && m.finished(10.0));
        assert!(!motion(Loop::Repeat, Easing::Linear).finished(1e6));
    }

    #[test]
    fn repeat_wraps_and_pingpong_returns() {
        let r = motion(Loop::Repeat, Easing::Linear);
        assert!(near(r.sample(12.5).frame.yaw, r.sample(2.5).frame.yaw));
        assert!(near(r.sample(-2.5).frame.yaw, 67.5));
        let p = motion(Loop::PingPong, Easing::Linear);
        assert!(near(p.sample(10.0).frame.yaw, 90.0));
        assert!(near(p.sample(15.0).frame.yaw, 45.0));
        assert!(near(p.sample(20.0).frame.yaw, 0.0));
    }

    #[test]
    fn zoom_range_follows_easing() {
        let mut m = motion(Loop::Once, Easing::Sine);
        m.zoom = Zoom::Range(35.0, 28.0);
        assert_eq!(m.sample(0.0).zoom, Some(35.0));
        assert!(near(m.sample(5.0).zoom.unwrap(), 31.5));
        assert!(near(m.sample(10.0).zoom.unwrap(), 28.0));
        assert_eq!(motion(Loop::Once, Easing::Sine).sample(3.0).zoom, None);
    }

    #[test]
    fn dolly_zoom_keeps_subject_size() {
        let mut m = motion(Loop::Once, Easing::Sine);
        m.zoom = Zoom::Dolly { per_meter: 13.4 };
        for t in [0.0, 2.5, 5.0, 7.5, 10.0] {
            let s = m.sample(t);
            // tamaño en cuadro ∝ focal / distancia: debe ser constante
            assert!(near(s.zoom.unwrap() / s.frame.distance, 13.4), "t={t}");
        }
        // al alejarse la focal sube (más teleobjetivo)
        assert!(m.sample(10.0).zoom.unwrap() > m.sample(0.0).zoom.unwrap());
    }

    #[test]
    fn handheld_is_bounded_smooth_and_deterministic() {
        let mut prev = handheld(0.0);
        assert_eq!(prev, handheld(0.0));
        for i in 1..2000 {
            let cur = handheld(i as f32 * 0.016);
            assert!(cur.0.iter().all(|v| v.abs() <= 0.015 + 1e-6) && cur.1.iter().all(|v| v.abs() <= 0.4 + 1e-6));
            // a 60 Hz el salto entre muestras es pequeño (suave)
            assert!(cur.0.iter().zip(prev.0).all(|(a, b)| (a - b).abs() < 0.002), "salto en i={i}");
            prev = cur;
        }
        assert!(prev != handheld(0.0), "no debe ser constante");
    }

    #[test]
    fn built_in_motions_are_valid() {
        let all = motions();
        assert_eq!(all.len(), 8);
        let orbit = all.iter().find(|m| m.name.starts_with("Órbita")).unwrap();
        assert!(near(orbit.sample(12.0).frame.yaw, 180.0));
        assert!(all.iter().any(|m| m.handheld));
    }
}
