//! Reproducción de movimientos: `Player` (estado y tiempo, puro) y `Runner` (hilo con ticker a 60 Hz).
use super::sample::{handheld, Loop, Motion, Sample};
use serde::Serialize;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Eco de Pose de VRChat llega a ~61–65 Hz como tope (docs/osc-findings.md): enviar a 60.
const HZ: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    pub sample: Sample,
    /// (Δposición m, Δrotación °) del temblor, si el movimiento lo pide.
    pub handheld: Option<([f32; 3], [f32; 3])>,
}

pub struct Player {
    motion: Motion,
    /// Tiempo dentro del movimiento (puede retroceder).
    t: f32,
    /// Tiempo total transcurrido; alimenta el temblor para que no dependa de la dirección.
    clock: f32,
    speed: f32,
    reverse: bool,
    /// Reinicia un movimiento de una sola vez en vez de terminar. No afecta a los que ya se repiten.
    looping: bool,
    state: PlayState,
}

impl Player {
    pub fn new(motion: Motion) -> Player {
        Player { motion, t: 0.0, clock: 0.0, speed: 1.0, reverse: false, looping: false, state: PlayState::Playing }
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn set_speed(&mut self, s: f32) {
        self.speed = if s.is_finite() { s.clamp(0.1, 4.0) } else { 1.0 };
    }

    /// Avanza `dt` segundos. Devuelve la muestra actual, o `None` si no está reproduciendo.
    /// Un movimiento de una sola vez entrega su última muestra y queda `Stopped`.
    pub fn tick(&mut self, dt: f32) -> Option<Output> {
        if self.state != PlayState::Playing {
            return None;
        }
        self.clock += dt;
        let step = dt * self.speed;
        self.t += if self.reverse { -step } else { step };
        if self.motion.looping == Loop::Once {
            let dur = self.motion.duration;
            if self.looping {
                self.t = self.t.rem_euclid(dur);
            } else if (!self.reverse && self.t >= dur) || (self.reverse && self.t <= 0.0) {
                self.t = self.t.clamp(0.0, dur);
                self.state = PlayState::Stopped;
            }
        }
        Some(Output {
            sample: self.motion.sample(self.t),
            handheld: self.motion.handheld.then(|| handheld(self.clock)),
        })
    }

    fn apply(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Play if self.state == PlayState::Paused => self.state = PlayState::Playing,
            Cmd::Pause if self.state == PlayState::Playing => self.state = PlayState::Paused,
            Cmd::Speed(s) => self.set_speed(s),
            Cmd::Reverse => self.reverse = !self.reverse,
            Cmd::Loop(on) => self.looping = on,
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Play,
    Pause,
    Speed(f32),
    Reverse,
    Loop(bool),
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shared {
    pub state: PlayState,
    /// Por qué se detuvo, si el sink falló (p. ej. la cámara se cerró).
    pub error: Option<String>,
}

pub struct Runner {
    pub name: String,
    tx: Sender<Cmd>,
    shared: Arc<Mutex<Shared>>,
    handle: Option<JoinHandle<()>>,
}

fn sleep_until(deadline: Instant) {
    while let Some(d) = deadline.checked_duration_since(Instant::now()) {
        if d > Duration::from_millis(1) {
            std::thread::sleep(Duration::from_millis(1));
        } else if d > Duration::from_micros(80) {
            std::thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }
}

fn run(mut player: Player, rx: Receiver<Cmd>, shared: Arc<Mutex<Shared>>, mut sink: impl FnMut(&Output) -> Result<(), String>) {
    let period = Duration::from_secs_f64(1.0 / HZ);
    let (mut next, mut last) = (Instant::now(), Instant::now());
    loop {
        loop {
            match rx.try_recv() {
                Ok(Cmd::Quit) | Err(TryRecvError::Disconnected) => return,
                Ok(cmd) => player.apply(cmd),
                Err(TryRecvError::Empty) => break,
            }
        }
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32();
        last = now;
        let mut error = None;
        if let Some(out) = player.tick(dt) {
            if let Err(e) = sink(&out) {
                error = Some(e);
            }
        }
        let state = if error.is_some() { PlayState::Stopped } else { player.state() };
        *shared.lock().unwrap() = Shared { state, error };
        if state == PlayState::Stopped {
            return;
        }
        next += period;
        if next + period < Instant::now() {
            next = Instant::now(); // nos atrasamos: no acumular ticks de recuperación
        }
        sleep_until(next);
    }
}

impl Runner {
    /// Arranca reproduciendo. `sink` recibe cada muestra; si devuelve `Err` el runner se detiene con ese error.
    pub fn start(motion: Motion, sink: impl FnMut(&Output) -> Result<(), String> + Send + 'static) -> Runner {
        let name = motion.name.clone();
        let (tx, rx) = channel();
        let shared = Arc::new(Mutex::new(Shared { state: PlayState::Playing, error: None }));
        let s = shared.clone();
        let handle = std::thread::spawn(move || run(Player::new(motion), rx, s, sink));
        Runner { name, tx, shared, handle: Some(handle) }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd); // si el hilo ya terminó, no hay nada que controlar
    }

    pub fn status(&self) -> Shared {
        self.shared.lock().unwrap().clone()
    }
}

impl Drop for Runner {
    /// Detiene el hilo (a lo sumo un tick de espera).
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::Frame;
    use crate::motion::sample::{Easing, Zoom};

    fn frame(distance: f32) -> Frame {
        Frame { distance, height: 0.0, yaw: 0.0, pitch: 0.0, roll: 0.0 }
    }

    fn motion(looping: Loop, handheld: bool) -> Motion {
        Motion {
            name: "t".into(),
            duration: 10.0,
            easing: Easing::Linear,
            looping,
            from: frame(1.0),
            to: frame(3.0),
            zoom: Zoom::Keep,
            handheld,
        }
    }

    fn distance(o: Option<Output>) -> f32 {
        o.unwrap().sample.frame.distance
    }

    #[test]
    fn advances_with_speed_and_finishes_once() {
        let mut p = Player::new(motion(Loop::Once, false));
        p.set_speed(2.0);
        assert_eq!(distance(p.tick(2.0)), 1.8); // t=4 → 1 + 2·0.4
        assert_eq!(p.state(), PlayState::Playing);
        let last = p.tick(10.0); // pasa de 10 s: entrega el final y se detiene
        assert_eq!(distance(last), 3.0);
        assert_eq!(p.state(), PlayState::Stopped);
        assert!(p.tick(1.0).is_none());
    }

    #[test]
    fn first_tick_with_zero_dt_does_not_stop() {
        let mut p = Player::new(motion(Loop::Once, false));
        assert_eq!(distance(p.tick(0.0)), 1.0);
        assert_eq!(p.state(), PlayState::Playing);
    }

    #[test]
    fn pause_resume_and_reverse() {
        let mut p = Player::new(motion(Loop::Once, false));
        p.tick(5.0);
        p.apply(Cmd::Pause);
        assert!(p.tick(1.0).is_none());
        p.apply(Cmd::Play);
        p.apply(Cmd::Reverse);
        assert_eq!(distance(p.tick(2.0)), 1.0 + 2.0 * 0.3); // t=3
        p.tick(10.0); // retrocede más allá de 0: se detiene al inicio
        assert_eq!(p.state(), PlayState::Stopped);
    }

    #[test]
    fn loop_flag_restarts_once_motions() {
        let mut p = Player::new(motion(Loop::Once, false));
        p.apply(Cmd::Loop(true));
        assert!((distance(p.tick(12.0)) - 1.4).abs() < 1e-4); // t=2 → 1 + 2·0.2
        assert_eq!(p.state(), PlayState::Playing);
    }

    #[test]
    fn handheld_only_when_requested_and_speed_clamped() {
        assert!(Player::new(motion(Loop::Repeat, false)).tick(0.1).unwrap().handheld.is_none());
        assert!(Player::new(motion(Loop::Repeat, true)).tick(0.1).unwrap().handheld.is_some());
        let mut p = Player::new(motion(Loop::Repeat, false));
        p.set_speed(100.0);
        assert_eq!(p.speed, 4.0);
        p.set_speed(f32::NAN);
        assert_eq!(p.speed, 1.0);
    }

    #[test]
    fn runner_ticks_near_60hz_and_stops_on_drop() {
        let count = Arc::new(Mutex::new(0u32));
        let c = count.clone();
        let r = Runner::start(motion(Loop::Repeat, false), move |_| {
            *c.lock().unwrap() += 1;
            Ok(())
        });
        std::thread::sleep(Duration::from_millis(500));
        let n = *count.lock().unwrap();
        assert!((25..=35).contains(&n), "ticks en 0.5 s: {n}");
        assert_eq!(r.status().state, PlayState::Playing);
        drop(r);
        let after = *count.lock().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(*count.lock().unwrap(), after, "sigue enviando tras detenerse");
    }

    #[test]
    fn runner_stops_with_sink_error() {
        let r = Runner::start(motion(Loop::Repeat, false), |_| Err("cámara cerrada".into()));
        std::thread::sleep(Duration::from_millis(100));
        let s = r.status();
        assert_eq!((s.state, s.error.as_deref()), (PlayState::Stopped, Some("cámara cerrada")));
    }
}
