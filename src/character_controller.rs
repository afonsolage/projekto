use std::collections::VecDeque;

use bevy::{
    ecs::{query::QuerySingleError, schedule::ShouldRun},
    prelude::*,
    utils::{HashMap, HashSet},
};
use bevy_inspector_egui::{Inspectable, InspectorPlugin};
use projekto_camera::orbit::{OrbitCamera, OrbitCameraConfig};
use projekto_core::{chunk, voxel};

use crate::world::{
    rendering::{ChunkEntityMap, ChunkLocal, ChunkMaterial},
    terraformation::prelude::WorldRes, debug::DrawVoxels,
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
                    .with_system(
                        update_view_frustum
                            .chain(update_chunk_material)
                            .after(CharacterPositionUpdate),
                    )
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
    // handle: Res<ChunkMaterialHandle>,
    // mut materials: ResMut<Assets<ChunkMaterial>>,
    mut position: ResMut<CharacterPosition>,
    q: Query<&Transform, (With<CharacterController>, Changed<Transform>)>,
) {
    let transform = match q.get_single() {
        Ok(t) => t,
        Err(_) => return,
    };

    if projekto_core::math::floor(transform.translation) != **position {
        **position = projekto_core::math::floor(transform.translation);

        // if let Some(mut material) = materials.get_mut(&handle.0) {
        //     material.clip_height = position.y as f32;
        // }
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
    ClipMaterial(f32, Vec<Vec3>),
    RevertMaterial,
}

fn update_view_frustum(
    world_res: Res<WorldRes>,
    position: Res<CharacterPosition>,
    q: Query<&Transform, With<CharacterController>>,
    mut meta: Local<bool>,
) -> ViewFrustumChain {
    if position.is_changed() == false && *meta == false {
        return ViewFrustumChain::DoNothing;
    }

    if world_res.is_ready() == false {
        *meta = true;
        return ViewFrustumChain::DoNothing;
    }

    *meta = false;

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
        return ViewFrustumChain::RevertMaterial;
    };

    let front_voxel = voxel::to_local(front_world);
    let front = chunk.kinds.get_absolute(front_voxel).unwrap_or_default();

    if front.is_opaque() == true {
        // Facing a wall. Does nothing
        trace!("Facing wall");
        return ViewFrustumChain::RevertMaterial;
    }

    let above_voxel = voxel::to_local(position.as_vec3() + Vec3::Y);
    let above = chunk.kinds.get_absolute(above_voxel).unwrap_or_default();

    // TODO: Check many blocks using view frustum
    if above.is_opaque() == false {
        // We aren't inside any building. Skip
        trace!("Not under roof");
        return ViewFrustumChain::RevertMaterial;
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
            if matches!(side, voxel::Side::Up) {
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

            flooded_voxels.push(next_voxel);
            queue.push_back(next_voxel);
            walked.insert(next_voxel.as_ivec3());
        }
    }

    info!("Flooded: {} voxels.", flooded_voxels.len());

    ViewFrustumChain::ClipMaterial(position.y as f32, flooded_voxels)
}

fn update_chunk_material(
    In(voxels): In<ViewFrustumChain>,
    q_chunk: Query<&Handle<ChunkMaterial>, With<ChunkLocal>>,
    chunk_map: Res<ChunkEntityMap>,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    mut flooded: Local<Vec<Handle<ChunkMaterial>>>,
    mut commands: Commands,
    mut meta: Local<Option<Entity>>,
) {
    if meta.is_none() {
        *meta = Some(commands.spawn().id());
    }

    match voxels {
        ViewFrustumChain::DoNothing => return,
        ViewFrustumChain::RevertMaterial => {
            trace!("Revert!");
            for handle in flooded.drain(..) {
                if let Some(mut material) = materials.get_mut(&handle) {
                    material.clip_map = default_clip_map();
                    material.clip_map_origin = Vec2::ZERO;
                    material.clip_height = f32::MAX;
                }
            }

            commands.entity(meta.unwrap()).insert(DrawVoxels::default());
        },
        ViewFrustumChain::ClipMaterial(height, voxels_world) => {
            trace!("Clip!");

            commands.entity(meta.unwrap()).insert(DrawVoxels {
                color: "pink".into(),
                voxels: voxels_world.iter().map(Vec3::as_ivec3).collect(),
                offset: voxels_world[0],
            });

            let chunk_voxels = voxels_world
                .into_iter()
                .map(|world| (chunk::to_local(world), voxel::to_local(world)))
                .fold(HashMap::new(), |mut map, (local, voxel)| {
                    map.entry(local).or_insert(vec![]).push(voxel);
                    map
                });

            for (local, voxels) in chunk_voxels {
                if let Some(e) = chunk_map.get(&local) 
                    && let Ok(handle) = q_chunk.get(*e) 
                    && let Some(mut material) = materials.get_mut(handle) {

                    let chunk_world = chunk::to_world(local);

                    material.clip_height = height;
                    material.clip_map_origin = Vec2::new(chunk_world.x, chunk_world.z);

                    let mut clip_heights = default_clip_map();
                    voxels
                        .into_iter()
                        .map(|v| v.x as usize * chunk::Z_AXIS_SIZE + v.z as usize)
                        .for_each(|idx| clip_heights[idx] = IVec4::splat(1));

                    material.clip_map = clip_heights;

                    flooded.push(handle.clone());
                }
            }
        },
    }
}

fn default_clip_map<T: Default + Copy>() -> [T; chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE] {
    [T::default(); chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE]
}
