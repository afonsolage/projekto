use bevy::prelude::*;

use crate::{ClientSet, net::ServerConnection};

/// Chunks around the player are loaded in a square of this radius.
///
/// The server caps requests at 32. Lowering the default keeps the initial load small while the
/// `PROJEKTO_VIEW_RADIUS` environment variable still allows a larger view distance.
const DEFAULT_LANDSCAPE_RADIUS: u8 = 16;
const MAX_LANDSCAPE_RADIUS: u8 = 32;
const RADIUS_ENV: &str = "PROJEKTO_VIEW_RADIUS";

pub(crate) struct SendInputPlugin;

impl Plugin for SendInputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerLandscape {
            radius: radius_from_env(std::env::var(RADIUS_ENV).ok().as_deref()),
            ..Default::default()
        })
        .add_systems(
            PostUpdate,
            (
                update_player_landscape.run_if(resource_changed::<PlayerLandscape>),
                send_welcome_message.run_if(resource_added::<ServerConnection>),
            )
                .in_set(ClientSet::SendInput),
        );
    }
}

fn radius_from_env(value: Option<&str>) -> u8 {
    value
        .and_then(|value| value.parse::<u8>().ok())
        .map(|radius| radius.clamp(1, MAX_LANDSCAPE_RADIUS))
        .unwrap_or(DEFAULT_LANDSCAPE_RADIUS)
}

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PlayerLandscape {
    pub center: IVec2,
    pub radius: u8,
}

fn update_player_landscape(server: Res<ServerConnection>, landscape: Res<PlayerLandscape>) {
    let PlayerLandscape { center, radius } = *landscape;
    let _ = server
        .channel()
        .send(projekto_messages::LandscapeUpdate { center, radius });
}

fn send_welcome_message(server: Res<ServerConnection>, landscape: Res<PlayerLandscape>) {
    let PlayerLandscape { center, radius } = *landscape;
    let _ = server
        .channel()
        .send(projekto_messages::LandscapeUpdate { center, radius });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_defaults_to_the_smaller_world() {
        assert_eq!(radius_from_env(None), DEFAULT_LANDSCAPE_RADIUS);
        assert_eq!(
            radius_from_env(Some("not-a-number")),
            DEFAULT_LANDSCAPE_RADIUS
        );
    }

    #[test]
    fn radius_is_clamped_to_supported_bounds() {
        assert_eq!(radius_from_env(Some("0")), 1);
        assert_eq!(radius_from_env(Some("32")), 32);
        assert_eq!(radius_from_env(Some("255")), MAX_LANDSCAPE_RADIUS);
    }
}
