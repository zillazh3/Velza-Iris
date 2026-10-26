//! Módulo de compatibilidad e integración con el Rig Visualizador de VRChat (Avatars 3.0 / Unity).
//!
//! Implementa:
//! - Los 11 endpoints canónicos estrictos de OSC.
//! - Normalización matemática respecto al Ancla y división angular modular.
//! - Protocolo de multiplexación temporal de 5 huesos (`SyncPathMeshAsync`) no bloqueante.
//! - Sincronización en tiempo real a 60 Hz de la cámara visualizadora vía OSC Bundles.

pub mod config;
pub mod protocol;
pub mod service;

#[cfg(test)]
mod tests;

pub use config::VisualizerConfig;
pub use protocol::*;
pub use service::{VisualizerService, VisualizerSnapshot};
