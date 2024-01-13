use bevy::{ecs::query::QuerySingleError, prelude::*};
use projekto_camera::orbit::{OrbitCamera, OrbitCameraConfig};
use projekto_core::{chunk, landscape};

pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterControllerConfig>()
            .init_resource::<CharacterPosition>()
            .add_plugins(bevy_inspector_egui::quick::ResourceInspectorPlugin::<
                CharacterPosition,
            >::new())
            .init_resource::<ChunkMaterialImage>()
            .register_type::<ChunkMaterialImage>()
            .add_systems(
                Update,
                ((
                    move_character,
                    sync_rotation,
                    update_character_position.in_set(CharacterPositionUpdate),
                )
                    .in_set(CharacterUpdate)
                    .run_if(is_active),),
            );
    }
}

#[derive(Debug, SystemSet, Hash, Clone, Copy, PartialEq, Eq)]
pub struct CharacterUpdate;

#[derive(Debug, SystemSet, Hash, Clone, Copy, PartialEq, Eq)]
pub struct CharacterPositionUpdate;

#[derive(Component, Default, Reflect)]
pub struct CharacterController;

#[derive(Resource)]
pub struct CharacterControllerConfig {
    pub active: bool,
    pub move_speed: f32,
}

impl Default for CharacterControllerConfig {
    fn default() -> Self {
        Self {
            active: true,
            move_speed: 10.0,
        }
    }
}

#[derive(Default, Debug, Reflect, Deref, DerefMut, Resource)]
pub struct ChunkMaterialImage(pub Handle<Image>);

#[derive(Default, Debug, Reflect, Deref, DerefMut, Resource)]
pub struct CharacterPosition(IVec3);

fn is_active(
    char_config: Res<CharacterControllerConfig>,
    cam_config: Res<OrbitCameraConfig>,
) -> bool {
    char_config.active && cam_config.active
}

fn sync_rotation(
    q_cam: Query<
        &Transform,
        (
            With<OrbitCamera>,
            Without<CharacterController>,
            Changed<Transform>,
        ),
    >,
    mut q: Query<&mut Transform, With<CharacterController>>,
) {
    let cam_transform = match q_cam.get_single() {
        Ok(t) => t,
        Err(_) => return,
    };

    let mut transform = match q.get_single_mut() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(_)) => return,
        Err(QuerySingleError::MultipleEntities(_)) => {
            panic!("There can be only one character controlled entity.")
        }
    };

    let (y, _, _) = cam_transform.rotation.to_euler(EulerRot::YXZ);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, y, 0.0, 0.0);
}

fn move_character(
    config: Res<CharacterControllerConfig>,
    time: Res<Time>,
    input: Res<Input<KeyCode>>,
    mut q: Query<&mut Transform, With<CharacterController>>,
) {
    let input_vec = calc_input_vector(&input);

    if input_vec == Vec3::ZERO {
        return;
    }

    let mut transform = match q.get_single_mut() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(_)) => return,
        Err(QuerySingleError::MultipleEntities(_)) => {
            panic!("There can be only one character controlled entity.")
        }
    };

    let forward_vector = transform.forward() * input_vec.z;
    let right_vector = transform.right() * input_vec.x;
    let up_vector = Vec3::Y * input_vec.y;

    let move_vector = forward_vector + right_vector + up_vector;

    transform.translation += config.move_speed * time.delta_seconds() * move_vector;
}

fn calc_input_vector(input: &Res<Input<KeyCode>>) -> Vec3 {
    let mut res = Vec3::ZERO;

    if input.pressed(KeyCode::W) {
        res.z += 1.0
    }

    if input.pressed(KeyCode::S) {
        res.z -= 1.0
    }

    if input.pressed(KeyCode::D) {
        res.x += 1.0
    }

    if input.pressed(KeyCode::A) {
        res.x -= 1.0
    }

    if input.pressed(KeyCode::Space) {
        res.y += 1.0
    }

    if input.pressed(KeyCode::ControlLeft) {
        res.y -= 1.0
    }

    res
}

fn update_character_position(
    mut position: ResMut<CharacterPosition>,
    q: Query<&Transform, (With<CharacterController>, Changed<Transform>)>,
) {
    let transform = match q.get_single() {
        Ok(t) => t,
        Err(_) => return,
    };

    if projekto_core::math::floor(transform.translation) != **position {
        **position = projekto_core::math::floor(transform.translation);
    }
}

// fn calc_clip_map(position: Vec3) -> (Vec2, [Vec4; chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE]) {
//     let offset = Vec2::new(
//         chunk::X_AXIS_SIZE as f32 / 2.0,
//         chunk::Z_AXIS_SIZE as f32 / 2.0,
//     );
//     let origin = Vec2::new(position.x, position.z) - offset;

//     (
//         origin,
//         [Vec4::splat(position.y.floor()); chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE],
//     )
// }

enum ViewFrustumChain {
    DoNothing,
    ClipMaterial(IVec3, Vec<Vec3>),
    RevertMaterial,
}

const X_AXIS: usize = landscape::HORIZONTAL_SIZE * chunk::Z_AXIS_SIZE;

fn is_on_landscape_bounds(coords: IVec2) -> bool {
    coords.x >= 0 && coords.x < X_AXIS as i32 && coords.y >= 0 && coords.y < X_AXIS as i32
}

fn pack_landscape_coords(coords: IVec2) -> usize {
    coords.x as usize * X_AXIS + coords.y as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_landscape_coords() {
        assert_eq!(super::pack_landscape_coords(IVec2::new(0, 0)), 0);
        assert_eq!(super::pack_landscape_coords(IVec2::new(0, 1)), 1);
        assert_eq!(super::pack_landscape_coords(IVec2::new(0, 2)), 2);
        assert_eq!(super::pack_landscape_coords(IVec2::new(0, 3)), 3);

        assert_eq!(
            super::pack_landscape_coords(IVec2::new(1, 0)),
            super::X_AXIS
        );
        assert_eq!(
            super::pack_landscape_coords(IVec2::new(2, 0)),
            2 * super::X_AXIS
        );
        assert_eq!(
            super::pack_landscape_coords(IVec2::new(3, 0)),
            3 * super::X_AXIS
        );

        assert_eq!(
            super::pack_landscape_coords(IVec2::new(1, 1)),
            super::X_AXIS + 1
        );
        assert_eq!(
            super::pack_landscape_coords(IVec2::new(2, 2)),
            2 * super::X_AXIS + 2
        );
        assert_eq!(
            super::pack_landscape_coords(IVec2::new(3, 3)),
            3 * super::X_AXIS + 3
        );
    }
}
