use bevy::prelude::*;
use projekto_core::chunk::Chunk;
use projekto_world_client::PlayerLandscape;

pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterControllerConfig>()
            .init_resource::<CharacterPosition>()
            .init_resource::<ChunkMaterialImage>()
            .register_type::<ChunkMaterialImage>()
            .add_systems(
                Update,
                ((
                    move_character,
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
            active: false,
            move_speed: 10.0,
        }
    }
}

#[derive(Default, Debug, Reflect, Deref, DerefMut, Resource)]
pub struct ChunkMaterialImage(pub Handle<Image>);

#[derive(Default, Debug, Reflect, Deref, DerefMut, Resource)]
pub struct CharacterPosition(IVec3);

fn is_active(char_config: Res<CharacterControllerConfig>) -> bool {
    char_config.active
}

fn move_character(
    config: Res<CharacterControllerConfig>,
    time: Res<Time>,
    input: Res<ButtonInput<KeyCode>>,
    mut q: Query<&mut Transform, With<CharacterController>>,
) {
    let input_vec = calc_input_vector(&input);

    if input_vec == Vec3::ZERO {
        return;
    }

    let Ok(mut transform) = q.get_single_mut() else {
        return;
    };

    let forward_vector = transform.forward() * input_vec.z;
    let right_vector = transform.right() * input_vec.x;
    let up_vector = Vec3::Y * input_vec.y;

    let move_vector = forward_vector + right_vector + up_vector;

    transform.translation += config.move_speed * time.delta_seconds() * move_vector;
}

fn calc_input_vector(input: &Res<ButtonInput<KeyCode>>) -> Vec3 {
    let mut res = Vec3::ZERO;

    if input.pressed(KeyCode::KeyW) {
        res.z += 1.0;
    }

    if input.pressed(KeyCode::KeyS) {
        res.z -= 1.0;
    }

    if input.pressed(KeyCode::KeyD) {
        res.x += 1.0;
    }

    if input.pressed(KeyCode::KeyA) {
        res.x -= 1.0;
    }

    if input.pressed(KeyCode::Space) {
        res.y += 1.0;
    }

    if input.pressed(KeyCode::ControlLeft) {
        res.y -= 1.0;
    }

    res
}

fn update_character_position(
    mut landscape: ResMut<PlayerLandscape>,
    q: Query<&Transform, (With<CharacterController>, Changed<Transform>)>,
) {
    let Ok(transform) = q.get_single() else {
        return;
    };

    let new_chunk: Chunk = transform.translation.into();
    let current_chunk: Chunk = landscape.center.into();

    if new_chunk != current_chunk {
        landscape.center = new_chunk.into();
    }
}
