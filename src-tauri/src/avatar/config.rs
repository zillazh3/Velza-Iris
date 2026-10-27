//! Configuración del módulo de integración con el visualizador de avatar de VRChat.

use super::service::MuxTiming;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizerConfig {
    pub max_range_meters: f32,
    pub max_angle_degrees: f32,
    pub auto_sync_on_scrub: bool,
    /// Esperas del multiplexor de huesos (`MuxTiming::REMOTE` por defecto: los params son synced).
    pub mux_timing: MuxTiming,
    /// Cada cuánto se reenvía el path a los remotos aunque no cambie (los que entran tarde no tienen historial).
    pub keepalive_ms: u64,
    /// Tras detectar que entró un jugador, el path se reenvía en ciclos seguidos durante este tiempo.
    pub join_boost_ms: u64,
}

impl Default for VisualizerConfig {
    fn default() -> Self {
        VisualizerConfig {
            max_range_meters: super::protocol::MAX_RANGE_METERS,
            max_angle_degrees: super::protocol::MAX_ANGLE_DEGREES,
            auto_sync_on_scrub: true,
            mux_timing: MuxTiming::REMOTE,
            keepalive_ms: 30_000,
            // ponytail: ventana fija; medir cuánto tarda en cargar el avatar un remoto real y ajustar.
            join_boost_ms: 90_000,
        }
    }
}
