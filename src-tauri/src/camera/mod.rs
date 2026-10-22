//! Estado de la cámara (fuente de verdad) alimentado por los ecos de VRChat.
use crate::osc::protocol::{kind, Kind, Pose, Value};
use crate::osc::Osc;
use rosc::{OscMessage, OscType};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Opened,
    Closed,
}

#[derive(Default)]
pub struct State {
    pub open: bool,
    pub pose: Option<Pose>,
    /// Último valor visto por endpoint (eco de VRChat).
    pub values: HashMap<String, Value>,
}

impl State {
    /// `Mode 0` = cerrada, `Mode ≠ 0` = abierta; un `Pose` también implica abierta
    /// (VRChat no lo emite con la cámara cerrada). Devuelve el cambio abierta/cerrada, si lo hubo.
    pub fn apply(&mut self, m: &OscMessage) -> Option<Change> {
        let name = m.addr.strip_prefix("/usercamera/")?;
        let open = if name == "Pose" {
            self.pose = Some(Pose::from_args(&m.args)?);
            true
        } else {
            let v = match (kind(name)?, m.args.first()?) {
                (Kind::Float(..), OscType::Float(f)) if f.is_finite() => Value::F(*f),
                (Kind::Int(..), OscType::Int(i)) => Value::I(*i),
                (Kind::Bool | Kind::Trigger, OscType::Bool(b)) => Value::B(*b),
                _ => return None,
            };
            self.values.insert(name.to_string(), v);
            if name != "Mode" {
                return None;
            }
            v != Value::I(0)
        };
        let was = std::mem::replace(&mut self.open, open);
        match (was, open) {
            (false, true) => Some(Change::Opened),
            (true, false) => Some(Change::Closed),
            _ => None,
        }
    }

    /// `Mode` y `Pose` con la cámara cerrada no hacen nada, pero `Mode` sí devuelve eco: si la app lo
    /// enviara, el estado creería que está abierta. Por eso se bloquean aquí.
    fn check_writable(&self, name: &str) -> Result<(), String> {
        if !self.open && matches!(name, "Mode" | "Pose") {
            return Err("cámara cerrada: ábrela en VRChat".into());
        }
        Ok(())
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
    ("FlySpeed", Value::F(3.0)),
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
        let state = Arc::new(Mutex::new(State::default()));
        let s = state.clone();
        let osc = Osc::start(target, move |m| {
            let change = s.lock().unwrap().apply(&m);
            change.into_iter().for_each(&on_change);
        })?;
        Ok(Camera { osc, state })
    }

    pub fn set(&self, name: &str, value: Value) -> Result<(), String> {
        self.state.lock().unwrap().check_writable(name)?;
        self.osc.set(name, value)
    }

    pub fn pose(&self, pose: &Pose) -> Result<(), String> {
        self.state.lock().unwrap().check_writable("Pose")?;
        self.osc.pose(pose)
    }

    pub fn reset(&self) -> Result<(), String> {
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
}
