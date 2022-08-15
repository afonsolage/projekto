use bevy::{
    ecs::{query::QuerySingleError, schedule::ShouldRun},
    prelude::*,
};
use projekto_camera::orbit::{OrbitCamera, OrbitCameraConfig};

use crate::world::rendering::{ChunkMaterial, ChunkMaterialHandle};
pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterControllerConfig>()
            .add_system_set(
                SystemSet::new()
                    .with_run_criteria(is_active)
                    .with_system(move_character)
                    .with_system(sync_rotation)
                    .with_system(update_clip_height)
                    .label(CharacterUpdate),
            );
    }
}

#[derive(SystemLabel)]
pub struct CharacterUpdate;

#[derive(Component, Default, Reflect)]
pub struct CharacterController;

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

fn is_active(
    char_config: Res<CharacterControllerConfig>,
    cam_config: Res<OrbitCameraConfig>,
) -> ShouldRun {
    if char_config.active && cam_config.active {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
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

    if input.pressed(KeyCode::LControl) {
        res.y -= 1.0
    }

    res
}

fn update_clip_height(
    handle: Res<ChunkMaterialHandle>,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    mut last_height: Local<f32>,
    q: Query<&Transform, (With<CharacterController>, Changed<Transform>)>,
) {
    let transform = match q.get_single() {
        Ok(t) => t,
        Err(_) => return,
    };

    if transform.translation.y.floor() != *last_height {
        *last_height = transform.translation.y.floor();

        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.clip_height = *last_height + 2.0;
        }
    }
}