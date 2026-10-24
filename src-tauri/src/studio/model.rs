//! Modelo de datos compartido entre el Modo Captura (Simple) y el Modo Editor (Avanzado) de Iris Studio.
//! Un único `SequenceProject` representa toda la secuencia sin pérdida de información al alternar vistas.

use crate::anchor::Anchor;
use crate::motion::sample::{Motion as PresetMotion, Zoom as PresetZoom};
use crate::osc::protocol::{in_range, Pose};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const STUDIO_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_PROJECT_DURATION_SEC: f32 = 10.0;
pub const MIN_PROJECT_DURATION_SEC: f32 = 0.2;
pub const MAX_PROJECT_DURATION_SEC: f32 = 3600.0;

/// Unidad preferida para visualizar y editar la duración del proyecto en la interfaz.
/// La representación interna del proyecto es siempre en segundos (`f32`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DurationUnit {
    #[default]
    Seconds,
    Minutes,
}

/// Modalidad preferida de entrada para la duración del proyecto en la interfaz.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DurationInputMode {
    #[default]
    ValueUnit,
    MmSs,
}

/// Estrategia explícita de resolución al modificar la duración del proyecto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DurationChangeStrategy {
    /// Conserva intactos los tiempos de todos los elementos; rechaza si algún elemento queda fuera del nuevo rango.
    #[default]
    Preserve,
    /// Ajusta al nuevo límite únicamente los elementos que excedan el rango reducido, sin tocar los demás.
    ClampToRange,
    /// Escala proporcionalmente todos los keyframes y clips al nuevo rango temporal.
    ScaleProportionally,
}

/// Resumen de elementos temporales que quedarían fuera del rango al reducir la duración.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutOfRangeReport {
    pub keyframes: usize,
    pub effects: usize,
}

impl OutOfRangeReport {
    pub fn has_conflicts(&self) -> bool {
        self.keyframes > 0 || self.effects > 0
    }
}

/// Espacio de referencia de la secuencia.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceSpace {
    /// Relativo al ancla calibrada (portable entre mundos y ubicaciones).
    #[default]
    Anchor,
    /// Coordenadas absolutas del mundo actual.
    World,
}

/// Comportamiento cinemático en un keyframe intermedio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnotMode {
    /// Paso continuo ($C^1$) sin frenar a cero en los puntos intermedios.
    #[default]
    PassThrough,
    /// Parada suave controlada en este keyframe antes de iniciar el siguiente tramo.
    Stop,
}

/// Perfiles iniciales del motor cinematográfico.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CinematicProfile {
    /// Movimientos suaves y naturales con aceleración/desaceleración progresiva e inercia balanceada.
    #[default]
    Cinematic,
    /// Trayectorias altamente reproducibles y controladas sin retardo de amortiguación extra.
    Precision,
    /// Inercia cálida de operador humano con micro-variación determinista opcional.
    Organic,
}

/// Configuración del motor cinematográfico almacenada en el proyecto.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CinematicConfig {
    pub profile: CinematicProfile,
    /// Constante de tiempo de inercia (s): 0.0 = respuesta directa, 0.18 = cinemático, 0.24 = orgánico.
    pub inertia_tau: f32,
    /// Límite de velocidad lineal (m/s).
    pub max_linear_speed: f32,
    /// Límite de aceleración/frenado lineal (m/s²).
    pub max_linear_accel: f32,
    /// Límite de velocidad angular (°/s).
    pub max_angular_speed: f32,
    /// Nivel de variación orgánica sutil `[0.0 ..= 1.0]`.
    #[serde(alias = "organicIntensity")]
    pub organic_variation: f32,
    /// Semilla determinista para reproducibilidad exacta de variaciones orgánicas.
    pub organic_seed: u32,
}

impl Default for CinematicConfig {
    fn default() -> Self {
        Self::for_profile(CinematicProfile::Cinematic)
    }
}

impl CinematicConfig {
    pub fn for_profile(profile: CinematicProfile) -> Self {
        match profile {
            CinematicProfile::Cinematic => CinematicConfig {
                profile,
                inertia_tau: 0.18,
                max_linear_speed: 4.0,
                max_linear_accel: 6.0,
                max_angular_speed: 120.0,
                organic_variation: 0.0,
                organic_seed: 104_729,
            },
            CinematicProfile::Precision => CinematicConfig {
                profile,
                inertia_tau: 0.0,
                max_linear_speed: 8.0,
                max_linear_accel: 15.0,
                max_angular_speed: 240.0,
                organic_variation: 0.0,
                organic_seed: 104_729,
            },
            CinematicProfile::Organic => CinematicConfig {
                profile,
                inertia_tau: 0.24,
                max_linear_speed: 3.5,
                max_linear_accel: 4.5,
                max_angular_speed: 90.0,
                organic_variation: 0.45,
                organic_seed: 104_729,
            },
        }
    }

    /// Ajustes recomendados de `SmoothMovement` y `SmoothingStrength` para la cámara de VRChat según el perfil.
    pub fn vrchat_smoothing(&self) -> (bool, f32) {
        match self.profile {
            CinematicProfile::Precision => (false, 2.0),
            CinematicProfile::Cinematic => (true, 5.5),
            CinematicProfile::Organic => (true, 6.5),
        }
    }
}

/// Ajustes persistentes y reversibles de calibración técnica y visual.
/// Nunca alteran las coordenadas originales de los keyframes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CalibrationConfig {
    /// Permite desactivar toda la capa de calibración sin impedir el funcionamiento normal de Iris.
    pub enabled: bool,
    /// Activa el corrector adaptativo acotado cuando exista evidencia suficiente de eco OSC.
    pub auto_correct: bool,
    /// Offset técnico de posición en metros `[dx, dy, dz]` (limitado a ±2.0 m).
    pub pos_offset: [f32; 3],
    /// Offset técnico de orientación horizontal (°) (limitado a ±180°).
    pub yaw_offset_deg: f32,
    /// Offset técnico de inclinación vertical (°) (limitado a ±25°).
    pub pitch_offset_deg: f32,
    /// Encuadre horizontal normalizado `[-1.0 ..= 1.0]` (-0.33 = tercio izquierdo, +0.33 = tercio derecho).
    pub composition_x: f32,
    /// Encuadre vertical / headroom normalizado `[-1.0 ..= 1.0]` (+0.25 = tercio superior de retrato).
    pub composition_y: f32,
    /// Compensación de espacio de mirada / movimiento (`lead room`) `[0.0 ..= 1.0]`.
    pub lead_room: f32,
}

/// Curva de transición entre puntos o de evolución de un efecto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Transition {
    Linear,
    #[default]
    Smooth,
    SlowIn,
    SlowOut,
    SmoothInOut,
}

impl Transition {
    /// Evalúa la curva de progreso para `u` en `[0, 1]`.
    pub fn apply(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Transition::Linear => u,
            Transition::Smooth => 0.5 - 0.5 * (PI * u).cos(),
            Transition::SlowIn => u * u * u,
            Transition::SlowOut => 1.0 - (1.0 - u).powi(3),
            Transition::SmoothInOut => {
                if u < 0.5 {
                    4.0 * u.powi(3)
                } else {
                    1.0 - (-2.0 * u + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

/// Evalúa una curva Bézier cúbica `[x1, y1, x2, y2]` estándar con resolución Newton-Raphson.
pub fn sample_cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let mut u = t;
    for _ in 0..6 {
        let one_minus_u = 1.0 - u;
        let slope = 3.0 * one_minus_u * one_minus_u * x1
            + 6.0 * one_minus_u * u * (x2 - x1)
            + 3.0 * u * u * (1.0 - x2);
        if slope.abs() < 1e-5 {
            break;
        }
        let current_x = 3.0 * one_minus_u * one_minus_u * u * x1
            + 3.0 * one_minus_u * u * u * x2
            + u * u * u;
        let diff = current_x - t;
        u = (u - diff / slope).clamp(0.0, 1.0);
    }
    let one_minus_u = 1.0 - u;
    3.0 * one_minus_u * one_minus_u * u * y1 + 3.0 * one_minus_u * u * u * y2 + u * u * u
}

/// Ajustes ópticos opcionales asociados a un keyframe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LookTrackPoint {
    pub zoom: Option<f32>,
    pub exposure: Option<f32>,
    pub focal_distance: Option<f32>,
    pub aperture: Option<f32>,
}

/// Punto de cámara capturado o editado (Punto A, Punto B, Punto C...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyframe {
    pub id: String,
    pub label: String,
    /// Instante temporal en segundos `[0.0 ..= duration]`.
    pub time: f32,
    /// Coordenadas exactas de mundo recibidas de VRChat en el momento de captura.
    pub raw_pose: Pose,
    /// Transformación relativa al ancla calibrada (cuando existe).
    #[serde(default)]
    pub local_pose: Option<Pose>,
    /// Timestamp epoch (ms) cuando se capturó el punto.
    #[serde(default)]
    pub captured_at_ms: u64,
    /// Transición para llegar a este punto desde el punto anterior.
    #[serde(default)]
    pub transition: Transition,
    /// Curva Bézier personalizada opcional `[x1, y1, x2, y2]`.
    #[serde(default)]
    pub custom_curve: Option<[f32; 4]>,
    /// Comportamiento de continuidad de velocidad al cruzar este punto (`PassThrough` vs `Stop`).
    #[serde(default)]
    pub knot_mode: KnotMode,
    /// Parámetros ópticos opcionales en este instante.
    #[serde(default)]
    pub look: LookTrackPoint,
}

impl Keyframe {
    /// Obtiene la Pose en espacio de mundo según el espacio de referencia y el ancla activa.
    pub fn world_pose(&self, space: ReferenceSpace, active_anchor: Option<&Anchor>, fallback_anchor: Option<&Anchor>, scale: f32) -> Pose {
        if space == ReferenceSpace::Anchor {
            if let Some(local) = &self.local_pose {
                if let Some(a) = active_anchor.or(fallback_anchor) {
                    return a.local_to_world(local, scale);
                }
            } else if let (Some(orig), Some(curr)) = (fallback_anchor, active_anchor) {
                let local = orig.world_to_local(&self.raw_pose);
                return curr.local_to_world(&local, scale);
            }
        }
        self.raw_pose
    }
}

/// Catálogo completo de movimientos, microefectos y efectos ópticos de Iris Studio.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectKind {
    // Movimientos espaciales aditivos
    DollyIn,
    DollyOut,
    Orbit,
    Truck,
    Crane,
    Pan,
    Tilt,
    Reveal,
    // Microefectos orgánicos
    MicroShake,
    ImpactShake,
    Handheld,
    CameraDrift,
    CameraBreathing,
    // Óptica
    SmoothZoom,
    CrashZoom,
    FocusPull,
    ApertureShift,
    DollyZoom,
}

/// Clip no destructivo en el timeline de efectos.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectClip {
    pub id: String,
    pub kind: EffectKind,
    /// Índice de pista en el Modo Editor (0, 1, 2...).
    #[serde(default)]
    pub track: u8,
    pub start_time: f32,
    pub duration: f32,
    /// Intensidad / amplitud principal (1.0 = nominal).
    #[serde(default = "default_one")]
    pub intensity: f32,
    /// Frecuencia / velocidad relativa (1.0 = nominal).
    #[serde(default = "default_one")]
    pub speed: f32,
    /// Parámetro secundario inicial (p. ej. Zoom inicial en mm, distancia inicial de enfoque en m).
    #[serde(default)]
    pub param_a: Option<f32>,
    /// Parámetro secundario final (p. ej. Zoom final en mm, distancia final de enfoque en m).
    #[serde(default)]
    pub param_b: Option<f32>,
    #[serde(default)]
    pub easing: Transition,
    #[serde(default)]
    pub custom_curve: Option<[f32; 4]>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_one() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

/// Fuente de datos del objetivo de seguimiento (`TargetProvider`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetSourceKind {
    /// Punto 3D estático capturado colocando la cámara en el objetivo.
    StaticCalibratedPoint,
    /// Origen del ancla calibrada (ojos del jugador operador local). Fuente predeterminada.
    #[default]
    CalibratedAnchor,
    /// Coordenada introducida o ajustada manualmente en el inspector.
    ManualReference,
    /// Futuro puente voluntario por parámetros OSC de avatar compatible (Experimental).
    CompanionAvatarOsc,
    /// Futuro puente de mundo Udon compatible (Experimental).
    CompanionUdonBridge,
}

/// Configuración de seguimiento de objetivo (`TargetProvider` estático calibrado o ancla).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetConfig {
    pub enabled: bool,
    #[serde(default)]
    pub source: TargetSourceKind,
    /// Coordenada en mundo del punto de interés calibrado.
    pub world_pos: [f32; 3],
    /// Peso de mezcla `[0.0 ..= 1.0]` entre la rotación de los keyframes y mirar al objetivo.
    #[serde(default = "default_one")]
    pub blend: f32,
    /// Nivel de confianza de la referencia `[0.0 ..= 1.0]`.
    #[serde(default = "default_one")]
    pub confidence: f32,
    /// Timestamp epoch (ms) cuando se calibró el objetivo.
    #[serde(default)]
    pub captured_at_ms: u64,
}

/// Muestra original capturada durante una grabación continua de recorrido.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedSample {
    pub time: f32,
    pub pose: Pose,
    #[serde(default)]
    pub zoom: Option<f32>,
}

fn default_project_duration() -> f32 {
    DEFAULT_PROJECT_DURATION_SEC
}

/// Proyecto completo de secuencia cinematográfica de Iris Studio.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceProject {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    /// Duración total de la toma expresada siempre en segundos.
    #[serde(default = "default_project_duration")]
    pub duration: f32,
    /// Unidad de visualización preferida para la configuración del proyecto (`seconds` o `minutes`).
    #[serde(default)]
    pub duration_unit: DurationUnit,
    /// Modalidad de entrada preferida en la configuración del proyecto (`valueUnit` o `mmSs`).
    #[serde(default)]
    pub duration_input_mode: DurationInputMode,
    #[serde(default)]
    pub space: ReferenceSpace,
    #[serde(default)]
    pub anchor_snapshot: Option<Anchor>,
    #[serde(default = "default_one")]
    pub trajectory_scale: f32,
    #[serde(default)]
    pub cinematic: CinematicConfig,
    #[serde(default)]
    pub calibration: CalibrationConfig,
    #[serde(default)]
    pub target: Option<TargetConfig>,
    #[serde(default)]
    pub keyframes: Vec<Keyframe>,
    #[serde(default)]
    pub effects: Vec<EffectClip>,
    /// Muestras crudas de grabación continua; nunca se sobrescriben al simplificar.
    #[serde(default)]
    pub raw_recording: Vec<RecordedSample>,
}

/// Etiqueta automática para el índice `idx` (`0 -> "Punto A"`, `1 -> "Punto B"`, ...).
pub fn point_label(idx: usize) -> String {
    let letter = (b'A' + (idx % 26) as u8) as char;
    if idx < 26 {
        format!("Punto {letter}")
    } else {
        format!("Punto {letter}{}", idx / 26 + 1)
    }
}

impl SequenceProject {
    pub fn new(id: impl Into<String>, name: impl Into<String>, duration: f32) -> SequenceProject {
        SequenceProject {
            schema_version: STUDIO_SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            duration: duration.clamp(MIN_PROJECT_DURATION_SEC, MAX_PROJECT_DURATION_SEC),
            duration_unit: DurationUnit::Seconds,
            duration_input_mode: DurationInputMode::ValueUnit,
            space: ReferenceSpace::Anchor,
            anchor_snapshot: None,
            trajectory_scale: 1.0,
            cinematic: CinematicConfig::default(),
            calibration: CalibrationConfig::default(),
            target: None,
            keyframes: Vec::new(),
            effects: Vec::new(),
            raw_recording: Vec::new(),
        }
    }

    /// Convierte un movimiento predefinido (`Motion`) en un `SequenceProject` editable para compartir
    /// la misma lógica de movimiento y el mismo modelo entre el Modo Simple y el Modo Avanzado.
    pub fn from_preset_motion(motion: &PresetMotion, anchor: Option<Anchor>) -> SequenceProject {
        let eff_anchor = anchor.unwrap_or(Anchor { eye: [0.0, 1.65, 0.0], yaw: 0.0 });
        let dur = motion.duration.clamp(MIN_PROJECT_DURATION_SEC, MAX_PROJECT_DURATION_SEC);
        let delta_yaw = (motion.to.yaw - motion.from.yaw).abs();
        // Si es una órbita o recorrido angular amplio (>90°), muestreamos 5 puntos para que la spline Catmull-Rom
        // forme un arco circular exacto en lugar de cortar por la cuerda.
        let steps = if delta_yaw > 90.0 { 4 } else { 2 };
        let mut keyframes = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let frac = i as f32 / steps as f32;
            let t = (dur * frac * 100.0).round() / 100.0;
            let sample = motion.sample(t);
            let world_pose = eff_anchor.pose(&sample.frame);
            let local_pose = eff_anchor.world_to_local(&world_pose);
            let zoom = match motion.zoom {
                PresetZoom::Keep => Some(45.0),
                _ => sample.zoom,
            };
            keyframes.push(Keyframe {
                id: format!("kf-mot-{}", i + 1),
                label: point_label(i),
                time: t,
                raw_pose: world_pose,
                local_pose: Some(local_pose),
                captured_at_ms: 0,
                transition: Transition::Smooth,
                custom_curve: None,
                knot_mode: if i == 0 || i == steps { KnotMode::Stop } else { KnotMode::PassThrough },
                look: LookTrackPoint {
                    zoom,
                    exposure: None,
                    focal_distance: None,
                    aperture: None,
                },
            });
        }

        let mut effects = Vec::new();
        if motion.handheld {
            effects.push(EffectClip {
                id: "ef-mot-handheld".into(),
                kind: EffectKind::Handheld,
                track: 0,
                start_time: 0.0,
                duration: dur,
                intensity: 1.0,
                speed: 1.0,
                param_a: None,
                param_b: None,
                easing: Transition::Linear,
                custom_curve: None,
                enabled: true,
            });
        }

        SequenceProject {
            schema_version: STUDIO_SCHEMA_VERSION,
            id: format!("mot-{}", motion.name.to_lowercase().replace(' ', "-")),
            name: motion.name.clone(),
            duration: dur,
            duration_unit: DurationUnit::Seconds,
            duration_input_mode: DurationInputMode::ValueUnit,
            space: ReferenceSpace::Anchor,
            anchor_snapshot: Some(eff_anchor),
            trajectory_scale: 1.0,
            cinematic: CinematicConfig::for_profile(if motion.handheld {
                CinematicProfile::Organic
            } else {
                CinematicProfile::Cinematic
            }),
            calibration: CalibrationConfig::default(),
            target: Some(TargetConfig {
                enabled: true,
                source: TargetSourceKind::CalibratedAnchor,
                world_pos: eff_anchor.eye,
                blend: 1.0,
                confidence: 1.0,
                captured_at_ms: 0,
            }),
            keyframes,
            effects,
            raw_recording: Vec::new(),
        }
    }

    /// Ordena los keyframes cronológicamente sin alterar sus coordenadas ni sus nombres personalizados.
    pub fn sort_keyframes(&mut self) {
        self.keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Detecta cuántos keyframes y clips de movimiento/efecto quedarían fuera de rango si la duración
    /// cambiara a `new_duration` (en segundos).
    pub fn out_of_range_elements(&self, new_duration: f32) -> OutOfRangeReport {
        let eps = 1e-3_f32;
        let keyframes = self
            .keyframes
            .iter()
            .filter(|kf| kf.time > new_duration + eps)
            .count();
        let effects = self
            .effects
            .iter()
            .filter(|ef| ef.start_time > new_duration + eps || ef.start_time + ef.duration > new_duration + eps)
            .count();
        OutOfRangeReport { keyframes, effects }
    }

    /// Aplica un cambio de duración de forma segura según la estrategia elegida por el usuario.
    /// - Al aumentar la duración con `Preserve` o `ClampToRange`, conserva intactos los tiempos de los keyframes
    ///   y las duraciones individuales de los movimientos/efectos.
    /// - Al reducir la duración con `Preserve`, devuelve error si existen elementos fuera del nuevo rango,
    ///   evitando modificaciones o recortes silenciosos.
    pub fn apply_duration_change(
        &mut self,
        new_duration: f32,
        strategy: DurationChangeStrategy,
    ) -> Result<(), String> {
        if !new_duration.is_finite()
            || !(MIN_PROJECT_DURATION_SEC..=MAX_PROJECT_DURATION_SEC).contains(&new_duration)
        {
            return Err(format!(
                "Duración fuera de rango ({MIN_PROJECT_DURATION_SEC}–{MAX_PROJECT_DURATION_SEC} s): {new_duration}"
            ));
        }
        let old_dur = self.duration.max(0.001);
        match strategy {
            DurationChangeStrategy::Preserve => {
                let report = self.out_of_range_elements(new_duration);
                if report.has_conflicts() {
                    return Err(format!(
                        "La nueva duración ({new_duration} s) dejaría fuera de rango {} punto(s) y {} efecto(s). Confirma cómo resolverlos.",
                        report.keyframes, report.effects
                    ));
                }
            }
            DurationChangeStrategy::ClampToRange => {
                for kf in &mut self.keyframes {
                    if kf.time > new_duration {
                        kf.time = new_duration;
                    }
                }
                for ef in &mut self.effects {
                    if ef.start_time >= new_duration {
                        ef.start_time = (new_duration - ef.duration.min(new_duration)).max(0.0);
                    }
                    if ef.start_time + ef.duration > new_duration {
                        ef.duration = (new_duration - ef.start_time).max(0.1);
                    }
                }
            }
            DurationChangeStrategy::ScaleProportionally => {
                if (new_duration - old_dur).abs() > 1e-4 {
                    let ratio = new_duration / old_dur;
                    for kf in &mut self.keyframes {
                        kf.time = ((kf.time * ratio * 100.0).round() / 100.0).clamp(0.0, new_duration);
                    }
                    for ef in &mut self.effects {
                        ef.start_time = ((ef.start_time * ratio * 100.0).round() / 100.0).clamp(0.0, new_duration);
                        ef.duration = ((ef.duration * ratio * 100.0).round() / 100.0)
                            .clamp(0.1, (new_duration - ef.start_time).max(0.1));
                    }
                }
            }
        }
        self.duration = new_duration;
        Ok(())
    }

    /// Cambia la duración total. Si `scale_proportionally` es `true`, escala los tiempos de puntos y efectos
    /// manteniendo intactas todas las coordenadas. Si es `false`, ajusta al límite los elementos que excedan `new_duration`.
    pub fn set_duration(&mut self, new_duration: f32, scale_proportionally: bool) {
        let new_dur = new_duration.clamp(MIN_PROJECT_DURATION_SEC, MAX_PROJECT_DURATION_SEC);
        let strategy = if scale_proportionally {
            DurationChangeStrategy::ScaleProportionally
        } else {
            DurationChangeStrategy::ClampToRange
        };
        let _ = self.apply_duration_change(new_dur, strategy);
    }

    /// Re-vincula las poses locales respecto a un ancla dada para hacer portable la secuencia.
    pub fn bind_anchor(&mut self, anchor: Anchor) {
        self.anchor_snapshot = Some(anchor);
        for kf in &mut self.keyframes {
            kf.local_pose = Some(anchor.world_to_local(&kf.raw_pose));
        }
    }

    /// ¿Tiene ajustes propios del Modo Editor (múltiples pistas, óptica por punto o seguimiento)?
    pub fn has_advanced_features(&self) -> bool {
        self.target.is_some_and(|t| t.enabled)
            || self.effects.iter().any(|e| e.track > 0)
            || self.keyframes.iter().any(|k| {
                k.look.zoom.is_some()
                    || k.look.exposure.is_some()
                    || k.look.focal_distance.is_some()
                    || k.look.aperture.is_some()
            })
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=STUDIO_SCHEMA_VERSION).contains(&self.schema_version) {
            return Err(format!("schemaVersion {} no soportado", self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err("El nombre de la secuencia no puede estar vacío".into());
        }
        if !self.duration.is_finite()
            || !(MIN_PROJECT_DURATION_SEC..=MAX_PROJECT_DURATION_SEC).contains(&self.duration)
        {
            return Err(format!(
                "Duración fuera de rango ({MIN_PROJECT_DURATION_SEC}–{MAX_PROJECT_DURATION_SEC} s): {}",
                self.duration
            ));
        }
        for kf in &self.keyframes {
            if !kf.time.is_finite() || kf.time < 0.0 || kf.time > self.duration + 1e-3 {
                return Err(format!("{}: tiempo {} fuera de la duración", kf.label, kf.time));
            }
            kf.raw_pose.to_args()?;
            for (name, val) in [
                ("Zoom", kf.look.zoom),
                ("Exposure", kf.look.exposure),
                ("FocalDistance", kf.look.focal_distance),
                ("Aperture", kf.look.aperture),
            ] {
                if let Some(v) = val.filter(|v| !in_range(name, *v)) {
                    return Err(format!("{}: {name} fuera de rango ({v})", kf.label));
                }
            }
        }
        for ef in &self.effects {
            if !ef.start_time.is_finite() || ef.start_time < 0.0 || ef.start_time > self.duration + 1e-3 {
                return Err(format!("Efecto {}: inicio {} fuera de la duración", ef.id, ef.start_time));
            }
            if !ef.duration.is_finite() || ef.duration <= 0.0 {
                return Err(format!("Efecto {}: duración inválida ({})", ef.id, ef.duration));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kf(id: &str, time: f32, x: f32) -> Keyframe {
        Keyframe {
            id: id.into(),
            label: format!("Punto {id}"),
            time,
            raw_pose: Pose { pos: [x, 1.6, 2.0], rot: [0.0, 180.0, 0.0] },
            local_pose: None,
            captured_at_ms: 1000,
            transition: Transition::Smooth,
            custom_curve: None,
            knot_mode: KnotMode::PassThrough,
            look: LookTrackPoint::default(),
        }
    }

    #[test]
    fn scaling_duration_scales_times_without_altering_coordinates() {
        let mut p = SequenceProject::new("p1", "Test", 10.0);
        p.keyframes.push(sample_kf("A", 0.0, 1.234));
        p.keyframes.push(sample_kf("B", 5.0, 5.678));
        p.keyframes.push(sample_kf("C", 10.0, 9.012));

        p.set_duration(20.0, true);
        assert_eq!(p.duration, 20.0);
        assert_eq!(p.keyframes[0].time, 0.0);
        assert_eq!(p.keyframes[1].time, 10.0);
        assert_eq!(p.keyframes[2].time, 20.0);
        // Las coordenadas originales permanecen idénticas al bit
        assert_eq!(p.keyframes[0].raw_pose.pos[0], 1.234);
        assert_eq!(p.keyframes[1].raw_pose.pos[0], 5.678);
        assert_eq!(p.keyframes[2].raw_pose.pos[0], 9.012);
    }

    #[test]
    fn detects_advanced_features_for_lossless_mode_indicator() {
        let mut p = SequenceProject::new("p1", "Simple", 10.0);
        p.keyframes.push(sample_kf("A", 0.0, 0.0));
        assert!(!p.has_advanced_features());

        p.keyframes[0].look.zoom = Some(60.0);
        assert!(p.has_advanced_features());
    }

    #[test]
    fn target_provider_serializes_with_source_and_confidence() {
        let mut p = SequenceProject::new("p-target", "Seguimiento", 8.0);
        p.target = Some(TargetConfig {
            enabled: true,
            source: TargetSourceKind::StaticCalibratedPoint,
            world_pos: [1.2, 1.65, -0.8],
            blend: 0.85,
            confidence: 1.0,
            captured_at_ms: 1729780000,
        });
        assert!(p.has_advanced_features());
        let json = serde_json::to_string(&p).unwrap();
        let round: SequenceProject = serde_json::from_str(&json).unwrap();
        assert_eq!(round.target, p.target);
    }

    #[test]
    fn increasing_duration_preserves_keyframe_and_effect_times_untouched() {
        let mut p = SequenceProject::new("p-ext", "Extendido", DEFAULT_PROJECT_DURATION_SEC);
        assert_eq!(p.duration, 10.0);
        assert_eq!(p.duration_unit, DurationUnit::Seconds);
        assert_eq!(p.duration_input_mode, DurationInputMode::ValueUnit);

        p.keyframes.push(sample_kf("A", 0.0, 1.0));
        p.keyframes.push(sample_kf("B", 4.5, 2.0));
        p.keyframes.push(sample_kf("C", 10.0, 3.0));
        p.effects.push(EffectClip {
            id: "ef-1".into(),
            kind: EffectKind::DollyIn,
            track: 0,
            start_time: 2.0,
            duration: 3.5,
            intensity: 1.0,
            speed: 1.0,
            param_a: Some(0.8),
            param_b: None,
            easing: Transition::Smooth,
            custom_curve: None,
            enabled: true,
        });

        // Ampliar a 30s, 1 min (60s), 1.5 min (90s) y 5 min (300s) conserva intactos los tiempos originales
        for target_dur in [30.0, 60.0, 90.0, 300.0] {
            p.apply_duration_change(target_dur, DurationChangeStrategy::Preserve)
                .unwrap();
            assert_eq!(p.duration, target_dur);
            assert_eq!(p.keyframes[0].time, 0.0);
            assert_eq!(p.keyframes[1].time, 4.5);
            assert_eq!(p.keyframes[2].time, 10.0);
            assert_eq!(p.effects[0].start_time, 2.0);
            assert_eq!(p.effects[0].duration, 3.5);
            assert!(p.validate().is_ok());
        }
    }

    #[test]
    fn reducing_duration_detects_out_of_range_elements_and_resolves_safely() {
        let mut p = SequenceProject::new("p-red", "Reducción", 60.0);
        p.keyframes.push(sample_kf("A", 0.0, 1.0));
        p.keyframes.push(sample_kf("B", 15.0, 2.0));
        p.keyframes.push(sample_kf("C", 45.0, 3.0));
        p.effects.push(EffectClip {
            id: "ef-late".into(),
            kind: EffectKind::SmoothZoom,
            track: 0,
            start_time: 25.0,
            duration: 12.0, // termina en 37.0s
            intensity: 1.0,
            speed: 1.0,
            param_a: Some(35.0),
            param_b: Some(65.0),
            easing: Transition::Smooth,
            custom_curve: None,
            enabled: true,
        });

        let report = p.out_of_range_elements(30.0);
        assert!(report.has_conflicts());
        assert_eq!(report.keyframes, 1); // Punto C en 45s
        assert_eq!(report.effects, 1); // ef-late termina en 37s

        // Preserve debe rechazar el cambio para no perder ni recortar silenciosamente
        assert!(p
            .apply_duration_change(30.0, DurationChangeStrategy::Preserve)
            .is_err());
        assert_eq!(p.duration, 60.0);
        assert_eq!(p.keyframes[2].time, 45.0);

        // ClampToRange ajusta únicamente los elementos que exceden 30s y conserva intactos los demás
        p.apply_duration_change(30.0, DurationChangeStrategy::ClampToRange)
            .unwrap();
        assert_eq!(p.duration, 30.0);
        assert_eq!(p.keyframes[0].time, 0.0);
        assert_eq!(p.keyframes[1].time, 15.0);
        assert_eq!(p.keyframes[2].time, 30.0);
        assert_eq!(p.effects[0].start_time, 25.0);
        assert_eq!(p.effects[0].duration, 5.0);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn legacy_projects_without_duration_unit_or_mode_deserialize_compatibly() {
        let legacy_json = r#"{
            "schemaVersion": 1,
            "id": "legacy-seq",
            "name": "Proyecto Antiguo",
            "keyframes": [],
            "effects": []
        }"#;
        let loaded: SequenceProject = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(loaded.duration, 10.0);
        assert_eq!(loaded.duration_unit, DurationUnit::Seconds);
        assert_eq!(loaded.duration_input_mode, DurationInputMode::ValueUnit);
        assert!(loaded.validate().is_ok());
    }
}
