use bevy::prelude::*;
use projekto_core::coords::Chunk;
use serde::{Deserialize, Serialize};

use crate::{ClientSet, net::ServerConnection};

/// Chunks around the player are loaded in a square of this radius.
///
/// The server caps requests at 32. `PROJEKTO_VIEW_RADIUS` can temporarily lower the value for
/// testing without changing the default view distance.
const DEFAULT_LANDSCAPE_RADIUS: u8 = 32;
const MAX_LANDSCAPE_RADIUS: u8 = 32;
const RADIUS_ENV: &str = "PROJEKTO_VIEW_RADIUS";

pub(crate) struct SendInputPlugin;

impl Plugin for SendInputPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PlayerLandscape>()
            .insert_resource(PlayerLandscape {
                radius: radius_from_env(std::env::var(RADIUS_ENV).ok().as_deref()),
                ..Default::default()
            })
            .add_systems(
                PostUpdate,
                (
                    update_landscape_center,
                    update_player_landscape.run_if(resource_changed::<PlayerLandscape>),
                    send_welcome_message.run_if(resource_added::<ServerConnection>),
                )
                    .chain()
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

#[derive(Resource, Default, Debug, Clone, Copy, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
pub struct PlayerLandscape {
    pub center: IVec2,
    pub radius: u8,
}

fn update_landscape_center(
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut landscape: ResMut<PlayerLandscape>,
) {
    let Some((_, transform)) = cameras.iter().find(|(camera, _)| camera.is_active) else {
        return;
    };

    let chunk = Chunk::from(transform.translation());
    if Chunk::from(landscape.center) != chunk {
        landscape.center = chunk.into();
    }
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
    fn radius_defaults_to_32() {
        assert_eq!(radius_from_env(None), 32);
        assert_eq!(radius_from_env(Some("not-a-number")), 32);
    }

    #[test]
    fn radius_is_clamped_to_supported_bounds() {
        assert_eq!(radius_from_env(Some("0")), 1);
        assert_eq!(radius_from_env(Some("16")), 16);
        assert_eq!(radius_from_env(Some("32")), 32);
        assert_eq!(radius_from_env(Some("255")), MAX_LANDSCAPE_RADIUS);
    }

    #[test]
    fn landscape_center_follows_active_camera() {
        let mut app = App::new();
        app.init_resource::<PlayerLandscape>()
            .add_systems(Update, update_landscape_center);

        app.world_mut().spawn((
            Camera3d::default(),
            Camera {
                is_active: true,
                ..default()
            },
            GlobalTransform::from_translation(Vec3::new(128.0, 256.0, 128.0)),
        ));

        app.update();

        assert_eq!(
            app.world().resource::<PlayerLandscape>().center,
            IVec2::new(8, 8)
        );
    }
}
