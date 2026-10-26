//! Protocolo OSC estricto y funciones matemáticas de normalización para el Rig Visualizador de VRChat (Avatars 3.0 / Unity).
//!
//! Todas las coordenadas se mapean en Unity con 1D BlendTrees donde:
//! - [-1.0, +1.0] equivale a [-50.0m, +50.0m] relativo al Ancla.
//! - [-1.0, +1.0] equivale a [-180.0°, +180.0°] en rotación Euler.

pub const MAX_RANGE_METERS: f32 = 50.0;
pub const MAX_ANGLE_DEGREES: f32 = 180.0;

// =========================================================================
// 12 Endpoints OSC Canónicos y Estrictos (Puerto 9000, Case-Sensitive)
// =========================================================================

pub const ADDR_IRIS_ENABLE: &str = "/avatar/parameters/iris_enable";
pub const ADDR_IRIS_CAM_X: &str = "/avatar/parameters/iris_cam_x";
pub const ADDR_IRIS_CAM_Y: &str = "/avatar/parameters/iris_cam_y";
pub const ADDR_IRIS_CAM_Z: &str = "/avatar/parameters/iris_cam_z";
pub const ADDR_IRIS_CAM_RX: &str = "/avatar/parameters/iris_cam_rx";
pub const ADDR_IRIS_CAM_RY: &str = "/avatar/parameters/iris_cam_ry";
pub const ADDR_IRIS_CAM_RZ: &str = "/avatar/parameters/iris_cam_rz";
pub const ADDR_IRIS_WP_INDEX: &str = "/avatar/parameters/iris_wp_index";
pub const ADDR_IRIS_WP_X: &str = "/avatar/parameters/iris_wp_x";
pub const ADDR_IRIS_WP_Y: &str = "/avatar/parameters/iris_wp_y";
pub const ADDR_IRIS_WP_Z: &str = "/avatar/parameters/iris_wp_z";
/// Bool (synced): oculta el gizmo y el path de las cámaras de VRChat (shader `_VRChatCameraMode`).
pub const ADDR_IRIS_HIDE_CAM: &str = "/avatar/parameters/iris_hide_cam";

pub const ALL_ENDPOINTS: [&str; 12] = [
    ADDR_IRIS_ENABLE,
    ADDR_IRIS_CAM_X,
    ADDR_IRIS_CAM_Y,
    ADDR_IRIS_CAM_Z,
    ADDR_IRIS_CAM_RX,
    ADDR_IRIS_CAM_RY,
    ADDR_IRIS_CAM_RZ,
    ADDR_IRIS_WP_INDEX,
    ADDR_IRIS_WP_X,
    ADDR_IRIS_WP_Y,
    ADDR_IRIS_WP_Z,
    ADDR_IRIS_HIDE_CAM,
];

// =========================================================================
// Funciones Matemáticas de Transformación y Normalización Exacta
// =========================================================================

/// Normaliza un eje de posición respecto a la coordenada del Ancla:
/// `norm = clamp((P_world - P_anchor) / MAX_RANGE_METERS, -1.0, 1.0)`.
#[inline]
pub fn normalize_position_axis(p_world: f32, p_anchor: f32) -> f32 {
    if !p_world.is_finite() || !p_anchor.is_finite() {
        return 0.0;
    }
    ((p_world - p_anchor) / MAX_RANGE_METERS).clamp(-1.0, 1.0)
}

/// Normaliza un vector 3D de coordenadas de mundo al espacio relativo del Ancla en [-1.0, 1.0].
#[inline]
pub fn normalize_position(p_world: [f32; 3], p_anchor: [f32; 3]) -> [f32; 3] {
    [
        normalize_position_axis(p_world[0], p_anchor[0]),
        normalize_position_axis(p_world[1], p_anchor[1]),
        normalize_position_axis(p_world[2], p_anchor[2]),
    ]
}

/// Normaliza un ángulo Euler en grados (en cualquier rango `0..360` o `-180..180`) al espacio OSC `[-1.0, 1.0]`.
/// Primero envuelve el ángulo a `[-180.0, +180.0]` y luego divide entre `180.0`:
/// `wrapped_angle = ((angle_degrees + 180.0) % 360.0 + 360.0) % 360.0 - 180.0`
/// `norm_rot = clamp(wrapped_angle / 180.0, -1.0, 1.0)`
#[inline]
pub fn normalize_euler_angle(angle_deg: f32) -> f32 {
    if !angle_deg.is_finite() {
        return 0.0;
    }
    let wrapped = (angle_deg + 180.0).rem_euclid(360.0) - 180.0;
    (wrapped / MAX_ANGLE_DEGREES).clamp(-1.0, 1.0)
}

/// Normaliza una rotación Euler `[Pitch (X), Yaw (Y), Roll (Z)]` en grados al espacio OSC `[-1.0, 1.0]`.
/// Espera ángulos ya relativos al ancla (`Anchor::world_to_local` resta el yaw): `Iris_World_Root` queda congelado en
/// el mundo con los ejes del yaw del ancla, no en (0, 0, 0) global.
#[inline]
pub fn normalize_rotation(rot_euler_deg: [f32; 3]) -> [f32; 3] {
    [
        normalize_euler_angle(rot_euler_deg[0]),
        normalize_euler_angle(rot_euler_deg[1]),
        normalize_euler_angle(rot_euler_deg[2]),
    ]
}
