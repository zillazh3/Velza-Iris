//! Estado de la cámara (fuente de verdad) alimentado por los ecos de VRChat.
use crate::osc::protocol::{kind, Kind, Pose, Value};
use crate::osc::Osc;
use rosc::{OscMessage, OscType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Opened,
    Closed,
}

/// Eventos estructurados recibidos por OSC desde VRChat para alimentar al gestor de anclaje (`AnchorManager`).
#[derive(Clone, Debug, PartialEq)]
pub enum CameraOscEvent {
    Mode {
        prev_mode: u8,
        new_mode: u8,
        timestamp_ms: u64,
    },
    Pose {
        pose: Pose,
        seq: u64,
        timestamp_ms: u64,
    },
    LookAtMe {
        enabled: bool,
        offsets_xy: Option<[f32; 2]>,
        timestamp_ms: u64,
    },
}

/// Instantánea verificada del estado de cámara para captura de puntos y diagnóstico UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraSnapshot {
    pub open: bool,
    /// Modo actual reportado por `/usercamera/Mode` (`0` = Off, `1` = Photo, `2` = Stream, `3..=6` = otros).
    #[serde(default)]
    pub mode: u8,
    /// `true` si el puerto UDP 9001 está recibiendo ecos; `false` si opera en modo Control Abierto.
    #[serde(default = "default_true")]
    pub rx_available: bool,
    pub pose: Option<Pose>,
    /// `true` si llegó al menos una Pose desde la última vez que se abrió la cámara.
    pub pose_fresh: bool,
    /// Contador monótono de paquetes `/usercamera/Pose` recibidos.
    pub pose_seq: u64,
    /// Timestamp (ms epoch) del último paquete `Pose` recibido de VRChat.
    pub pose_epoch_ms: Option<u64>,
    pub zoom: Option<f32>,
    pub exposure: Option<f32>,
    pub focal_distance: Option<f32>,
    pub aperture: Option<f32>,
    pub look_at_me: Option<bool>,
    pub smooth_movement: Option<bool>,
    pub streaming: Option<bool>,
}

impl Default for CameraSnapshot {
    fn default() -> Self {
        CameraSnapshot {
            open: false,
            mode: 0,
            rx_available: true,
            pose: None,
            pose_fresh: false,
            pose_seq: 0,
            pose_epoch_ms: None,
            zoom: None,
            exposure: None,
            focal_distance: None,
            aperture: None,
            look_at_me: None,
            smooth_movement: None,
            streaming: None,
        }
    }
}

fn default_true() -> bool {
    true
}

pub struct State {
    pub open: bool,
    pub mode: u8,
    pub rx_available: bool,
    pub pose: Option<Pose>,
    pub pose_fresh: bool,
    pub pose_seq: u64,
    pub pose_epoch_ms: Option<u64>,
    /// Último valor visto por endpoint (eco de VRChat).
    pub values: HashMap<String, Value>,
    /// Último valor enviado por endpoint para deduplicar tráfico óptico redundante a 60 Hz.
    pub last_sent: HashMap<String, Value>,
}

impl Default for State {
    fn default() -> Self {
        State {
            open: false,
            mode: 0,
            rx_available: true,
            pose: None,
            pose_fresh: false,
            pose_seq: 0,
            pose_epoch_ms: None,
            values: HashMap::new(),
            last_sent: HashMap::new(),
        }
    }
}

fn values_close_enough(a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::F(x), Value::F(y)) => (x - y).abs() < 1e-3,
        (Value::I(x), Value::I(y)) => x == y,
        (Value::B(x), Value::B(y)) => x == y,
        _ => false,
    }
}

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl State {
    /// `Mode 0` = cerrada, `Mode ≠ 0` = abierta; un `Pose` también implica abierta
    /// (VRChat no lo emite con la cámara cerrada). Devuelve el cambio abierta/cerrada, si lo hubo.
    pub fn apply(&mut self, m: &OscMessage) -> Option<Change> {
        self.apply_with_event(m, now_epoch_ms()).0
    }

    /// Aplica un mensaje OSC con marca temporal explícita y devuelve tanto el cambio `Opened`/`Closed`
    /// como el evento tipado `CameraOscEvent` para el gestor de anclaje (`AnchorManager`).
    pub fn apply_with_event(
        &mut self,
        m: &OscMessage,
        now_ms: u64,
    ) -> (Option<Change>, Option<CameraOscEvent>) {
        let Some(name) = m.addr.strip_prefix("/usercamera/") else {
            return (None, None);
        };
        if name == "Pose" {
            let Some(parsed_pose) = Pose::from_args(&m.args) else {
                return (None, None);
            };
            self.pose = Some(parsed_pose);
            self.pose_fresh = true;
            self.pose_seq = self.pose_seq.wrapping_add(1);
            self.pose_epoch_ms = Some(now_ms);
            if self.mode == 0 {
                self.mode = 1;
            }
            let was = std::mem::replace(&mut self.open, true);
            let change = if !was {
                self.last_sent.clear();
                Some(Change::Opened)
            } else {
                None
            };
            let ev = CameraOscEvent::Pose {
                pose: parsed_pose,
                seq: self.pose_seq,
                timestamp_ms: now_ms,
            };
            return (change, Some(ev));
        }

        let Some(k) = kind(name) else {
            return (None, None);
        };
        let Some(first_arg) = m.args.first() else {
            return (None, None);
        };
        let v = match (k, first_arg) {
            (Kind::Float(..), OscType::Float(f)) if f.is_finite() => Value::F(*f),
            (Kind::Int(..), OscType::Int(i)) => Value::I(*i),
            (Kind::Bool | Kind::Trigger, OscType::Bool(b)) => Value::B(*b),
            _ => return (None, None),
        };
        self.values.insert(name.to_string(), v);
        // Si el usuario cambió un slider en VRChat, actualizamos también last_sent para permitir re-sobrescribirlo
        self.last_sent.insert(name.to_string(), v);

        if matches!(name, "LookAtMe" | "LookAtMeXOffset" | "LookAtMeYOffset") {
            let enabled = match self.values.get("LookAtMe") {
                Some(Value::B(b)) => *b,
                _ => false,
            };
            let x_off = match self.values.get("LookAtMeXOffset") {
                Some(Value::F(f)) => *f,
                _ => 0.0,
            };
            let y_off = match self.values.get("LookAtMeYOffset") {
                Some(Value::F(f)) => *f,
                _ => 0.0,
            };
            let ev = CameraOscEvent::LookAtMe {
                enabled,
                offsets_xy: Some([x_off, y_off]),
                timestamp_ms: now_ms,
            };
            return (None, Some(ev));
        }

        if name != "Mode" {
            return (None, None);
        }

        let Value::I(raw_mode) = v else {
            return (None, None);
        };
        let prev_mode = self.mode;
        let new_mode = raw_mode.clamp(0, 6) as u8;
        self.mode = new_mode;
        let is_open = new_mode != 0;
        if !is_open {
            // Al cerrarse la cámara, la pose anterior deja de ser fresca y limpiamos caché de envíos.
            self.pose_fresh = false;
            self.last_sent.clear();
        }
        let was = std::mem::replace(&mut self.open, is_open);
        let change = match (was, is_open) {
            (false, true) => {
                self.last_sent.clear();
                Some(Change::Opened)
            }
            (true, false) => Some(Change::Closed),
            _ => None,
        };
        let ev = CameraOscEvent::Mode {
            prev_mode,
            new_mode,
            timestamp_ms: now_ms,
        };
        (change, Some(ev))
    }

    pub fn snapshot(&self) -> CameraSnapshot {
        let get_f = |k: &str| match self.values.get(k).or_else(|| self.last_sent.get(k)) {
            Some(Value::F(v)) => Some(*v),
            _ => None,
        };
        let get_b = |k: &str| match self.values.get(k).or_else(|| self.last_sent.get(k)) {
            Some(Value::B(v)) => Some(*v),
            _ => None,
        };
        CameraSnapshot {
            open: self.open,
            mode: self.mode,
            rx_available: self.rx_available,
            pose: self.pose,
            pose_fresh: self.pose_fresh,
            pose_seq: self.pose_seq,
            pose_epoch_ms: self.pose_epoch_ms,
            zoom: get_f("Zoom"),
            exposure: get_f("Exposure"),
            focal_distance: get_f("FocalDistance"),
            aperture: get_f("Aperture"),
            look_at_me: get_b("LookAtMe"),
            smooth_movement: get_b("SmoothMovement"),
            streaming: get_b("Streaming"),
        }
    }

    /// `Mode` y `Pose` con la cámara cerrada no hacen nada, pero `Mode` sí devuelve eco: si la app lo
    /// enviara, el estado creería que está abierta. Por eso se bloquean aquí (salvo cuando `rx_available` es `false`
    /// y operamos en modo de Control Abierto).
    fn check_writable(&self, name: &str) -> Result<(), String> {
        if self.rx_available && !self.open && matches!(name, "Mode" | "Pose") {
            return Err("cámara cerrada: ábrela en VRChat".into());
        }
        Ok(())
    }

    /// Determina si debemos enviar el valor por UDP o si es idéntico al último enviado (evita inundar a 60 Hz).
    pub fn should_send(&mut self, name: &str, value: Value) -> Result<bool, String> {
        self.check_writable(name)?;
        if matches!(kind(name), Some(Kind::Trigger)) {
            return Ok(true);
        }
        if let Some(prev) = self.last_sent.get(name).copied() {
            if values_close_enough(prev, value) {
                return Ok(false);
            }
        }
        self.last_sent.insert(name.to_string(), value);
        Ok(true)
    }
}

/// Valores por defecto de VRChat (tabla de CLAUDE.md; Lightness 50 = máximo medido). No incluye `Mode`,
/// `Lock` ni otras preferencias del usuario.
pub const DEFAULTS: &[(&str, Value)] = &[
    ("Zoom", Value::F(45.0)),
    ("Exposure", Value::F(0.0)),
    ("FocalDistance", Value::F(1.5)),
    ("Aperture", Value::F(15.0)),
    ("Hue", Value::F(120.0)),
    ("Saturation", Value::F(100.0)),
    ("Lightness", Value::F(50.0)),
    ("LookAtMeXOffset", Value::F(0.0)),
    ("LookAtMeYOffset", Value::F(0.0)),
    // En VRChat (/usercamera/FlySpeed, ver docs/oscquery-usercamera.json), 0.2 corresponde al factor
    // por defecto 3.0 en la UI (3.0 / 15.0 = 0.2); enviar 3.0 satura el slider en 15.0.
    ("FlySpeed", Value::F(0.2)),
    ("TurnSpeed", Value::F(1.0)),
    ("SmoothingStrength", Value::F(5.0)),
    ("PhotoRate", Value::F(1.0)),
    ("Duration", Value::F(2.0)),
    ("LookAtMe", Value::B(false)),
    ("AutoLevelRoll", Value::B(false)),
    ("AutoLevelPitch", Value::B(false)),
    ("SmoothMovement", Value::B(false)),
    ("GreenScreen", Value::B(false)),
    ("Streaming", Value::B(false)),
    ("CameraEars", Value::B(false)),
    ("ShowFocus", Value::B(false)),
    ("ShowUIInCamera", Value::B(false)),
    ("LocalPlayer", Value::B(true)),
    ("RemotePlayer", Value::B(true)),
    ("Environment", Value::B(true)),
    ("OrientationIsLandscape", Value::B(true)),
];

pub struct Camera {
    osc: Osc,
    pub state: Arc<Mutex<State>>,
}

impl Camera {
    /// `on_change` corre en el hilo de recepción: úsalo para reaplicar el preset al abrirse.
    pub fn start(target: SocketAddr, on_change: impl Fn(Change) + Send + 'static) -> Result<Camera, String> {
        Self::start_with_events(target, move |change, _ev| {
            if let Some(c) = change {
                on_change(c);
            }
        })
    }

    /// Inicia el cliente y escucha OSC invocando `on_event(change, osc_event)` por cada mensaje relevante.
    pub fn start_with_events(
        target: SocketAddr,
        on_event: impl Fn(Option<Change>, Option<CameraOscEvent>) + Send + 'static,
    ) -> Result<Camera, String> {
        let state = Arc::new(Mutex::new(State::default()));
        let s = state.clone();
        let osc = Osc::start(target, move |m| {
            let now_ms = now_epoch_ms();
            let (change, ev) = s.lock().unwrap().apply_with_event(&m, now_ms);
            if change.is_some() || ev.is_some() {
                on_event(change, ev);
            }
        })?;
        {
            let mut st = state.lock().unwrap();
            st.rx_available = osc.rx_bound;
            if !osc.rx_bound {
                // En modo Control Abierto (sin escucha en 9001), asumimos cámara abierta para no bloquear envíos.
                st.open = true;
            }
        }
        Ok(Camera { osc, state })
    }

    pub fn set(&self, name: &str, value: Value) -> Result<(), String> {
        if !self.state.lock().unwrap().should_send(name, value)? {
            return Ok(());
        }
        self.osc.set(name, value)
    }

    pub fn pose(&self, pose: &Pose) -> Result<(), String> {
        self.state.lock().unwrap().check_writable("Pose")?;
        self.osc.pose(pose)
    }

    pub fn reset(&self) -> Result<(), String> {
        self.state.lock().unwrap().last_sent.clear();
        DEFAULTS.iter().try_for_each(|(n, v)| self.set(n, *v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(name: &str, args: Vec<OscType>) -> OscMessage {
        OscMessage { addr: format!("/usercamera/{name}"), args }
    }

    #[test]
    fn tracks_open_close_and_values() {
        let mut s = State::default();
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(0)])), None);
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(1)])), Some(Change::Opened));
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(2)])), None);
        assert_eq!(s.apply(&msg("Zoom", vec![OscType::Float(30.0)])), None);
        assert_eq!(s.values["Zoom"], Value::F(30.0));
        assert_eq!(s.apply(&msg("Mode", vec![OscType::Int(0)])), Some(Change::Closed));
    }

    #[test]
    fn pose_implies_open() {
        let mut s = State::default();
        let pose: Vec<_> = [1.0, 1.5, 2.0, 10.0, 90.0, 0.0].iter().map(|v| OscType::Float(*v)).collect();
        assert_eq!(s.apply(&msg("Pose", pose)), Some(Change::Opened));
        assert_eq!(s.pose.unwrap().pos, [1.0, 1.5, 2.0]);
        // mensajes ajenos o malformados se ignoran
        assert_eq!(s.apply(&msg("Pose", vec![OscType::Float(1.0)])), None);
        assert_eq!(s.apply(&msg("Zoom", vec![OscType::Bool(true)])), None);
        assert_eq!(s.apply(&OscMessage { addr: "/avatar/x".into(), args: vec![] }), None);
    }

    #[test]
    fn defaults_are_valid_and_in_range() {
        for (n, v) in DEFAULTS {
            assert!(crate::osc::protocol::encode(n, *v).is_ok(), "{n}");
            if let Value::F(f) = v {
                assert!(crate::osc::protocol::in_range(n, *f), "{n}={f} fuera de rango");
            }
        }
    }

    #[test]
    fn blocks_mode_and_pose_while_closed() {
        let mut s = State::default();
        assert!(s.check_writable("Mode").is_err());
        assert!(s.check_writable("Pose").is_err());
        assert!(s.check_writable("Zoom").is_ok());
        s.open = true;
        assert!(s.check_writable("Mode").is_ok());
    }

    #[test]
    fn deduplicates_identical_slider_writes_and_allows_triggers() {
        let mut s = State::default();
        s.open = true;
        assert!(s.should_send("Zoom", Value::F(45.0)).unwrap());
        assert!(!s.should_send("Zoom", Value::F(45.0002)).unwrap());
        assert!(s.should_send("Zoom", Value::F(46.0)).unwrap());
        // Los triggers (Capture) siempre se envían
        assert!(s.should_send("Capture", Value::B(true)).unwrap());
        assert!(s.should_send("Capture", Value::B(true)).unwrap());
    }

    #[test]
    fn invalidates_pose_freshness_on_close_until_new_pose() {
        let mut s = State::default();
        assert!(!s.snapshot().pose_fresh);
        let pose: Vec<_> = [1.0, 1.5, 2.0, 10.0, 90.0, 0.0].iter().map(|v| OscType::Float(*v)).collect();
        s.apply(&msg("Pose", pose.clone()));
        assert!(s.snapshot().pose_fresh);
        assert_eq!(s.snapshot().pose_seq, 1);
        // Cerrar cámara invalida la frescura de la pose previa
        s.apply(&msg("Mode", vec![OscType::Int(0)]));
        assert!(!s.snapshot().pose_fresh);
        // Reabrir por Mode no hace fresca la pose antigua hasta recibir un nuevo paquete Pose
        s.apply(&msg("Mode", vec![OscType::Int(1)]));
        assert!(!s.snapshot().pose_fresh);
        s.apply(&msg("Pose", pose));
        assert!(s.snapshot().pose_fresh);
        assert_eq!(s.snapshot().pose_seq, 2);
    }

    #[test]
    fn snapshot_tracks_boolean_toggles() {
        let mut s = State::default();
        assert_eq!(s.snapshot().look_at_me, None);
        assert_eq!(s.snapshot().streaming, None);
        s.apply(&msg("LookAtMe", vec![OscType::Bool(true)]));
        s.apply(&msg("Streaming", vec![OscType::Bool(false)]));
        s.apply(&msg("SmoothMovement", vec![OscType::Bool(true)]));
        assert_eq!(s.snapshot().look_at_me, Some(true));
        assert_eq!(s.snapshot().streaming, Some(false));
        assert_eq!(s.snapshot().smooth_movement, Some(true));
    }

    #[test]
    fn emits_structured_camera_osc_events_for_mode_pose_and_look_at_me() {
        let mut s = State::default();
        let (ch1, ev1) = s.apply_with_event(&msg("Mode", vec![OscType::Int(2)]), 1000);
        assert_eq!(ch1, Some(Change::Opened));
        assert_eq!(
            ev1,
            Some(CameraOscEvent::Mode {
                prev_mode: 0,
                new_mode: 2,
                timestamp_ms: 1000
            })
        );
        assert_eq!(s.snapshot().mode, 2);

        let (ch2, ev2) = s.apply_with_event(&msg("LookAtMe", vec![OscType::Bool(true)]), 1010);
        assert_eq!(ch2, None);
        assert_eq!(
            ev2,
            Some(CameraOscEvent::LookAtMe {
                enabled: true,
                offsets_xy: Some([0.0, 0.0]),
                timestamp_ms: 1010
            })
        );
    }
}
