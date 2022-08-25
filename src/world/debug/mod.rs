use std::collections::HashMap;

use bevy::{
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};
use itertools::Itertools;
use projekto_camera::fly_by::{self, FlyByCamera};

use crate::world::rendering::*;
use projekto_core::*;
use projekto_shaping as shaping;

use self::wireframe::WireframeMaterial;

use crate::world::terraformation::prelude::*;

mod wireframe;

pub struct WireframeDebugPlugin;

impl Plugin for WireframeDebugPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DebugWireframeStateRes::default())
            .register_type::<DrawVoxels>()
            .register_type::<RaycastDebug>()
            .add_startup_system(setup_wireframe_shader)
            .add_asset::<WireframeMaterial>()
            .add_plugin(MaterialPlugin::<WireframeMaterial>::default())
            .add_system_set(
                SystemSet::new()
                    .with_run_criteria(fly_by::is_active)
                    .with_system(do_raycast)
                    .with_system(remove_voxel)
                    .with_system(add_voxel),
            )
            .add_system(toggle_mesh_wireframe)
            .add_system(toggle_chunk_voxels_wireframe)
            .add_system(toggle_landscape_pause)
            .add_system(draw_voxels)
            .add_system(draw_raycast)
            .add_system(check_raycast_intersections);
    }
}

// Resources
#[derive(Default)]
struct DebugWireframeStateRes {
    show_voxel: bool,
    wireframe: bool,
}

#[derive(Default)]
struct WireframeMaterialsMap(HashMap<String, Handle<WireframeMaterial>>);

#[derive(Component)]
struct WireframeDraw {
    original_mesh: Handle<Mesh>,
    original_material: Handle<ChunkMaterial>,
}

#[derive(Component, Debug, Reflect)]
pub struct RaycastDebug {
    pub origin: Vec3,
    pub dir: Vec3,
    pub range: f32,
}

#[derive(Component)]
struct RaycastDebugNoPoint;

#[derive(Component, Default, Reflect)]
pub struct DrawVoxels {
    pub color: String,
    pub voxels: Vec<IVec3>,
    pub offset: Vec3,
    pub visible: bool,
}

// Systems

fn toggle_landscape_pause(keyboard: Res<Input<KeyCode>>, mut config: ResMut<LandscapeConfig>) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }

    config.paused = !config.paused;
}

#[derive(Component)]
struct WireframeVoxels;

fn toggle_chunk_voxels_wireframe(
    mut commands: Commands,
    keyboard: Res<Input<KeyCode>>,
    mut wireframe_state: ResMut<DebugWireframeStateRes>,
    kinds: Res<ChunkKindRes>,
    chunk_map: Res<ChunkEntityMap>,
    q_draws: Query<Entity, (With<DrawVoxels>, With<WireframeVoxels>)>,
) {
    if !keyboard.just_pressed(KeyCode::F2) {
        return;
    }

    wireframe_state.show_voxel = !wireframe_state.show_voxel;

    if !wireframe_state.show_voxel {
        for e in q_draws.iter() {
            // Remove only entities with DrawVoxels and with a Chunk as a parent
            commands.entity(e).despawn();
        }
    } else {
        for (local, kinds) in kinds.iter() {
            if let Some(&entity) = chunk_map.get(&local) {
                let voxels = chunk::voxels()
                    .filter(|&v| kinds.get(v).is_none() == false)
                    .collect_vec();

                commands.entity(entity).with_children(|c| {
                    c.spawn()
                        .insert(DrawVoxels {
                            color: "gray".into(),
                            voxels,
                            ..Default::default()
                        })
                        .insert(WireframeVoxels);
                });
            }
        }
    }
}

fn setup_wireframe_shader(
    mut commands: Commands,
    mut materials: ResMut<Assets<WireframeMaterial>>,
) {
    let mut wireframe_materials = WireframeMaterialsMap::default();

    wireframe_materials.0.insert(
        "red".into(),
        materials.add(WireframeMaterial { color: Color::RED }),
    );
    wireframe_materials.0.insert(
        "white".into(),
        materials.add(WireframeMaterial {
            color: Color::WHITE,
        }),
    );
    wireframe_materials.0.insert(
        "green".into(),
        materials.add(WireframeMaterial {
            color: Color::GREEN,
        }),
    );
    wireframe_materials.0.insert(
        "pink".into(),
        materials.add(WireframeMaterial { color: Color::PINK }),
    );
    wireframe_materials.0.insert(
        "blue".into(),
        materials.add(WireframeMaterial { color: Color::BLUE }),
    );
    wireframe_materials.0.insert(
        "gray".into(),
        materials.add(WireframeMaterial { color: Color::GRAY }),
    );

    commands.insert_resource(wireframe_materials);
}

fn toggle_mesh_wireframe(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut debug_state: ResMut<DebugWireframeStateRes>,
    materials: Res<WireframeMaterialsMap>,
    keyboard: Res<Input<KeyCode>>,
    to_wireframe: Query<(Entity, &Handle<Mesh>, &Handle<ChunkMaterial>), Without<WireframeDraw>>,
    to_original: Query<(Entity, &WireframeDraw)>,
) {
    if !keyboard.just_pressed(KeyCode::F1) {
        return;
    }

    debug_state.wireframe = !debug_state.wireframe;
    info!("Mesh wireframe was set to {}", debug_state.wireframe);

    if debug_state.wireframe {
        for (e, mesh, material) in to_wireframe.iter() {
            let mut wireframe_mesh = Mesh::new(PrimitiveTopology::LineList);

            if let Some(mesh_asset) = meshes.get_mut(mesh) {
                let vertices = mesh_asset.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();

                wireframe_mesh.set_indices(Some(Indices::U32(compute_wireframe_indices(
                    vertices.len(),
                ))));
                wireframe_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices.clone());

                //Remove this when https://github.com/bevyengine/bevy/issues/5147 gets fixed
                wireframe_mesh.insert_attribute(
                    Mesh::ATTRIBUTE_NORMAL,
                    vec![[0.0, 0.0, 0.0]; vertices.len()],
                );
                wireframe_mesh
                    .insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; vertices.len()]);

                let wireframe_mesh_handle = meshes.add(wireframe_mesh);
                let wireframe_draw = WireframeDraw {
                    original_mesh: mesh.clone(),
                    original_material: material.clone(),
                };

                commands
                    .entity(e)
                    .insert(wireframe_mesh_handle) //The new wireframe mesh
                    .insert(wireframe_draw)
                    .insert(materials.0.get("white").unwrap().clone())
                    .remove::<Handle<ChunkMaterial>>();
            }
        }
    } else {
        for (e, wireframe) in to_original.iter() {
            commands
                .entity(e)
                .insert(wireframe.original_mesh.clone())
                .insert(wireframe.original_material.clone())
                .remove::<WireframeDraw>()
                .remove::<Handle<WireframeMaterial>>();
        }
    }
}

fn compute_wireframe_indices(vertex_count: usize) -> Vec<u32> {
    let index_count = (vertex_count as f32 * 2.0) as usize;

    let mut res = vec![0; index_count];
    let mut i = 0u32;

    while i < vertex_count as u32 {
        res.push(i);
        res.push(i + 1);

        res.push(i + 1);
        res.push(i + 2);

        res.push(i + 2);
        res.push(i + 3);

        res.push(i + 3);
        res.push(i);

        i += 4;
    }

    res
}

fn draw_voxels(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterialsMap>,
    q: Query<(Entity, &DrawVoxels), Changed<DrawVoxels>>,
) {
    for (e, draw_voxels) in q.iter() {
        if draw_voxels.voxels.is_empty() {
            commands.entity(e).insert(Handle::<Mesh>::default());
            continue;
        }

        let (vertices, indices) = generate_voxel_edges_mesh(&draw_voxels.voxels);
        let first_voxel = draw_voxels.voxels[0];

        let mut mesh = Mesh::new(PrimitiveTopology::LineList);

        //Remove this when https://github.com/bevyengine/bevy/issues/5147 gets fixed
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; vertices.len()]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; vertices.len()]);

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.set_indices(Some(Indices::U32(indices)));

        let mesh_handle = meshes.add(mesh);

        commands.entity(e).insert_bundle(MaterialMeshBundle {
            mesh: mesh_handle,
            material: materials.0.get(&draw_voxels.color).unwrap().clone(),
            transform: Transform::from_translation(
                first_voxel.as_vec3() * -1.0 + draw_voxels.offset,
            ),
            visibility: Visibility {
                is_visible: draw_voxels.visible,
            },
            ..Default::default()
        });
    }
}

fn generate_voxel_edges_mesh(voxels: &[IVec3]) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut vertices = vec![];

    for voxel in voxels.iter() {
        for side in voxel::SIDES {
            let side_idx = side as usize;

            for idx in shaping::VERTICES_INDICES[side_idx] {
                let v = &shaping::VERTICES[idx];

                vertices.push([
                    v[0] + voxel.x as f32,
                    v[1] + voxel.y as f32,
                    v[2] + voxel.z as f32,
                ]);
            }
        }
    }

    let indices = compute_wireframe_indices(vertices.len());

    (vertices, indices)
}

fn do_raycast(
    mut commands: Commands,
    keyboard: Res<Input<KeyCode>>,
    q_cam: Query<&Transform, With<FlyByCamera>>,
) {
    if !keyboard.just_pressed(KeyCode::F3) {
        return;
    }

    if let Ok(transform) = q_cam.get_single() {
        let raycast = RaycastDebug {
            origin: transform.translation,
            dir: transform.rotation.mul_vec3(Vec3::Z).normalize() * -1.0,
            range: 100.0, //TODO: Change this later
        };

        commands.spawn().insert(raycast);
    }
}

fn check_raycast_intersections(
    mut commands: Commands,
    added_q: Query<(Entity, &RaycastDebug), (Added<RaycastDebug>, Without<RaycastDebugNoPoint>)>,
    kinds: Res<ChunkKindRes>,
) {
    for (e, raycast) in &added_q {
        // Get only world position of raycast
        let voxels = query::raycast(raycast.origin, raycast.dir, raycast.range)
            .into_iter()
            .map(|(_, voxel_hits)| voxel_hits)
            .flatten()
            .map(|hit| hit.position)
            .filter(|&w| kinds.get_at_world(w).is_some_and(|k| k.is_none() == false))
            .map(|v| v.as_ivec3())
            .collect();

        commands.entity(e).insert(DrawVoxels {
            color: "pink".into(),
            voxels,
            ..Default::default()
        });
    }
}

// fn add_debug_ball(commands: &mut Commands, meshes: &mut ResMut<Assets<Mesh>>, position: Vec3) {
//     let mesh = Mesh::from(shape::UVSphere {
//         radius: 0.01,
//         sectors: 10,
//         stacks: 10,
//     });

//     commands.spawn_bundle(PbrBundle {
//         mesh: meshes.add(mesh),
//         transform: Transform::from_translation(position),
//         ..Default::default()
//     });
// }

fn draw_raycast(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterialsMap>,
    q: Query<(Entity, &RaycastDebug), Changed<RaycastDebug>>,
) {
    for (e, raycast) in q.iter() {
        let end = raycast.dir * raycast.range;

        let vertices = vec![Vec3::ZERO.to_array(), end.to_array()];
        let indices = vec![0, 1];

        let mut mesh = Mesh::new(PrimitiveTopology::LineList);

        //Remove this when https://github.com/bevyengine/bevy/issues/5147 gets fixed
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; vertices.len()]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; vertices.len()]);

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.set_indices(Some(Indices::U32(indices)));

        let mesh_handle = meshes.add(mesh);

        commands.entity(e).insert_bundle(MaterialMeshBundle {
            mesh: mesh_handle,
            transform: Transform::from_translation(raycast.origin),
            material: materials.0.get("pink").unwrap().clone(),
            ..Default::default()
        });
    }
}

fn remove_voxel(
    q_cam: Query<&Transform, With<FlyByCamera>>,
    mouse_input: Res<Input<MouseButton>>,
    mut cmd_buffer: ResMut<GenesisCommandBuffer>,
    kinds: Res<ChunkKindRes>,
) {
    if !mouse_input.just_pressed(MouseButton::Right) {
        return;
    }

    if let Ok(transform) = q_cam.get_single() {
        let origin = transform.translation;
        let dir = transform.rotation.mul_vec3(Vec3::Z).normalize() * -1.0;
        let range = 100.0;

        let world_hit = query::raycast(origin, dir, range)
            .into_iter()
            .map(|(_, voxel_hits)| voxel_hits)
            .flatten()
            .map(|hit| hit.position)
            .filter(|&w| kinds.get_at_world(w).is_some_and(|k| k.is_none() == false))
            .next();

        if let Some(world) = world_hit {
            let local = chunk::to_local(world);
            let voxel = voxel::to_local(world);

            debug!("Hit voxel at {:?} {:?}", local, voxel);
            cmd_buffer.update(local, vec![(voxel, voxel::Kind::none())]);
        }
    }
}

fn add_voxel(
    q_cam: Query<&Transform, With<FlyByCamera>>,
    mouse_input: Res<Input<MouseButton>>,
    mut cmd_buffer: ResMut<GenesisCommandBuffer>,
    kinds: Res<ChunkKindRes>,
) {
    if !mouse_input.just_pressed(MouseButton::Right) {
        return;
    }

    if let Ok(transform) = q_cam.get_single() {
        let origin = transform.translation;
        let dir = transform.rotation.mul_vec3(Vec3::Z).normalize() * -1.0;
        let range = 100.0;

        let world_hit = query::raycast(origin, dir, range)
            .into_iter()
            .map(|(_, voxel_hits)| voxel_hits)
            .flatten()
            .map(|hit| hit.position)
            .filter(|&w| kinds.get_at_world(w).is_some_and(|k| k.is_none() == false))
            .next();

        if let Some(world) = world_hit {
            let local = chunk::to_local(world);
            let voxel = voxel::to_local(world);

            debug!("Hit voxel at {:?} {:?}", local, voxel);
            cmd_buffer.update(local, vec![(voxel, voxel::Kind::id(4))]);
        }
    }
}
