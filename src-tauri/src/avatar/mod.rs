//! Módulo de compatibilidad e integración con el Rig Visualizador de VRChat (Avatars 3.0 / Unity).
//!
//! Implementa:
//! - Direcciones OSC canónicas: canal del dueño (no synced) y bus sincronizado hacia los remotos.
//! - Normalización matemática respecto al Ancla y división angular modular.
//! - Bus time-sliced con coarse + fine, región móvil y progreso `Path_T` para los remotos (`net`).
//! - Sincronización en tiempo real a 60 Hz de la cámara visualizadora vía OSC Bundles.

pub mod config;
pub mod net;
pub mod protocol;
pub mod service;

#[cfg(test)]
mod tests;

pub use config::VisualizerConfig;
pub use protocol::*;
pub use service::{VisualizerService, VisualizerSnapshot};
