//! Adaptador de estado de salida para sincronizar con los parámetros de expresión del avatar.

use super::protocol::*;
use rosc::{OscMessage, OscPacket, OscType};

#[derive(Clone, Debug, Default)]
pub struct AssetStateAdapter {
    pub last_state: Option<AvatarIntegrationState>,
    pub last_progress: Option<f32>,
    pub last_connected: Option<bool>,
}

impl AssetStateAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construye paquetes OSC para sincronizar el estado actual con el avatar,
    /// deduplicando valores idénticos para evitar saturación de la red UDP.
    pub fn sync_state_packets(
        &mut self,
        state: AvatarIntegrationState,
        progress: f32,
        connected: bool,
    ) -> Vec<OscPacket> {
        let mut packets = Vec::new();

        // 1. Estado del sistema
        if self.last_state != Some(state) {
            self.last_state = Some(state);
            packets.push(OscPacket::Message(OscMessage {
                addr: ADDR_OUT_STATE.into(),
                args: vec![OscType::Int(state as i32)],
            }));
        }

        // 2. Progreso normalizado [0.0, 1.0]
        let safe_p = if progress.is_finite() { progress.clamp(0.0, 1.0) } else { 0.0 };
        let should_send_p = match self.last_progress {
            Some(prev) => (prev - safe_p).abs() >= 0.005 || safe_p == 0.0 || safe_p == 1.0,
            None => true,
        };
        if should_send_p {
            self.last_progress = Some(safe_p);
            packets.push(OscPacket::Message(OscMessage {
                addr: ADDR_OUT_PROGRESS.into(),
                args: vec![OscType::Float(safe_p)],
            }));
        }

        // 3. Confirmación de conexión
        if self.last_connected != Some(connected) {
            self.last_connected = Some(connected);
            packets.push(OscPacket::Message(OscMessage {
                addr: ADDR_OUT_CONNECTED.into(),
                args: vec![OscType::Bool(connected)],
            }));
        }

        packets
    }

    /// Genera el paquete de eco para resetear el parámetro de acción en el avatar (liberar botón radial).
    pub fn build_action_reset_packet() -> OscPacket {
        OscPacket::Message(OscMessage {
            addr: ADDR_ACTION.into(),
            args: vec![OscType::Int(AvatarAction::Inactive as i32)],
        })
    }
}
