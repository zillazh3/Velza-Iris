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
}

impl Default for VisualizerConfig {
    fn default() -> Self {
        VisualizerConfig {
            max_range_meters: super::protocol::MAX_RANGE_METERS,
            max_angle_degrees: super::protocol::MAX_ANGLE_DEGREES,
            auto_sync_on_scrub: true,
            mux_timing: MuxTiming::REMOTE,
        }
    }
}
