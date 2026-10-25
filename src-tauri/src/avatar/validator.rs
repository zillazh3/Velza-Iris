//! Validador de parámetros OSC, deduplicación y detección de flanco con rearme.

use super::protocol::*;
use rosc::OscMessage;

#[derive(Clone, Debug, PartialEq)]
pub enum ValidatedAvatarCommand {
    Action(AvatarAction),
    Duration(f32),
    AnchorMode(AvatarAnchorMode),
    PathType(AvatarPathType),
    Loop(bool),
    AssetVisible(bool),
    AvatarChange(String),
}

#[derive(Clone, Debug)]
pub struct ParameterValidator {
    pub last_action: AvatarAction,
    pub action_armed: bool,
    pub last_duration_norm: Option<f32>,
    pub last_anchor_mode: Option<AvatarAnchorMode>,
    pub last_path_type: Option<AvatarPathType>,
    pub last_loop: Option<bool>,
    pub last_asset_visible: Option<bool>,
    pub last_avatar_id: Option<String>,
}

impl Default for ParameterValidator {
    fn default() -> Self {
        ParameterValidator {
            last_action: AvatarAction::Inactive,
            action_armed: true,
            last_duration_norm: None,
            last_anchor_mode: None,
            last_path_type: None,
            last_loop: None,
            last_asset_visible: None,
            last_avatar_id: None,
        }
    }
}

impl ParameterValidator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Valida un mensaje OSC recibido y determina si corresponde a una orden válida y no redundante.
    pub fn validate_message(&mut self, m: &OscMessage) -> Result<Option<ValidatedAvatarCommand>, String> {
        let addr = m.addr.as_str();

        if addr == ADDR_AVATAR_CHANGE {
            let Some(arg) = m.args.first() else {
                return Err("Mensaje /avatar/change sin argumentos".into());
            };
            let Some(avatar_id) = extract_osc_string(arg) else {
                return Err("Argumento inválido en /avatar/change (esperaba String)".into());
            };
            if self.last_avatar_id.as_ref() == Some(&avatar_id) {
                return Ok(None);
            }
            self.last_avatar_id = Some(avatar_id.clone());
            // Un cambio de avatar re-arma la detección de acciones
            self.action_armed = true;
            return Ok(Some(ValidatedAvatarCommand::AvatarChange(avatar_id)));
        }

        if !addr.starts_with("/avatar/parameters/Iris_") {
            return Ok(None);
        }

        let Some(first_arg) = m.args.first() else {
            return Err(format!("Parámetro {} sin argumentos", addr));
        };

        match addr {
            ADDR_ACTION => {
                let Some(raw_val) = extract_osc_int(first_arg) else {
                    return Err(format!("Iris_Action requiere un entero, recibido: {:?}", first_arg));
                };
                let Some(action) = AvatarAction::from_i32(raw_val) else {
                    return Err(format!("Iris_Action fuera de rango [0..6]: {}", raw_val));
                };

                // Lógica de detección de flanco y rearme:
                // Si la acción es Inactivo (0), se rearma el disparador
                if action == AvatarAction::Inactive {
                    self.last_action = AvatarAction::Inactive;
                    self.action_armed = true;
                    return Ok(None);
                }

                // Si ya se disparó esta acción y no ha habido rearme previo, ignorar mensaje duplicado
                if !self.action_armed && self.last_action == action {
                    return Ok(None);
                }

                self.last_action = action;
                self.action_armed = false;
                Ok(Some(ValidatedAvatarCommand::Action(action)))
            }
            ADDR_DURATION => {
                let Some(f) = extract_osc_float(first_arg) else {
                    return Err(format!("Iris_Duration requiere un flotante, recibido: {:?}", first_arg));
                };
                if !f.is_finite() {
                    return Err("Iris_Duration no finito (NaN o Infinito)".into());
                }
                let clamped = f.clamp(0.0, 1.0);
                if let Some(prev) = self.last_duration_norm {
                    if (prev - clamped).abs() < 1e-3 {
                        return Ok(None);
                    }
                }
                self.last_duration_norm = Some(clamped);
                Ok(Some(ValidatedAvatarCommand::Duration(clamped)))
            }
            ADDR_ANCHOR_MODE => {
                let Some(raw_val) = extract_osc_int(first_arg) else {
                    return Err(format!("Iris_AnchorMode requiere un entero, recibido: {:?}", first_arg));
                };
                let Some(mode) = AvatarAnchorMode::from_i32(raw_val) else {
                    return Err(format!("Iris_AnchorMode fuera de rango [0..2]: {}", raw_val));
                };
                if self.last_anchor_mode == Some(mode) {
                    return Ok(None);
                }
                self.last_anchor_mode = Some(mode);
                Ok(Some(ValidatedAvatarCommand::AnchorMode(mode)))
            }
            ADDR_PATH_TYPE => {
                let Some(raw_val) = extract_osc_int(first_arg) else {
                    return Err(format!("Iris_PathType requiere un entero, recibido: {:?}", first_arg));
                };
                let Some(pt) = AvatarPathType::from_i32(raw_val) else {
                    return Err(format!("Iris_PathType fuera de rango [0..2]: {}", raw_val));
                };
                if self.last_path_type == Some(pt) {
                    return Ok(None);
                }
                self.last_path_type = Some(pt);
                Ok(Some(ValidatedAvatarCommand::PathType(pt)))
            }
            ADDR_LOOP => {
                let Some(b) = extract_osc_bool(first_arg) else {
                    return Err(format!("Iris_Loop requiere un booleano, recibido: {:?}", first_arg));
                };
                if self.last_loop == Some(b) {
                    return Ok(None);
                }
                self.last_loop = Some(b);
                Ok(Some(ValidatedAvatarCommand::Loop(b)))
            }
            ADDR_ASSET_VISIBLE => {
                let Some(b) = extract_osc_bool(first_arg) else {
                    return Err(format!("Iris_AssetVisible requiere un booleano, recibido: {:?}", first_arg));
                };
                if self.last_asset_visible == Some(b) {
                    return Ok(None);
                }
                self.last_asset_visible = Some(b);
                Ok(Some(ValidatedAvatarCommand::AssetVisible(b)))
            }
            _ => {
                // Parámetro no reconocido pero bajo el prefijo Iris_
                Err(format!("Parámetro de avatar no reconocido: {}", addr))
            }
        }
    }

    /// Fuerza el rearme manual del disparador de órdenes.
    pub fn force_rearm(&mut self) {
        self.action_armed = true;
        self.last_action = AvatarAction::Inactive;
    }
}
