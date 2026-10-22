//! Endpoints `/usercamera/*`, sus rangos (clamp) y el formato de Pose, según `docs/osc-findings.md`.
use rosc::OscType;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Float(f32, f32),
    Int(i32, i32),
    Bool,
    /// Acción sin valor (`Capture`, `Close`…): se envía `true`.
    Trigger,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    F(f32),
    I(i32),
    B(bool),
}

use Kind::*;

/// OSCQuery reporta todos los rangos `null`: salen de aquí (Zoom 20–300 medido en M0).
const ENDPOINTS: &[(&str, Kind)] = &[
    ("Mode", Int(0, 6)),
    ("Zoom", Float(20.0, 300.0)),
    ("Exposure", Float(-10.0, 4.0)),
    ("FocalDistance", Float(0.0, 10.0)),
    ("Aperture", Float(1.4, 32.0)),
    ("Hue", Float(0.0, 360.0)),
    ("Saturation", Float(0.0, 100.0)),
    ("Lightness", Float(0.0, 50.0)),
    ("LookAtMeXOffset", Float(-25.0, 25.0)),
    ("LookAtMeYOffset", Float(-25.0, 25.0)),
    ("FlySpeed", Float(0.1, 15.0)),
    ("TurnSpeed", Float(0.1, 5.0)),
    ("SmoothingStrength", Float(0.1, 10.0)),
    ("PhotoRate", Float(0.1, 2.0)),
    ("Duration", Float(0.1, 60.0)),
    ("ShowUIInCamera", Bool),
    ("Lock", Bool),
    ("LocalPlayer", Bool),
    ("RemotePlayer", Bool),
    ("Environment", Bool),
    ("GreenScreen", Bool),
    ("SmoothMovement", Bool),
    ("LookAtMe", Bool),
    ("AutoLevelRoll", Bool),
    ("AutoLevelPitch", Bool),
    ("Flying", Bool),
    ("TriggerTakesPhotos", Bool),
    ("DollyPathsStayVisible", Bool),
    ("CameraEars", Bool),
    ("ShowFocus", Bool),
    ("Streaming", Bool),
    ("RollWhileFlying", Bool),
    ("OrientationIsLandscape", Bool),
    ("Capture", Trigger),
    ("CaptureDelayed", Trigger),
    ("Close", Trigger),
];

pub fn kind(name: &str) -> Option<Kind> {
    ENDPOINTS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
}

/// Valida `value` contra el tipo del endpoint, recorta flotantes/enteros a su rango y devuelve (dirección OSC, args).
pub fn encode(name: &str, value: Value) -> Result<(String, Vec<OscType>), String> {
    let arg = match (kind(name).ok_or(format!("endpoint desconocido: {name}"))?, value) {
        (Float(lo, hi), Value::F(v)) if v.is_finite() => OscType::Float(v.clamp(lo, hi)),
        (Int(lo, hi), Value::I(v)) => OscType::Int(v.clamp(lo, hi)),
        (Bool, Value::B(b)) => OscType::Bool(b),
        (Trigger, _) => OscType::Bool(true),
        (k, v) => return Err(format!("{name}: valor {v:?} no válido para {k:?}")),
    };
    Ok((format!("/usercamera/{name}"), vec![arg]))
}

/// Pose en mundo: posición en metros, rotación Euler en grados (pitch, yaw, roll).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

/// Lleva un ángulo a (-180, 180].
fn wrap180(a: f32) -> f32 {
    let r = a.rem_euclid(360.0);
    if r > 180.0 { r - 360.0 } else { r }
}

impl Pose {
    pub const ADDR: &'static str = "/usercamera/Pose";

    /// Recibe 6 floats (VRChat entrega los ángulos en [0,360)). El pitch queda en [-90, 90].
    pub fn from_args(args: &[OscType]) -> Option<Pose> {
        let v: Vec<f32> = args.iter().filter_map(|a| a.clone().float()).collect();
        let v: [f32; 6] = v.try_into().ok()?;
        Some(Pose { pos: [v[0], v[1], v[2]], rot: [wrap180(v[3]), wrap180(v[4]), wrap180(v[5])] })
    }

    /// Pitch recortado a ±90° (más se reinterpreta como otro Euler, ver findings). Rechaza NaN/inf.
    pub fn to_args(&self) -> Result<Vec<OscType>, String> {
        let all = self.pos.iter().chain(&self.rot);
        if !all.clone().all(|v| v.is_finite()) {
            return Err("Pose con valores no finitos".into());
        }
        let [pitch, yaw, roll] = self.rot;
        let rot = [wrap180(pitch).clamp(-90.0, 90.0), yaw, roll];
        Ok(self.pos.iter().chain(&rot).map(|v| OscType::Float(*v)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_to_range() {
        let f = |n, v| encode(n, Value::F(v)).unwrap().1;
        assert_eq!(f("Zoom", 500.0), vec![OscType::Float(300.0)]);
        assert_eq!(f("Zoom", 10.0), vec![OscType::Float(20.0)]);
        assert_eq!(f("Aperture", 0.0), vec![OscType::Float(1.4)]);
        assert_eq!(f("LookAtMeXOffset", -99.0), vec![OscType::Float(-25.0)]);
        assert_eq!(encode("Mode", Value::I(9)).unwrap().1, vec![OscType::Int(6)]);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(encode("Zoom", Value::F(f32::NAN)).is_err());
        assert!(encode("Zoom", Value::B(true)).is_err());
        assert!(encode("Nope", Value::F(1.0)).is_err());
    }

    #[test]
    fn encodes_addr_bool_and_trigger() {
        assert_eq!(encode("LookAtMe", Value::B(true)).unwrap(), ("/usercamera/LookAtMe".into(), vec![OscType::Bool(true)]));
        assert_eq!(encode("Capture", Value::B(false)).unwrap().1, vec![OscType::Bool(true)]);
    }

    #[test]
    fn pose_pitch_limit_and_wrap() {
        let p = Pose { pos: [1.0, 1.5, 2.0], rot: [95.0, 90.0, 0.0] };
        assert_eq!(p.to_args().unwrap()[3], OscType::Float(90.0));
        // eco real de VRChat para pitch -10: 349.99997
        let echo: Vec<_> = [1.0, 1.5, 2.0, 349.99997, 270.0, 0.0].iter().map(|v| OscType::Float(*v)).collect();
        let q = Pose::from_args(&echo).unwrap();
        assert!((q.rot[0] + 10.0).abs() < 1e-3 && (q.rot[1] + 90.0).abs() < 1e-3);
        assert!(Pose::from_args(&echo[..5]).is_none());
        assert!(Pose { pos: [f32::NAN, 0.0, 0.0], rot: [0.0; 3] }.to_args().is_err());
    }
}
