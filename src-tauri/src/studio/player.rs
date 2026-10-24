//! Motor de reproducción determinista de secuencias de Iris Studio a 60 Hz.
//! Soporta dos modos:
//! - Vista Previa Matemática (`live_osc = false`): evalúa la secuencia en el tiempo sin exigir conexión con VRChat.
//! - Reproducción en Vivo (`live_osc = true`): envía `/usercamera/Pose` y parámetros ópticos a VRChat a 60 Hz.

use super::evaluator::{evaluate, EvaluatedFrame};
use super::kinematics::KinematicsState;
use super::math3d::{angle_diff_deg, wrap180};
use super::model::SequenceProject;
use crate::anchor::Anchor;
use crate::motion::runner::PlayState;
use crate::osc::protocol::Pose;
use serde::Serialize;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const HZ: f64 = 60.0;

#[derive(Clone, Debug)]
pub enum StudioCmd {
    Play,
    Pause,
    Seek(f32),
    Loop(bool),
    Speed(f32),
    UpdateProject(SequenceProject, Option<Anchor>),
    Quit,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioPlaybackStatus {
    pub state: PlayState,
    pub current_time: f32,
    pub duration: f32,
    pub looping: bool,
    pub speed: f32,
    pub live_osc: bool,
    pub frame: Option<EvaluatedFrame>,
    pub error: Option<String>,
}

impl Default for StudioPlaybackStatus {
    fn default() -> Self {
        StudioPlaybackStatus {
            state: PlayState::Stopped,
            current_time: 0.0,
            duration: 10.0,
            looping: false,
            speed: 1.0,
            live_osc: false,
            frame: None,
            error: None,
        }
    }
}

pub struct StudioRunner {
    tx: Sender<StudioCmd>,
    shared: Arc<Mutex<StudioPlaybackStatus>>,
    handle: Option<JoinHandle<()>>,
}

fn sleep_until(deadline: Instant) {
    if let Some(d) = deadline.checked_duration_since(Instant::now()) {
        if d > Duration::from_millis(2) {
            std::thread::sleep(d - Duration::from_millis(2));
        }
    }
    while Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

fn run_loop(
    mut project: SequenceProject,
    mut anchor: Option<Anchor>,
    mut current_time: f32,
    mut looping: bool,
    live_osc: bool,
    rx: Receiver<StudioCmd>,
    shared: Arc<Mutex<StudioPlaybackStatus>>,
    mut sink: impl FnMut(&EvaluatedFrame, bool) -> Result<(), String>,
) {
    let period = Duration::from_secs_f64(1.0 / HZ);
    let mut state = PlayState::Playing;
    let mut braking = false;
    let mut speed = 1.0_f32;
    let (mut next, mut last) = (Instant::now(), Instant::now());

    // Emitir evaluación inicial inmediata e inicializar estado cinemático sobre la trayectoria base
    let init_frame = evaluate(&project, anchor.as_ref(), current_time);
    let mut kinematics = KinematicsState::new(init_frame.base_pose);
    let mut init_err = None;
    if let Err(e) = sink(&init_frame, live_osc) {
        init_err = Some(e);
        state = PlayState::Stopped;
    }
    *shared.lock().unwrap() = StudioPlaybackStatus {
        state,
        current_time,
        duration: project.duration,
        looping,
        speed,
        live_osc,
        frame: Some(init_frame),
        error: init_err,
    };
    if state == PlayState::Stopped {
        return;
    }

    loop {
        let mut force_eval = false;
        loop {
            match rx.try_recv() {
                Ok(StudioCmd::Quit) | Err(TryRecvError::Disconnected) => return,
                Ok(StudioCmd::Play) => {
                    braking = false;
                    if state != PlayState::Playing {
                        if current_time >= project.duration {
                            current_time = 0.0;
                            let f0 = evaluate(&project, anchor.as_ref(), 0.0);
                            kinematics.reset(f0.base_pose);
                        }
                        state = PlayState::Playing;
                        last = Instant::now();
                        force_eval = true;
                    }
                }
                Ok(StudioCmd::Pause) => {
                    state = PlayState::Paused;
                    braking = live_osc && kinematics.linear_speed_mps > 0.02;
                    force_eval = true;
                }
                Ok(StudioCmd::Seek(t)) => {
                    braking = false;
                    current_time = t.clamp(0.0, project.duration);
                    let f_seek = evaluate(&project, anchor.as_ref(), current_time);
                    kinematics.reset(f_seek.base_pose);
                    last = Instant::now();
                    force_eval = true;
                }
                Ok(StudioCmd::Loop(on)) => {
                    looping = on;
                }
                Ok(StudioCmd::Speed(s)) => {
                    if s.is_finite() {
                        speed = s.clamp(0.1, 4.0);
                    }
                }
                Ok(StudioCmd::UpdateProject(p, a)) => {
                    project = p;
                    anchor = a;
                    current_time = current_time.clamp(0.0, project.duration);
                    force_eval = true;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32();
        last = now;

        let mut error = None;
        if state == PlayState::Playing {
            current_time += dt * speed;
            let dur = project.duration.max(0.1);
            if current_time >= dur {
                if looping {
                    current_time = current_time.rem_euclid(dur);
                } else {
                    current_time = dur;
                    state = PlayState::Stopped;
                }
            }
            let mut frame = evaluate(&project, anchor.as_ref(), current_time);
            // Aplicar filtro cinemático a la trayectoria base (inercia del perfil y límites v_max / a_max)
            // y preservar intacto el delta aditivo de las pistas de efectos (FX Layers)
            if state == PlayState::Playing {
                let fx_dp = [
                    frame.final_pose.pos[0] - frame.base_pose.pos[0],
                    frame.final_pose.pos[1] - frame.base_pose.pos[1],
                    frame.final_pose.pos[2] - frame.base_pose.pos[2],
                ];
                let fx_dr = [
                    frame.final_pose.rot[0] - frame.base_pose.rot[0],
                    angle_diff_deg(frame.base_pose.rot[1], frame.final_pose.rot[1]),
                    angle_diff_deg(frame.base_pose.rot[2], frame.final_pose.rot[2]),
                ];
                let kin_base = kinematics.step(frame.base_pose, dt, &project.cinematic);
                frame.base_pose = kin_base;
                frame.final_pose = Pose {
                    pos: [
                        kin_base.pos[0] + fx_dp[0],
                        kin_base.pos[1] + fx_dp[1],
                        kin_base.pos[2] + fx_dp[2],
                    ],
                    rot: [
                        (kin_base.rot[0] + fx_dr[0]).clamp(-89.9, 89.9),
                        wrap180(kin_base.rot[1] + fx_dr[1]),
                        wrap180(kin_base.rot[2] + fx_dr[2]),
                    ],
                };
            } else {
                kinematics.reset(frame.base_pose);
            }
            if let Err(e) = sink(&frame, live_osc) {
                error = Some(e);
                state = PlayState::Stopped;
            }
            *shared.lock().unwrap() = StudioPlaybackStatus {
                state,
                current_time,
                duration: project.duration,
                looping,
                speed,
                live_osc,
                frame: Some(frame),
                error,
            };
            if state == PlayState::Stopped {
                return;
            }
        } else if braking {
            let (braked_pose, is_stopped) = kinematics.brake_step(dt, &project.cinematic);
            let mut frame = evaluate(&project, anchor.as_ref(), current_time);
            frame.final_pose = braked_pose;
            frame.linear_speed_mps = kinematics.linear_speed_mps;
            frame.angular_speed_dps = kinematics.angular_speed_dps;
            if is_stopped {
                braking = false;
            }
            if let Err(e) = sink(&frame, live_osc) {
                error = Some(e);
                state = PlayState::Stopped;
                braking = false;
            }
            *shared.lock().unwrap() = StudioPlaybackStatus {
                state,
                current_time,
                duration: project.duration,
                looping,
                speed,
                live_osc,
                frame: Some(frame),
                error,
            };
        } else if force_eval {
            let frame = evaluate(&project, anchor.as_ref(), current_time);
            if let Err(e) = sink(&frame, live_osc) {
                error = Some(e);
                state = PlayState::Stopped;
            }
            *shared.lock().unwrap() = StudioPlaybackStatus {
                state,
                current_time,
                duration: project.duration,
                looping,
                speed,
                live_osc,
                frame: Some(frame),
                error,
            };
        }

        next += period;
        if next + period < Instant::now() {
            next = Instant::now();
        }
        sleep_until(next);
    }
}

impl StudioRunner {
    pub fn start(
        project: SequenceProject,
        anchor: Option<Anchor>,
        start_time: f32,
        looping: bool,
        live_osc: bool,
        sink: impl FnMut(&EvaluatedFrame, bool) -> Result<(), String> + Send + 'static,
    ) -> StudioRunner {
        let (tx, rx) = channel();
        let start_t = start_time.clamp(0.0, project.duration);
        let shared = Arc::new(Mutex::new(StudioPlaybackStatus {
            state: PlayState::Playing,
            current_time: start_t,
            duration: project.duration,
            looping,
            speed: 1.0,
            live_osc,
            frame: None,
            error: None,
        }));
        let s = shared.clone();
        let handle = std::thread::spawn(move || {
            run_loop(project, anchor, start_t, looping, live_osc, rx, s, sink);
        });
        StudioRunner { tx, shared, handle: Some(handle) }
    }

    pub fn send(&self, cmd: StudioCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> StudioPlaybackStatus {
        self.shared.lock().unwrap().clone()
    }
}

impl Drop for StudioRunner {
    fn drop(&mut self) {
        let _ = self.tx.send(StudioCmd::Quit);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc::protocol::Pose;
    use crate::studio::model::{Keyframe, KnotMode, LookTrackPoint, ReferenceSpace, Transition};

    fn demo_project() -> SequenceProject {
        let mut p = SequenceProject::new("test", "Demo", 0.25);
        p.space = ReferenceSpace::World;
        p.keyframes.push(Keyframe {
            id: "a".into(),
            label: "Punto A".into(),
            time: 0.0,
            raw_pose: Pose { pos: [0.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] },
            local_pose: None,
            captured_at_ms: 0,
            transition: Transition::Linear,
            knot_mode: KnotMode::Stop,
            look: LookTrackPoint::default(),
        });
        p.keyframes.push(Keyframe {
            id: "b".into(),
            label: "Punto B".into(),
            time: 0.25,
            raw_pose: Pose { pos: [1.0, 1.6, 2.0], rot: [0.0, 180.0, 0.0] },
            local_pose: None,
            captured_at_ms: 0,
            transition: Transition::Linear,
            knot_mode: KnotMode::Stop,
            look: LookTrackPoint::default(),
        });
        p
    }

    #[test]
    fn preview_runner_plays_to_completion_and_stops_cleanly() {
        let runner = StudioRunner::start(demo_project(), None, 0.0, false, false, |_, _| Ok(()));
        std::thread::sleep(Duration::from_millis(320));
        let st = runner.status();
        assert_eq!(st.state, PlayState::Stopped);
        assert!((st.current_time - 0.25).abs() < 1e-3);
        assert!((st.frame.unwrap().final_pose.pos[0] - 1.0).abs() < 1e-3);
    }
}
