use std::collections::VecDeque;

use bevy::{
    ecs::{query::QuerySingleError, schedule::ShouldRun},
    prelude::*,
    utils::HashSet,
};
use bevy_inspector_egui::{Inspectable, InspectorPlugin};
use projekto_camera::orbit::{OrbitCamera, OrbitCameraConfig};
use projekto_core::{chunk, voxel};

use crate::world::{
    debug::{DrawVoxels, RaycastDebug},
    rendering::{ChunkMaterial, ChunkMaterialHandle},
    terraformation::prelude::WorldRes,
};
pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterControllerConfig>()
            .init_resource::<CharacterPosition>()
            .add_plugin(InspectorPlugin::<CharacterPosition>::new())
            .add_system_set(
                SystemSet::new()
                    .with_run_criteria(is_active)
                    .with_system(move_character)
                    .with_system(sync_rotation)
                    .with_system(update_character_position.label(CharacterPositionUpdate))
                    .with_system(update_view_frustum.after(CharacterPositionUpdate))
                    .label(CharacterUpdate),
            );
    }
}

#[derive(SystemLabel)]
pub struct CharacterUpdate;

#[derive(SystemLabel)]
pub struct CharacterPositionUpdate;

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

#[derive(Default, Debug, Reflect, Deref, DerefMut, Inspectable)]
pub struct CharacterPosition(IVec3);

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

fn update_character_position(
    handle: Res<ChunkMaterialHandle>,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    mut position: ResMut<CharacterPosition>,
    q: Query<&Transform, (With<CharacterController>, Changed<Transform>)>,
) {
    let transform = match q.get_single() {
        Ok(t) => t,
        Err(_) => return,
    };

    if projekto_core::math::floor(transform.translation) != **position {
        **position = projekto_core::math::floor(transform.translation);

        if let Some(mut material) = materials.get_mut(&handle.0) {
            let (origin, map) = calc_clip_map(transform.translation);
            material.clip_map_origin = origin;
            material.clip_map = map;
            material.clip_height = position.y as f32;
        }
    }
}

fn calc_clip_map(position: Vec3) -> (Vec2, [Vec4; chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE]) {
    let offset = Vec2::new(
        chunk::X_AXIS_SIZE as f32 / 2.0,
        chunk::Z_AXIS_SIZE as f32 / 2.0,
    );
    let origin = Vec2::new(position.x, position.z) - offset;

    (
        origin,
        [Vec4::splat(position.y.floor()); chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE],
    )
}

fn update_view_frustum(
    world_res: Res<WorldRes>,
    position: Res<CharacterPosition>,
    q: Query<&Transform, With<CharacterController>>,
    mut debug: Local<Option<(Entity, Entity, Entity)>>,
    mut commands: Commands,
) {
    if position.is_changed() == false || world_res.is_ready() == false {
        return;
    }

    if debug.is_none() {
        *debug = Some((
            commands.spawn().insert(Name::new("Raycast Front")).id(),
            commands.spawn().insert(Name::new("Raycast Up")).id(),
            commands.spawn().insert(Name::new("Flood Fill")).id(),
        ));
    }

    let forward = projekto_core::math::to_dir(q.single().forward());
    let front_world = (forward + **position).as_vec3();

    let local = chunk::to_local(front_world);

    let chunk = if let Some(chunk) = world_res.get(local) {
        chunk
    } else {
        warn!(
            "Unable to update view frustum. Chunk not found at {:?}",
            local
        );
        return;
    };

    let front_voxel = voxel::to_local(front_world);

    let front = chunk.kinds.get_absolute(front_voxel).unwrap_or_default();

    commands.entity(debug.unwrap().0).insert(RaycastDebug {
        origin: position.as_vec3() + Vec3::splat(0.5),
        dir: forward.as_vec3(),
        range: 1.0,
    });

    let above_voxel = voxel::to_local(position.as_vec3() + Vec3::Y);

    commands.entity(debug.unwrap().1).insert(RaycastDebug {
        origin: position.as_vec3() + Vec3::splat(0.5),
        dir: Vec3::Y,
        range: 1.0,
    });

    if front.is_opaque() == true {
        // Facing a wall. Does nothing
        return;
    }

    let above = chunk.kinds.get_absolute(above_voxel).unwrap_or_default();

    // TODO: Check many blocks using view frustum
    if above.is_opaque() == false {
        // We aren't inside any building. Skip
        return;
    }

    info!(
        "Update view frustum. Voxel: {:?} - {:?}",
        front_voxel, above
    );

    let mut queue = VecDeque::new();
    queue.push_back(front_world);

    let mut flooded_voxels = vec![];
    let mut walked = HashSet::default();

    while let Some(voxel_world) = queue.pop_front() {
        for side in voxel::SIDES {
            // Let's work with X, Z axis only for now.
            if matches!(side, voxel::Side::Up | voxel::Side::Down) {
                continue;
            }
            let next_voxel = voxel_world + side.dir().as_vec3();

            if walked.contains(&next_voxel.as_ivec3()) {
                continue;
            }

            let chunk_local = chunk::to_local(next_voxel);
            let voxel = voxel::to_local(next_voxel);

            let chunk = if let Some(chunk) = world_res.get(chunk_local) {
                chunk
            } else {
                continue;
            };

            let kind = chunk.kinds.get(voxel);

            if kind.is_opaque() {
                continue;
            }

            let light = chunk.lights.get(voxel);

            if light.get(voxel::LightTy::Natural) == voxel::Light::MAX_NATURAL_INTENSITY {
                // This means it's an outside voxel, so skip it since there is no roof on top.
                continue;
            }

            flooded_voxels.push(next_voxel);
            queue.push_back(next_voxel);
            walked.insert(next_voxel.as_ivec3());
        }
    }

    info!("Flooded: {} voxels.", flooded_voxels.len());

    if flooded_voxels.len() == 0 {
        commands
            .entity(debug.unwrap().2)
            .insert(DrawVoxels::default());
    } else if flooded_voxels.len() > 0 {
        let offset = flooded_voxels[0];
        let draw_voxels = DrawVoxels {
            color: "pink".into(),
            voxels: flooded_voxels.into_iter().map(|v| v.as_ivec3()).collect(),
            offset,
        };

        commands.entity(debug.unwrap().2).insert(draw_voxels);
    }
}
