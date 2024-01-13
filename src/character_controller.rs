use std::collections::VecDeque;

use bevy::{ecs::query::QuerySingleError, math::Vec3Swizzles, prelude::*, utils::HashSet};
use projekto_camera::orbit::{OrbitCamera, OrbitCameraConfig};
use projekto_core::{chunk, landscape, voxel};
use projekto_genesis::{ChunkKindRes, ChunkLightRes};

use crate::world::{
    debug::DrawVoxels,
    rendering::{ChunkMaterial, ChunkMaterialHandle},
};
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
                (
                    sync_material_image,
                    (
                        move_character,
                        sync_rotation,
                        update_character_position.in_set(CharacterPositionUpdate),
                        update_view_frustum
                            .pipe(update_chunk_material)
                            .after(CharacterPositionUpdate),
                    )
                        .in_set(CharacterUpdate)
                        .run_if(is_active),
                ),
            );
        // .add_system(sync_material_image)
        // .add_system_set(
        //     SystemSet::new()
        //         .with_run_criteria(is_active)
        //         .with_system(move_character)
        //         .with_system(sync_rotation)
        //         .with_system(update_character_position.label(CharacterPositionUpdate))
        //         .with_system(
        //             update_view_frustum
        //                 .pipe(update_chunk_material)
        //                 .after(CharacterPositionUpdate),
        //         )
        //         .label(CharacterUpdate),
        // );
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

fn sync_material_image(
    material: Res<ChunkMaterialHandle>,
    materials: Res<Assets<ChunkMaterial>>,
    mut image_handle: ResMut<ChunkMaterialImage>,
) {
    if material.is_changed() {
        **image_handle = materials.get(&**material).unwrap().clip_map.clone();
    }
}

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

fn update_view_frustum(
    kinds: Res<ChunkKindRes>,
    lights: Res<ChunkLightRes>,
    position: Res<CharacterPosition>,
    q: Query<&Transform, With<CharacterController>>,
) -> ViewFrustumChain {
    if !position.is_changed() {
        return ViewFrustumChain::DoNothing;
    }

    // Use normalized direction to avoid diagonal voxels
    let forward = projekto_core::math::to_dir(q.single().forward());
    let front_world = (forward + **position).as_vec3();

    let front = match kinds.get_at_world(front_world) {
        Some(k) => k,
        None => return ViewFrustumChain::DoNothing,
    };

    if front.is_opaque() {
        // Facing a wall. Does nothing
        return ViewFrustumChain::RevertMaterial;
    }

    let above_world = position.as_vec3() + Vec3::Y;
    let above = match lights.get_at_world(above_world) {
        Some(l) => l,
        None => return ViewFrustumChain::DoNothing,
    };

    // TODO: Check many blocks using view frustum
    if above.get(voxel::LightTy::Natural) == voxel::Light::MAX_NATURAL_INTENSITY {
        // We aren't inside any building. Skip
        return ViewFrustumChain::RevertMaterial;
    }

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

            let kind = match kinds.get_at_world(next_voxel) {
                Some(k) => k,
                None => continue,
            };

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
    mut debug_entity: Local<Option<Entity>>,
    mut clipped: Local<bool>,
) {
    if debug_entity.is_none() {
        *debug_entity = Some(commands.spawn(Visibility::Hidden).id());
    }

    match voxels {
        ViewFrustumChain::DoNothing => (),
        ViewFrustumChain::RevertMaterial => {
            if !*clipped {
                return;
            }

            trace!("Revert!");

            if let Some(material) = materials.get_mut(&**chunk_material_handle) {
                if let Some(image) = images.get_mut(&material.clip_map) {
                    material.clip_map_origin = Vec2::ZERO;
                    material.clip_height = f32::MAX;
                    material.show_back_faces = false;

                    image.data.fill(0);
                }
            }

            commands
                .entity(debug_entity.unwrap())
                .insert(DrawVoxels::default());
        }
        ViewFrustumChain::ClipMaterial(char_pos, voxels_world) => {
            trace!("Clip!");

            *clipped = true;

            commands.entity(debug_entity.unwrap()).insert(DrawVoxels {
                color: "pink".into(),
                voxels: voxels_world.iter().map(Vec3::as_ivec3).collect(),
                offset: voxels_world[0],
                visible: false,
            });

            if let Some(material) = materials.get_mut(&**chunk_material_handle) {
                if let Some(image) = images.get_mut(&material.clip_map) {
                    let char_chunk = chunk::to_local(char_pos.as_vec3());
                    let left_bottom_chunk =
                        char_chunk - IVec3::splat(landscape::HORIZONTAL_RADIUS as i32);

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
