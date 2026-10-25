//! Enrutador de comandos que despacha órdenes validadas hacia el servicio de integración.

use super::service::IrisIntegrationService;
use super::validator::ValidatedAvatarCommand;

pub struct CommandRouter;

impl CommandRouter {
    /// Despacha un comando validado hacia el servicio de integración del avatar.
    pub fn route(service: &mut IrisIntegrationService, cmd: ValidatedAvatarCommand) -> Result<(), String> {
        match cmd {
            ValidatedAvatarCommand::Action(action) => service.handle_action(action),
            ValidatedAvatarCommand::Duration(norm) => service.handle_duration(norm),
            ValidatedAvatarCommand::AnchorMode(mode) => service.handle_anchor_mode(mode),
            ValidatedAvatarCommand::PathType(pt) => service.handle_path_type(pt),
            ValidatedAvatarCommand::Loop(b) => service.handle_loop(b),
            ValidatedAvatarCommand::AssetVisible(b) => service.handle_asset_visibility(b),
            ValidatedAvatarCommand::AvatarChange(avatar_id) => service.handle_avatar_change(avatar_id),
        }
    }
}
