//! Configuración versionada y persistente para la integración con el avatar de VRChat.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarIntegrationConfig {
    pub schema_version: u32,
    pub enabled: bool,
    pub send_host: String,
    pub send_port: u16,
    pub listen_port: u16,
    pub min_duration: f32,
    pub max_duration: f32,
    pub bone_count: usize,
    pub rig_scale: f32,
    pub coordinate_system: String,
    pub auto_rearm: bool,
    pub log_diagnostics: bool,
}

impl Default for AvatarIntegrationConfig {
    fn default() -> Self {
        AvatarIntegrationConfig {
            schema_version: 1,
            enabled: true,
            send_host: "127.0.0.1".into(),
            send_port: 9000,
            listen_port: 9001,
            min_duration: 1.0,
            max_duration: 30.0,
            bone_count: 16,
            rig_scale: 1.0,
            coordinate_system: "UnityYUp".into(),
            auto_rearm: true,
            log_diagnostics: true,
        }
    }
}
