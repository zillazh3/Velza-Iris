//! Módulo de integración con el asset de avatar para VRChat (VRCFury).
//! Separa estrictamente el protocolo de comunicación, el enrutamiento de órdenes,
//! el adaptador del rig de huesos y la sincronización de estado.

pub mod config;
pub mod protocol;
pub mod rig_adapter;
pub mod router;
pub mod service;
pub mod state_adapter;
pub mod validator;

#[cfg(test)]
mod tests;
