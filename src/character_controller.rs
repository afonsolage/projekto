use std::collections::VecDeque;

use bevy::{
    ecs::{query::QuerySingleError, schedule::ShouldRun},
    math::Vec3Swizzles,
    prelude::*,
    utils::HashSet,
};
use bevy_inspector_egui::{Inspectable, InspectorPlugin};
use projekto_camera::orbit::{OrbitCamera, OrbitCameraConfig};
use projekto_core::{chunk, voxel, landscape};

use crate::world::{
    debug::DrawVoxels,
    rendering::{ChunkMaterial, ChunkMaterialHandle},
    terraformation::prelude::WorldRes,
};
pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterControllerConfig>()
            .init_resource::<CharacterPosition>()
            .add_plugin(InspectorPlugin::<CharacterPosition>::new())
            .init_resource::<ChunkMaterialImage>()
            .register_type::<ChunkMaterialImage>()
            .add_system(sync_material_image)
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

#[derive(Default, Debug, Reflect, Deref, DerefMut)]
pub struct ChunkMaterialImage(pub Handle<Image>);

fn sync_material_image(material: Res<ChunkMaterialHandle>, materials: Res<Assets<ChunkMaterial>>, mut image_handle: ResMut<ChunkMaterialImage>) {
    if material.is_changed() {
        **image_handle = materials.get(&material).unwrap().clip_map.clone();
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
    // TODO: Check on correct chunk
    let above = chunk.lights.get_absolute(above_voxel).unwrap_or_default();

    // TODO: Check many blocks using view frustum
    if above.get(voxel::LightTy::Natural) == voxel::Light::MAX_NATURAL_INTENSITY {
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

    ViewFrustumChain::ClipMaterial(**position, flooded_voxels)
}

fn update_chunk_material(
    In(voxels): In<ViewFrustumChain>,
    chunk_material_handle: Res<ChunkMaterialHandle>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    mut commands: Commands,
    mut meta: Local<Option<Entity>>,
) {
    if meta.is_none() {
        *meta = Some(commands.spawn().insert(Visibility {is_visible: false}).id());
    }

    match voxels {
        ViewFrustumChain::DoNothing => return,
        ViewFrustumChain::RevertMaterial => {
            trace!("Revert!");
            if let Some(material) = materials.get_mut(&chunk_material_handle) 
                && let Some(image) = images.get_mut(&material.clip_map) {
                material.clip_map_origin = Vec2::ZERO;
                material.clip_height = f32::MAX;
                material.show_back_faces = false;

                image.data.fill(0);
            }

            commands.entity(meta.unwrap()).insert(DrawVoxels::default());
        }
        ViewFrustumChain::ClipMaterial(char_pos, voxels_world) => {
            trace!("Clip!");

            commands.entity(meta.unwrap()).insert(DrawVoxels {
                color: "pink".into(),
                voxels: voxels_world.iter().map(Vec3::as_ivec3).collect(),
                offset: voxels_world[0],
                visible: false,
            });

            if let Some(material) = materials.get_mut(&chunk_material_handle)
                && let Some(image) = images.get_mut(&material.clip_map) {
                    let char_chunk = chunk::to_local(char_pos.as_vec3());
                    let left_bottom_chunk = char_chunk - IVec3::splat(landscape::HORIZONTAL_RADIUS as i32);
                    
                    let clip_origin = chunk::to_world(left_bottom_chunk).xz();
                    let clip_height = char_pos.y as f32;

                    material.clip_height = clip_height;
                    material.clip_map_origin = clip_origin;
                    material.show_back_faces = true;

                    let len = image.data.len();
                    let mut data = vec![0; len];

                    
                    for voxel in voxels_world {
                        if voxel.y > clip_height {
                            continue;
                        }

                        let coords = (voxel.xz() - clip_origin).as_ivec2();
                        if is_on_landscape_bounds(coords) {
                            let idx = pack_landscape_coords(coords);
                            if voxel.y > data[idx] as f32 {
                                data[idx] = voxel.y as u8;
                            }
                        }
                    }

                    image.data = data;
            }
        }
    }
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

        assert_eq!(super::pack_landscape_coords(IVec2::new(1, 0)), super::X_AXIS);
        assert_eq!(super::pack_landscape_coords(IVec2::new(2, 0)), 2 * super::X_AXIS);
        assert_eq!(super::pack_landscape_coords(IVec2::new(3, 0)), 3 * super::X_AXIS);

        assert_eq!(super::pack_landscape_coords(IVec2::new(1, 1)), super::X_AXIS + 1);
        assert_eq!(super::pack_landscape_coords(IVec2::new(2, 2)), 2 * super::X_AXIS + 2);
        assert_eq!(super::pack_landscape_coords(IVec2::new(3, 3)), 3 * super::X_AXIS + 3);
    }
}