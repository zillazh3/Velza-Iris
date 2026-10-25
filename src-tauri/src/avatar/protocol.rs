//! Protocolo y contrato OSC versionado (v1.0) para integración con el asset de avatar de VRChat (VRCFury).

use rosc::OscType;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Acciones ordenadas desde el avatar (`/avatar/parameters/Iris_Action`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum AvatarAction {
    Inactive = 0,
    CaptureA = 1,
    CaptureB = 2,
    Preview = 3,
    Execute = 4,
    Stop = 5,
    Clear = 6,
}

impl AvatarAction {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(AvatarAction::Inactive),
            1 => Some(AvatarAction::CaptureA),
            2 => Some(AvatarAction::CaptureB),
            3 => Some(AvatarAction::Preview),
            4 => Some(AvatarAction::Execute),
            5 => Some(AvatarAction::Stop),
            6 => Some(AvatarAction::Clear),
            _ => None,
        }
    }
}

/// Modo de anclaje (`/avatar/parameters/Iris_AnchorMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum AvatarAnchorMode {
    Auto = 0,
    Player = 1,
    Manual = 2,
}

impl AvatarAnchorMode {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(AvatarAnchorMode::Auto),
            1 => Some(AvatarAnchorMode::Player),
            2 => Some(AvatarAnchorMode::Manual),
            _ => None,
        }
    }
}

/// Tipo de trayectoria (`/avatar/parameters/Iris_PathType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum AvatarPathType {
    Linear = 0,
    Bezier = 1,
    Orbital = 2,
}

impl AvatarPathType {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(AvatarPathType::Linear),
            1 => Some(AvatarPathType::Bezier),
            2 => Some(AvatarPathType::Orbital),
            _ => None,
        }
    }
}

/// Estado del sistema reportado al avatar (`/avatar/parameters/Iris_State`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
#[serde(rename_all = "camelCase")]
pub enum AvatarIntegrationState {
    Idle = 0,
    Configuring = 1,
    Ready = 2,
    Previewing = 3,
    Playing = 4,
    Stopped = 5,
    Error = 6,
}

/// Rutas canónicas OSC del contrato.
pub const ADDR_ACTION: &str = "/avatar/parameters/Iris_Action";
pub const ADDR_DURATION: &str = "/avatar/parameters/Iris_Duration";
pub const ADDR_ANCHOR_MODE: &str = "/avatar/parameters/Iris_AnchorMode";
pub const ADDR_PATH_TYPE: &str = "/avatar/parameters/Iris_PathType";
pub const ADDR_LOOP: &str = "/avatar/parameters/Iris_Loop";
pub const ADDR_ASSET_VISIBLE: &str = "/avatar/parameters/Iris_AssetVisible";
pub const ADDR_AVATAR_CHANGE: &str = "/avatar/change";

pub const ADDR_OUT_STATE: &str = "/avatar/parameters/Iris_State";
pub const ADDR_OUT_PROGRESS: &str = "/avatar/parameters/Iris_Progress";
pub const ADDR_OUT_CONNECTED: &str = "/avatar/parameters/Iris_Connected";

/// Convierte el valor normalizado `[0.0, 1.0]` a segundos según la fórmula canónica:
/// `duration = min_duration + norm * (max_duration - min_duration)`.
pub fn normalized_to_duration(norm: f32, min_duration: f32, max_duration: f32) -> f32 {
    let safe_norm = if norm.is_finite() { norm.clamp(0.0, 1.0) } else { 0.0 };
    let min_d = min_duration.max(0.1);
    let max_d = max_duration.max(min_d);
    min_d + safe_norm * (max_d - min_d)
}

/// Convierte segundos al valor normalizado `[0.0, 1.0]`.
pub fn duration_to_normalized(seconds: f32, min_duration: f32, max_duration: f32) -> f32 {
    let min_d = min_duration.max(0.1);
    let max_d = max_duration.max(min_d + 1e-4);
    let safe_sec = if seconds.is_finite() { seconds.clamp(min_d, max_d) } else { min_d };
    ((safe_sec - min_d) / (max_d - min_d)).clamp(0.0, 1.0)
}

/// Extrae un `i32` seguro desde un argumento OSC (`Int` o `Float`).
pub fn extract_osc_int(arg: &OscType) -> Option<i32> {
    match arg {
        OscType::Int(i) => Some(*i),
        OscType::Float(f) if f.is_finite() => Some(*f as i32),
        _ => None,
    }
}

/// Extrae un `f32` finito seguro desde un argumento OSC (`Float` o `Int`).
pub fn extract_osc_float(arg: &OscType) -> Option<f32> {
    match arg {
        OscType::Float(f) if f.is_finite() => Some(*f),
        OscType::Int(i) => Some(*i as f32),
        _ => None,
    }
}

/// Extrae un `bool` seguro desde un argumento OSC (`Bool` o `Int`).
pub fn extract_osc_bool(arg: &OscType) -> Option<bool> {
    match arg {
        OscType::Bool(b) => Some(*b),
        OscType::Int(i) => Some(*i != 0),
        OscType::Float(f) if f.is_finite() => Some(*f > 0.5),
        _ => None,
    }
}

/// Extrae un `String` desde un argumento OSC.
pub fn extract_osc_string(arg: &OscType) -> Option<String> {
    match arg {
        OscType::String(s) => Some(s.clone()),
        _ => None,
    }
}
