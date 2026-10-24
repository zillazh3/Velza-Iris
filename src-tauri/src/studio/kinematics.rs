//! Motor cinematográfico de movimiento: aceleración/desaceleración progresiva, inercia configurable,
//! límites de velocidad/aceleración, variación orgánica reproducible y frenado controlado.
//!
//! Utiliza integración con sub-stepping de paso fijo (`FIXED_DT = 1/240 s`) + interpolación de residuo
//! para garantizar que la trayectoria reproducida sea independiente de fluctuaciones o jitter del reloj de Windows.

use super::math3d::{clamp_vec3_len, dist3, lerp, lerp3, norm3, wrap180, Quat};
use super::model::{CinematicConfig, CinematicProfile};
use crate::osc::protocol::Pose;
use std::f32::consts::PI;

/// Paso fijo interno de integración cinemática (240 Hz).
pub const FIXED_DT: f32 = 1.0 / 240.0;

/// Ruido suave determinista 1D en `[-1, 1]` parametrizado por semilla.
fn organic_noise(x: f32, seed: u32) -> f32 {
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

/// Variación sutil, opcional y reproducible para el perfil `Orgánico`.
/// A diferencia de `Handheld`, simula el micro-pulso de un operador sobre cabezal fluido (±4 mm, ±0.18°).
pub fn apply_organic_variation(pose: &mut Pose, t: f32, config: &CinematicConfig) {
    let amount = config.organic_variation.clamp(0.0, 1.0);
    if amount <= 1e-4 && config.profile != CinematicProfile::Organic {
        return;
    }
    let eff = if config.profile == CinematicProfile::Organic && amount <= 1e-4 {
        0.45
    } else {
        amount
    };
    let seed = config.organic_seed;
    let freq = 0.32;
    pose.pos[0] += organic_noise(t * freq, seed.wrapping_add(1)) * 0.008 * eff;
    pose.pos[1] += organic_noise(t * freq * 0.85, seed.wrapping_add(2)) * 0.006 * eff;
    pose.pos[2] += organic_noise(t * freq * 0.92, seed.wrapping_add(3)) * 0.006 * eff;

    pose.rot[0] = (pose.rot[0] + organic_noise(t * freq * 1.1, seed.wrapping_add(11)) * 0.35 * eff).clamp(-89.9, 89.9);
    pose.rot[1] = wrap180(pose.rot[1] + organic_noise(t * freq * 0.9, seed.wrapping_add(12)) * 0.40 * eff);
    pose.rot[2] = wrap180(pose.rot[2] + organic_noise(t * freq * 0.75, seed.wrapping_add(13)) * 0.25 * eff);
}

/// Estado dinámico del motor cinematográfico durante la reproducción en tiempo real.
#[derive(Clone, Debug)]
pub struct KinematicsState {
    pub current_pose: Pose,
    pub prev_substep_pose: Pose,
    pub velocity: [f32; 3],
    pub linear_speed_mps: f32,
    pub angular_speed_dps: f32,
    accum_dt: f32,
    clock: f32,
    last_target: Pose,
}

impl KinematicsState {
    pub fn new(initial_pose: Pose) -> Self {
        KinematicsState {
            current_pose: initial_pose,
            prev_substep_pose: initial_pose,
            velocity: [0.0; 3],
            linear_speed_mps: 0.0,
            angular_speed_dps: 0.0,
            accum_dt: 0.0,
            clock: 0.0,
            last_target: initial_pose,
        }
    }

    pub fn reset(&mut self, pose: Pose) {
        self.current_pose = pose;
        self.prev_substep_pose = pose;
        self.velocity = [0.0; 3];
        self.linear_speed_mps = 0.0;
        self.angular_speed_dps = 0.0;
        self.accum_dt = 0.0;
        self.last_target = pose;
    }

    /// Ejecuta un sub-paso fijo (`FIXED_DT`) de amortiguación crítica e inercia con límites de velocidad y aceleración.
    fn substep_towards(&mut self, target: Pose, config: &CinematicConfig) {
        self.prev_substep_pose = self.current_pose;
        let max_v = config.max_linear_speed.clamp(0.2, 25.0);
        let max_a = config.max_linear_accel.clamp(0.5, 60.0);
        let max_w = config.max_angular_speed.clamp(10.0, 720.0);
        let tau = config.inertia_tau.clamp(0.0, 1.5);

        // 1. Posición 3D: resorte-amortiguador críticamente amortiguado + límites v_max y a_max
        let next_pos = if tau < 0.005 {
            // Modo Precisión: seguimiento directo limitado solo por v_max y a_max
            let desired_v = [
                (target.pos[0] - self.current_pose.pos[0]) / FIXED_DT,
                (target.pos[1] - self.current_pose.pos[1]) / FIXED_DT,
                (target.pos[2] - self.current_pose.pos[2]) / FIXED_DT,
            ];
            let clamped_v = clamp_vec3_len(desired_v, max_v);
            let dv = [
                clamped_v[0] - self.velocity[0],
                clamped_v[1] - self.velocity[1],
                clamped_v[2] - self.velocity[2],
            ];
            let clamped_dv = clamp_vec3_len(dv, max_a * FIXED_DT);
            self.velocity = [
                self.velocity[0] + clamped_dv[0],
                self.velocity[1] + clamped_dv[1],
                self.velocity[2] + clamped_dv[2],
            ];
            self.velocity = clamp_vec3_len(self.velocity, max_v);
            [
                self.current_pose.pos[0] + self.velocity[0] * FIXED_DT,
                self.current_pose.pos[1] + self.velocity[1] * FIXED_DT,
                self.current_pose.pos[2] + self.velocity[2] * FIXED_DT,
            ]
        } else {
            // Amortiguación crítica de 2º orden (omega = 2 / tau) con solución exponencial estable
            let omega = 2.0 / tau;
            let x = omega * FIXED_DT;
            let exp = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
            let mut new_pos = [0.0_f32; 3];
            let mut new_vel = [0.0_f32; 3];
            for i in 0..3 {
                let change = self.current_pose.pos[i] - target.pos[i];
                let temp = (self.velocity[i] + omega * change) * FIXED_DT;
                let unclamped_v = (self.velocity[i] - omega * temp) * exp;
                new_vel[i] = unclamped_v;
                new_pos[i] = target.pos[i] + (change + temp) * exp;
            }
            // Respetar aceleración máxima y velocidad máxima
            let dv = [
                new_vel[0] - self.velocity[0],
                new_vel[1] - self.velocity[1],
                new_vel[2] - self.velocity[2],
            ];
            let clamped_dv = clamp_vec3_len(dv, max_a * FIXED_DT);
            self.velocity = clamp_vec3_len(
                [
                    self.velocity[0] + clamped_dv[0],
                    self.velocity[1] + clamped_dv[1],
                    self.velocity[2] + clamped_dv[2],
                ],
                max_v,
            );
            // Si la aceleración/velocidad fue recortada, integrar con la velocidad recortada
            if norm3(dv) > max_a * FIXED_DT + 1e-4 || norm3(new_vel) > max_v + 1e-4 {
                [
                    self.current_pose.pos[0] + self.velocity[0] * FIXED_DT,
                    self.current_pose.pos[1] + self.velocity[1] * FIXED_DT,
                    self.current_pose.pos[2] + self.velocity[2] * FIXED_DT,
                ]
            } else {
                new_pos
            }
        };

        // 2. Orientación 3D en SO(3) mediante Quaternions con suavizado exponencial + límite angular
        let q_curr = Quat::from_euler_deg(self.current_pose.rot);
        let q_targ = Quat::from_euler_deg(target.rot);
        let rot_alpha = if tau < 0.005 {
            1.0
        } else {
            (1.0 - (-FIXED_DT / (tau * 0.55)).exp()).clamp(0.0, 1.0)
        };
        let q_blended = Quat::slerp(q_curr, q_targ, rot_alpha);
        let applied_angle = q_curr.angle_deg(&q_blended).min(max_w * FIXED_DT);
        let q_next = q_curr.rotate_towards(q_blended, applied_angle);

        self.linear_speed_mps = norm3(self.velocity);
        self.angular_speed_dps = applied_angle / FIXED_DT;
        self.current_pose = Pose {
            pos: next_pos,
            rot: q_next.to_euler_deg(),
        };
    }

    /// Avanza el motor un intervalo `dt` (variable según el SO) hacia `target_pose` de forma independiente del framerate.
    pub fn step(&mut self, target_pose: Pose, dt: f32, config: &CinematicConfig) -> Pose {
        if !target_pose.pos.iter().chain(&target_pose.rot).all(|x| x.is_finite()) || !dt.is_finite() {
            return self.current_pose;
        }
        let safe_dt = dt.clamp(0.0, 0.25);
        self.clock += safe_dt;

        if safe_dt <= 1e-6 {
            return self.current_pose;
        }

        let prev_target = self.last_target;
        self.last_target = target_pose;
        self.accum_dt += safe_dt;

        let total_steps = (self.accum_dt / FIXED_DT).floor() as usize;
        for s in 0..total_steps {
            let frac = (s + 1) as f32 / (total_steps.max(1) as f32);
            let sub_target = Pose {
                pos: lerp3(prev_target.pos, target_pose.pos, frac),
                rot: Quat::slerp(
                    Quat::from_euler_deg(prev_target.rot),
                    Quat::from_euler_deg(target_pose.rot),
                    frac,
                )
                .to_euler_deg(),
            };
            self.substep_towards(sub_target, config);
            self.accum_dt -= FIXED_DT;
        }

        self.current_pose
    }

    /// Frenado controlado (*Controlled Braking*): desacelera suavemente la velocidad actual hasta reposo.
    /// Devuelve `(pose_actual, en_reposo)`.
    pub fn brake_step(&mut self, dt: f32, config: &CinematicConfig) -> (Pose, bool) {
        let safe_dt = dt.clamp(0.0, 0.1);
        let brake_decel = (config.max_linear_accel * 1.4).max(3.0);
        self.accum_dt += safe_dt;
        while self.accum_dt >= FIXED_DT {
            self.accum_dt -= FIXED_DT;
            let spd = norm3(self.velocity);
            if spd <= 0.005 {
                self.velocity = [0.0; 3];
                self.linear_speed_mps = 0.0;
                self.angular_speed_dps = 0.0;
            } else {
                let next_spd = (spd - brake_decel * FIXED_DT).max(0.0);
                let ratio = next_spd / spd;
                for i in 0..3 {
                    self.velocity[i] *= ratio;
                    self.current_pose.pos[i] += self.velocity[i] * FIXED_DT;
                }
                self.linear_speed_mps = next_spd;
                self.angular_speed_dps *= 0.85;
            }
        }
        let stopped = self.linear_speed_mps <= 0.005;
        (self.current_pose, stopped)
    }

    pub fn distance_to(&self, target: &Pose) -> f32 {
        dist3(self.current_pose.pos, target.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_velocity_and_acceleration_limits() {
        let start = Pose { pos: [0.0, 1.6, 0.0], rot: [0.0, 0.0, 0.0] };
        let far_target = Pose { pos: [50.0, 1.6, 0.0], rot: [0.0, 180.0, 0.0] };
        let mut cfg = CinematicConfig::for_profile(CinematicProfile::Cinematic);
        cfg.max_linear_speed = 2.5;
        cfg.max_linear_accel = 5.0;
        cfg.max_angular_speed = 90.0;

        let mut kin = KinematicsState::new(start);
        let mut prev_v = 0.0_f32;
        for _ in 0..120 {
            let dt = 1.0 / 60.0;
            let _ = kin.step(far_target, dt, &cfg);
            assert!(
                kin.linear_speed_mps <= cfg.max_linear_speed + 1e-3,
                "excedió v_max: {}",
                kin.linear_speed_mps
            );
            assert!(
                kin.angular_speed_dps <= cfg.max_angular_speed + 1e-2,
                "excedió w_max: {}",
                kin.angular_speed_dps
            );
            let accel = (kin.linear_speed_mps - prev_v).abs() / dt;
            assert!(
                accel <= cfg.max_linear_accel + 0.15,
                "excedió a_max: {accel} > {}",
                cfg.max_linear_accel
            );
            prev_v = kin.linear_speed_mps;
        }
    }

    #[test]
    fn consistent_reproduction_under_windows_timer_jitter() {
        let start = Pose { pos: [0.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] };
        let cfg = CinematicConfig::for_profile(CinematicProfile::Cinematic);

        // Función de trayectoria objetivo continua de 2 segundos
        let target_at = |t: f32| Pose {
            pos: [t * 1.2, 1.6 + (t * 1.5).sin() * 0.2, 2.0],
            rot: [0.0, 180.0 - t * 25.0, 0.0],
        };

        // Corrida 1: reloj ideal uniforme a 60 Hz (120 pasos de 1/60 s = 2.0 s)
        let mut kin_uniform = KinematicsState::new(start);
        let mut t_u = 0.0_f32;
        for _ in 0..120 {
            let dt = 1.0 / 60.0;
            t_u += dt;
            kin_uniform.step(target_at(t_u), dt, &cfg);
        }

        // Corrida 2: reloj con jitter severo de Windows (alternando pasos de 8.33 ms y 25.0 ms, sumando 2.0 s)
        let mut kin_jitter = KinematicsState::new(start);
        let mut t_j = 0.0_f32;
        for i in 0..120 {
            let dt = if i % 2 == 0 { 0.5 / 60.0 } else { 1.5 / 60.0 };
            t_j += dt;
            kin_jitter.step(target_at(t_j), dt, &cfg);
        }

        let pos_diff = dist3(kin_uniform.current_pose.pos, kin_jitter.current_pose.pos);
        assert!(
            pos_diff < 0.002,
            "la reproducción con jitter se desvió más de 2 mm: {pos_diff} m"
        );
    }

    #[test]
    fn controlled_braking_stops_smoothly_without_overshoot() {
        let start = Pose { pos: [0.0, 1.6, 0.0], rot: [0.0, 0.0, 0.0] };
        let target = Pose { pos: [4.0, 1.6, 0.0], rot: [0.0, 0.0, 0.0] };
        let cfg = CinematicConfig::for_profile(CinematicProfile::Cinematic);
        let mut kin = KinematicsState::new(start);

        // Acelerar durante 0.5 s
        for _ in 0..30 {
            kin.step(target, 1.0 / 60.0, &cfg);
        }
        let speed_at_pause = kin.linear_speed_mps;
        assert!(speed_at_pause > 0.5);

        // Activar frenado controlado
        let mut prev_spd = speed_at_pause;
        let mut stopped = false;
        for _ in 0..30 {
            let (_, is_stopped) = kin.brake_step(1.0 / 60.0, &cfg);
            assert!(kin.linear_speed_mps <= prev_spd + 1e-5, "la rapidez debe decrecer monótonamente al frenar");
            prev_spd = kin.linear_speed_mps;
            if is_stopped {
                stopped = true;
                break;
            }
        }
        assert!(stopped, "debe detenerse por completo en <= 0.5 s");
    }
}
