//! Turnaround (ref sheet): paradas alrededor del jugador, con pausa y captura en cada una.
use crate::anchor::Frame;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Mismo encuadre que "Cuerpo completo (ref)": 3.5 m, a la altura de la cadera.
pub const DISTANCE: f32 = 3.5;
pub const HEIGHT: f32 = -0.75;
/// Focal (mm) para que un avatar de cuerpo completo llene ~90 % del alto del cuadro a 3.5 m.
/// Medido en vivo: con 45 mm ocupaba ~58 % (el sensor efectivo de VRChat es mayor que los 24 mm supuestos).
pub const ZOOM: f32 = 68.0;

/// `LookAtMe` apunta a los ojos; con la cámara en la cadera eso inclina la vista hacia arriba y corta las piernas.
/// Este offset vertical (°) nivela la mirada para encuadrar el centro del cuerpo.
pub fn look_at_y_offset() -> f32 {
    (-HEIGHT).atan2(DISTANCE).to_degrees()
}

/// Yaw de cada parada (0° = frente al jugador, + = a su derecha). 4 vistas: frente, ¾, perfil, espalda.
pub fn yaws(four_views: bool) -> Vec<f32> {
    if four_views { vec![0.0, 45.0, 90.0, 180.0] } else { (0..8).map(|i| i as f32 * 45.0).collect() }
}

pub fn frame(yaw: f32) -> Frame {
    Frame { distance: DISTANCE, height: HEIGHT, yaw, pitch: 0.0, roll: 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Salta a la parada con este yaw.
    Go(f32),
    Capture,
}

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// Pausa en cada parada antes de capturar (1.5 s según el prompt).
    pub hold: Duration,
    /// Espera tras capturar, para que VRChat termine la foto antes de mover la cámara.
    pub settle: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing { hold: Duration::from_millis(1500), settle: Duration::from_millis(300) }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub finished: bool,
    pub error: Option<String>,
}

pub struct Turnaround {
    stop: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    handle: Option<JoinHandle<()>>,
}

/// Duerme `d` en trozos cortos; devuelve `false` si se pidió detener.
fn nap(stop: &AtomicBool, d: Duration) -> bool {
    let end = std::time::Instant::now() + d;
    while let Some(left) = end.checked_duration_since(std::time::Instant::now()) {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(left.min(Duration::from_millis(25)));
    }
    !stop.load(Ordering::Relaxed)
}

impl Turnaround {
    /// Recorre `yaws`. `finish` corre siempre al terminar (también al detener o fallar): restaura los ajustes.
    pub fn start(
        yaws: Vec<f32>,
        timing: Timing,
        mut sink: impl FnMut(Action) -> Result<(), String> + Send + 'static,
        finish: impl FnOnce() + Send + 'static,
    ) -> Turnaround {
        let stop = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(Progress { total: yaws.len(), ..Progress::default() }));
        let (s, p) = (stop.clone(), progress.clone());
        let handle = std::thread::spawn(move || {
            let mut error = None;
            for (i, yaw) in yaws.into_iter().enumerate() {
                let step = sink(Action::Go(yaw))
                    .and_then(|_| if nap(&s, timing.hold) { sink(Action::Capture) } else { Err(String::new()) });
                match step {
                    Ok(()) => p.lock().unwrap().done = i + 1,
                    Err(e) => {
                        error = Some(e).filter(|e| !e.is_empty()); // cadena vacía = detenido por el usuario
                        break;
                    }
                }
                if !nap(&s, timing.settle) {
                    break;
                }
            }
            finish();
            let mut p = p.lock().unwrap();
            p.finished = true;
            p.error = error;
        });
        Turnaround { stop, progress, handle: Some(handle) }
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().unwrap().clone()
    }
}

impl Drop for Turnaround {
    /// Detiene la secuencia (a lo sumo ~25 ms de espera) y espera a que restaure los ajustes.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    const FAST: Timing = Timing { hold: Duration::from_millis(5), settle: Duration::from_millis(5) };

    fn wait_finished(t: &Turnaround) -> Progress {
        for _ in 0..200 {
            let p = t.progress();
            if p.finished {
                return p;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no terminó");
    }

    #[test]
    fn stops_are_every_45_degrees_or_four_views() {
        assert_eq!(yaws(false), [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]);
        assert_eq!(yaws(true), [0.0, 45.0, 90.0, 180.0]);
        assert_eq!(frame(90.0).distance, 3.5);
    }

    #[test]
    fn level_offset_cancels_the_upward_tilt() {
        // la cámara a -0.75 m mira hacia arriba atan(0.75/3.5) ≈ 12.1°; el offset lo compensa
        assert!((look_at_y_offset() - 12.09).abs() < 0.01);
        assert!(look_at_y_offset() <= 25.0, "dentro del rango de LookAtMeYOffset");
    }

    #[test]
    fn goes_then_captures_at_each_stop_and_restores() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let finished = Arc::new(AtomicUsize::new(0));
        let (l, f) = (log.clone(), finished.clone());
        let t = Turnaround::start(
            yaws(true),
            FAST,
            move |a| {
                l.lock().unwrap().push(a);
                Ok(())
            },
            move || {
                f.fetch_add(1, Ordering::SeqCst);
            },
        );
        let p = wait_finished(&t);
        assert_eq!((p.done, p.total, p.error), (4, 4, None));
        let expected: Vec<Action> = [0.0, 45.0, 90.0, 180.0].iter().flat_map(|y| [Action::Go(*y), Action::Capture]).collect();
        assert_eq!(*log.lock().unwrap(), expected);
        assert_eq!(finished.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dropping_stops_quickly_and_still_restores() {
        let finished = Arc::new(AtomicUsize::new(0));
        let captures = Arc::new(AtomicUsize::new(0));
        let (f, c) = (finished.clone(), captures.clone());
        let slow = Timing { hold: Duration::from_secs(5), settle: Duration::from_secs(5) };
        let t = Turnaround::start(
            yaws(false),
            slow,
            move |a| {
                if a == Action::Capture {
                    c.fetch_add(1, Ordering::SeqCst);
                }
                Ok(())
            },
            move || {
                f.fetch_add(1, Ordering::SeqCst);
            },
        );
        std::thread::sleep(Duration::from_millis(50));
        let start = Instant::now();
        drop(t);
        assert!(start.elapsed() < Duration::from_secs(1), "tardó {:?}", start.elapsed());
        assert_eq!((finished.load(Ordering::SeqCst), captures.load(Ordering::SeqCst)), (1, 0));
    }

    #[test]
    fn sink_error_stops_with_message_and_restores() {
        let finished = Arc::new(AtomicUsize::new(0));
        let f = finished.clone();
        let t = Turnaround::start(
            yaws(false),
            FAST,
            |a| if a == Action::Capture { Err("cámara cerrada".into()) } else { Ok(()) },
            move || {
                f.fetch_add(1, Ordering::SeqCst);
            },
        );
        let p = wait_finished(&t);
        assert_eq!((p.done, p.error.as_deref()), (0, Some("cámara cerrada")));
        assert_eq!(finished.load(Ordering::SeqCst), 1);
    }
}
