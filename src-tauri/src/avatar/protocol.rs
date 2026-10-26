//! Protocolo OSC y matemáticas de codificación del Rig Visualizador de VRChat (Avatars 3.0 / Unity).
//!
//! Dos canales con el mismo avatar:
//! - **Dueño (parámetros NO sincronizados, float32 exacto):** `iris_cam_*` (pose en vivo, [-1, 1] = ±50 m / ±180°),
//!   `iris_pc_*`/`iris_pr_*` (puntos de control del path) e `iris_reg_*` (centro de la región). Escritos directo por OSC.
//! - **Remotos (parámetros sincronizados, 8 bits):** un bus `iris_net_x/y/z` + selector `iris_net_slot` (time-slicing con
//!   latch por `VRCAvatarParameterDriver`) y el progreso `iris_path_t`. Los valores de 8 bits se codifican en
//!   coarse + fine para recuperar ~1.6 mm sin gastar bits (ver `split_coarse_fine`).
//!
//! El contrato de nombres, slots y escalas se duplica en `VelzaIrisBuilder.cs` (Unity): si se cambia aquí, cambiarlo allá.

pub const MAX_RANGE_METERS: f32 = 50.0;
pub const MAX_ANGLE_DEGREES: f32 = 180.0;

/// Puntos de control del path que viajan por el bus (el gizmo remoto interpola linealmente entre ellos).
pub const NET_POINTS: usize = 8;
/// Índice del punto de control que mueve cada uno de los 5 huesos del mesh del path (reparto uniforme en índice).
pub const BONE_POINT: [usize; 5] = [0, 2, 4, 5, 7];
/// Rango de los centros de región: ±1000 m (el centro se elige en la rejilla de 1/127 de ese rango).
pub const REGION_RANGE_METERS: f32 = 1000.0;
/// Las rotaciones de los puntos del path se desenvuelven (sin saltos ±180°) y viajan en ±360°.
pub const ROT_RANGE_DEGREES: f32 = 360.0;
/// VRChat sincroniza un float de [-1, 1] en 8 bits: múltiplos de 1/127.
pub const NET_QUANT_STEPS: f32 = 127.0;
/// Se envía el centro del "bin" (k ± 0.02) para que el redondeo, el truncado o un epsilon de float caigan en el mismo k.
pub const NET_BIN_BIAS: f32 = 0.02;

// =========================================================================
// Direcciones OSC (puerto 9000, sensibles a mayúsculas)
// =========================================================================

pub const ADDR_IRIS_ENABLE: &str = "/avatar/parameters/iris_enable";
/// Bool (synced): oculta el gizmo y el path de las cámaras de VRChat (shader `_VRChatCameraMode`).
pub const ADDR_IRIS_HIDE_CAM: &str = "/avatar/parameters/iris_hide_cam";
// Pose en vivo del dueño (no synced).
pub const ADDR_IRIS_CAM_X: &str = "/avatar/parameters/iris_cam_x";
pub const ADDR_IRIS_CAM_Y: &str = "/avatar/parameters/iris_cam_y";
pub const ADDR_IRIS_CAM_Z: &str = "/avatar/parameters/iris_cam_z";
pub const ADDR_IRIS_CAM_RX: &str = "/avatar/parameters/iris_cam_rx";
pub const ADDR_IRIS_CAM_RY: &str = "/avatar/parameters/iris_cam_ry";
pub const ADDR_IRIS_CAM_RZ: &str = "/avatar/parameters/iris_cam_rz";
// Centro de la región (no synced en el dueño; los remotos lo reciben por el slot `REGION`).
pub const ADDR_IRIS_REG_X: &str = "/avatar/parameters/iris_reg_x";
pub const ADDR_IRIS_REG_Y: &str = "/avatar/parameters/iris_reg_y";
pub const ADDR_IRIS_REG_Z: &str = "/avatar/parameters/iris_reg_z";
// Bus sincronizado hacia los remotos.
pub const ADDR_IRIS_NET_SLOT: &str = "/avatar/parameters/iris_net_slot";
pub const ADDR_IRIS_NET_X: &str = "/avatar/parameters/iris_net_x";
pub const ADDR_IRIS_NET_Y: &str = "/avatar/parameters/iris_net_y";
pub const ADDR_IRIS_NET_Z: &str = "/avatar/parameters/iris_net_z";
pub const ADDR_IRIS_PATH_T: &str = "/avatar/parameters/iris_path_t";

const AXES: [&str; 3] = ["x", "y", "z"];

/// `iris_pc_{i}_{x|y|z}`: posición del punto de control `i` (coarse), en unidades de 50 m relativas al centro de la región.
pub fn addr_pc(i: usize, axis: usize) -> String {
    format!("/avatar/parameters/iris_pc_{i}_{}", AXES[axis])
}

/// `iris_pr_{i}_{x|y|z}`: rotación del punto de control `i`, en unidades de 360° (desenvuelta).
pub fn addr_pr(i: usize, axis: usize) -> String {
    format!("/avatar/parameters/iris_pr_{i}_{}", AXES[axis])
}

/// Direcciones que escribe la app SIN pasar por el bus (parámetros locales del dueño).
pub fn owner_endpoints() -> Vec<String> {
    let mut v: Vec<String> = [
        ADDR_IRIS_CAM_X,
        ADDR_IRIS_CAM_Y,
        ADDR_IRIS_CAM_Z,
        ADDR_IRIS_CAM_RX,
        ADDR_IRIS_CAM_RY,
        ADDR_IRIS_CAM_RZ,
        ADDR_IRIS_REG_X,
        ADDR_IRIS_REG_Y,
        ADDR_IRIS_REG_Z,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for i in 0..NET_POINTS {
        for a in 0..3 {
            v.push(addr_pc(i, a));
        }
    }
    for i in 0..NET_POINTS {
        for a in 0..3 {
            v.push(addr_pr(i, a));
        }
    }
    v
}

/// Parámetros sincronizados (82 → 42 bits: 2 bool + 1 int + 4 float).
pub const SYNCED_ENDPOINTS: [&str; 7] = [
    ADDR_IRIS_ENABLE,
    ADDR_IRIS_HIDE_CAM,
    ADDR_IRIS_NET_SLOT,
    ADDR_IRIS_NET_X,
    ADDR_IRIS_NET_Y,
    ADDR_IRIS_NET_Z,
    ADDR_IRIS_PATH_T,
];

// =========================================================================
// Slots del bus (`iris_net_slot`; 0 = libre). Orden de envío: región, posiciones coarse, rotaciones, posiciones fine.
// =========================================================================

pub mod slot {
    use super::NET_POINTS;

    pub const REGION: i32 = 1;
    pub const fn pos_c(i: usize) -> i32 {
        2 + i as i32
    }
    pub const fn rot_c(i: usize) -> i32 {
        2 + NET_POINTS as i32 + i as i32
    }
    pub const fn pos_f(i: usize) -> i32 {
        2 + 2 * NET_POINTS as i32 + i as i32
    }
    /// Pose retenida (cámara a mano): coarse, fine y rotación.
    pub const POSE_C: i32 = 2 + 3 * NET_POINTS as i32;
    pub const POSE_F: i32 = POSE_C + 1;
    pub const POSE_R: i32 = POSE_C + 2;
    /// El gizmo remoto sigue el path (`iris_path_t`) o la pose retenida.
    pub const MODE_PATH: i32 = POSE_C + 3;
    pub const MODE_HAND: i32 = POSE_C + 4;
    pub const COUNT: i32 = MODE_HAND;
}

// =========================================================================
// Normalización (canal del dueño y compatibilidad con el protocolo anterior)
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
/// Espera ángulos ya relativos al ancla (`Anchor::world_to_local` resta el yaw).
#[inline]
pub fn normalize_rotation(rot_euler_deg: [f32; 3]) -> [f32; 3] {
    [
        normalize_euler_angle(rot_euler_deg[0]),
        normalize_euler_angle(rot_euler_deg[1]),
        normalize_euler_angle(rot_euler_deg[2]),
    ]
}

// =========================================================================
// Canal sincronizado: cuantización de red y coarse + fine
// =========================================================================

/// Lo que la red devuelve a un remoto para `x ∈ [-1, 1]`: el múltiplo de 1/127 más cercano.
#[inline]
pub fn q8(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    (x.clamp(-1.0, 1.0) * NET_QUANT_STEPS).round() / NET_QUANT_STEPS
}

/// Valor a ESCRIBIR en un parámetro synced para que el remoto reciba exactamente `q8(x)`: el centro del bin.
#[inline]
pub fn net_bin(x: f32) -> f32 {
    let k = if x.is_finite() { (x.clamp(-1.0, 1.0) * NET_QUANT_STEPS).round() } else { 0.0 };
    // Sin sesgo en 0 (ya es exacto) ni en los extremos ±1 (no se puede salir del rango).
    let bias = if k == 0.0 || k.abs() >= NET_QUANT_STEPS { 0.0 } else { NET_BIN_BIAS * k.signum() };
    (k + bias) / NET_QUANT_STEPS
}

/// Divide `v ∈ [-1, 1]` en (coarse, fine): `v ≈ c + f/254`. El residual se calcula sobre el coarse YA cuantizado
/// (cero sesgo) y se reescala ×254 para llenar [-1, 1]; el paso efectivo es 1/(127·254) del rango (≈ 1.6 mm con 50 m).
#[inline]
pub fn split_coarse_fine(v: f32) -> (f32, f32) {
    let v = if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
    let c = q8(v);
    let f = q8((v - c) * 254.0);
    (c, f)
}

#[inline]
pub fn join_coarse_fine(c: f32, f: f32) -> f32 {
    c + f / 254.0
}

/// Centro de región ajustado a la rejilla que el remoto sabe representar (coarse de ±1000 m, sin fine).
pub fn snap_region(center_world: [f32; 3]) -> [f32; 3] {
    let s = |x: f32| q8(x / REGION_RANGE_METERS) * REGION_RANGE_METERS;
    [s(center_world[0]), s(center_world[1]), s(center_world[2])]
}
