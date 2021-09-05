use std::collections::HashMap;

use bevy::{
    prelude::*,
    reflect::TypeUuid,
    render::{
        mesh::Indices,
        pipeline::{
            FrontFace, PipelineDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
            RenderPipeline,
        },
        render_graph::{base::node::MAIN_PASS, AssetRenderResourcesNode, RenderGraph},
        renderer::RenderResources,
        shader::ShaderStages,
    },
};

use crate::{
    fly_by_camera::FlyByCamera,
    world::storage::{chunk, voxel},
};

use super::ecs::*;
use super::*;

pub struct WireframeDebugPlugin;

impl Plugin for WireframeDebugPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DebugWireframeStateRes::default())
            .add_startup_system(setup_wireframe_shader_system)
            .add_asset::<WireframeMaterial>()
            // .add_system(draw_debug_line)
            .add_system(toggle_mesh_wireframe_system)
            .add_system(draw_voxels_system)
            .add_system(draw_raycast_system)
            .add_system(toggle_chunk_voxels_wireframe_system)
            .add_system(do_raycast_system)
            .add_system(remove_voxel_system)
            .add_system(check_raycast_intersections_system);
    }
}

// Resources
#[derive(Default)]
struct DebugWireframeStateRes {
    show_voxel: bool,
}
struct WireframePipelineRes(Handle<PipelineDescriptor>);

#[derive(RenderResources, Default, TypeUuid)]
#[uuid = "1e08866c-0b8a-437e-8bce-37733b25127e"]
struct WireframeMaterial {
    pub color: Color,
}

#[derive(Default)]
struct WireframeMaterialsRes(HashMap<String, Handle<WireframeMaterial>>);

// Components
struct MeshNPipelineBackup(Handle<Mesh>, RenderPipelines);

#[derive(Debug)]
struct RaycastDebug {
    origin: Vec3,
    dir: Vec3,
    range: f32,
}

struct RaycastDebugNoPoint;

#[derive(Default)]
pub struct DrawVoxels {
    color: String,
    voxels: Vec<IVec3>,
    offset: Vec3,
}

// Systems

fn toggle_chunk_voxels_wireframe_system(
    mut commands: Commands,
    keyboard: Res<Input<KeyCode>>,
    mut wireframe_state: ResMut<DebugWireframeStateRes>,
    q_chunks: Query<(Entity, &ChunkVoxels)>,
    q_draws: Query<(Entity, &Parent), With<DrawVoxels>>,
) {
    if !keyboard.just_pressed(KeyCode::F2) {
        return;
    }

    if wireframe_state.show_voxel {
        for (e, parent) in q_draws.iter() {
            // Remove only entities with DrawVoxels and with a Chunk as a parent
            if q_chunks.iter().any(|(c_e, _)| c_e.eq(&parent.0)) {
                commands.entity(e).despawn();
            }
        }
    } else {
        for (e, types) in q_chunks.iter() {
            let voxels = types
                .0
                .iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    //TODO: Use enum or consts later on
                    if *v == 0u8 {
                        None
                    } else {
                        Some(chunk::to_xyz_ivec3(i))
                    }
                })
                .collect();

            commands.entity(e).with_children(|c| {
                c.spawn().insert(DrawVoxels {
                    color: "gray".into(),
                    voxels,
                    ..Default::default()
                });
            });
        }
    }

    wireframe_state.show_voxel = !wireframe_state.show_voxel;
}

impl WireframeMaterialsRes {
    fn get(&self, color: &str) -> Handle<WireframeMaterial> {
        self.0.get(color).unwrap().clone()
    }

    fn add(&mut self, color: &str, handle: Handle<WireframeMaterial>) {
        self.0.insert(color.into(), handle);
    }
}

fn setup_wireframe_shader_system(
    mut commands: Commands,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    mut render_graph: ResMut<RenderGraph>,
    mut materials: ResMut<Assets<WireframeMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let pipeline_handle = pipelines.add(PipelineDescriptor {
        name: Some("Wireframe Chunk".into()),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: PolygonMode::Fill,
            clamp_depth: false,
            conservative: false,
        },
        ..PipelineDescriptor::default_config(ShaderStages {
            vertex: asset_server.load("shaders/wireframe.vert"),
            fragment: Some(asset_server.load("shaders/wireframe.frag")),
        })
    });

    render_graph.add_system_node(
        "wireframe_material",
        AssetRenderResourcesNode::<WireframeMaterial>::new(true),
    );

    if let Err(error) = render_graph.add_node_edge("wireframe_material", MAIN_PASS) {
        error!("Failed to setup render graph: {}", error);
    };

    commands.insert_resource(WireframePipelineRes(pipeline_handle));

    let mut wireframe_materials = WireframeMaterialsRes::default();
    wireframe_materials.add(
        "red",
        materials.add(WireframeMaterial { color: Color::RED }),
    );
    wireframe_materials.add(
        "white",
        materials.add(WireframeMaterial {
            color: Color::WHITE,
        }),
    );
    wireframe_materials.add(
        "green",
        materials.add(WireframeMaterial {
            color: Color::GREEN,
        }),
    );
    wireframe_materials.add(
        "pink",
        materials.add(WireframeMaterial { color: Color::PINK }),
    );
    wireframe_materials.add(
        "blue",
        materials.add(WireframeMaterial { color: Color::BLUE }),
    );
    wireframe_materials.add(
        "gray",
        materials.add(WireframeMaterial { color: Color::GRAY }),
    );

    commands.insert_resource(wireframe_materials);
}

fn toggle_mesh_wireframe_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterialsRes>,
    pipeline_handle: Res<WireframePipelineRes>,
    keyboard: Res<Input<KeyCode>>,
    q: Query<(
        Entity,
        &Handle<Mesh>,
        &RenderPipelines,
        &ChunkVertices,
        Option<&MeshNPipelineBackup>,
    )>,
) {
    if !keyboard.just_pressed(KeyCode::F1) {
        return;
    }

    for (e, mesh, pipelines, vertices, backup) in q.iter() {
        if let Some(mesh_n_pipeline_backup) = backup {
            meshes.remove(mesh);

            commands
                .entity(e)
                .insert(mesh_n_pipeline_backup.0.clone())
                .insert(mesh_n_pipeline_backup.1.clone())
                .remove::<MeshNPipelineBackup>()
                .remove::<Handle<WireframeMaterial>>();
        } else {
            let mesh_n_pipeline_backup = MeshNPipelineBackup(mesh.clone(), pipelines.clone());

            let mut wireframe_mesh = Mesh::new(PrimitiveTopology::LineList);

            let mut positions: Vec<[f32; 3]> = vec![];

            for side in voxel::SIDES {
                let side_idx = side as usize;
                let side_vertices = &vertices.0[side_idx];

                positions.extend(side_vertices);
            }

            let vertex_count = positions.len();

            wireframe_mesh.set_indices(Some(Indices::U32(compute_wireframe_indices(vertex_count))));
            wireframe_mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);

            let wireframe_mesh_handle = meshes.add(wireframe_mesh);

            commands
                .entity(e)
                .insert(wireframe_mesh_handle) //The new wireframe mesh
                .insert(RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                    pipeline_handle.0.clone(),
                )])) //The new wireframe shader/pipeline
                .insert(Visible::default()) //Why?
                .insert(mesh_n_pipeline_backup)
                .insert(materials.get("white")); //The old mesh and pipeline, so I can switch back to it
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

fn draw_voxels_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterialsRes>,
    wireframe_pipeline_handle: Res<WireframePipelineRes>,
    q: Query<(Entity, &DrawVoxels), Added<DrawVoxels>>,
) {
    for (e, draw_voxels) in q.iter() {
        let (vertices, indices) = generate_voxel_edges_mesh(&draw_voxels.voxels);
        let first_voxel = draw_voxels.voxels[0];

        let mut mesh = Mesh::new(PrimitiveTopology::LineList);
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.set_indices(Some(Indices::U32(indices)));

        let mesh_handle = meshes.add(mesh);

        commands
            .entity(e)
            .insert_bundle(MeshBundle {
                mesh: mesh_handle,
                render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                    wireframe_pipeline_handle.0.clone(),
                )]),
                transform: Transform::from_translation(
                    first_voxel.as_f32() * -1.0 + draw_voxels.offset,
                ),
                ..Default::default()
            })
            .insert(materials.get(&draw_voxels.color));
    }
}

fn generate_voxel_edges_mesh(voxels: &[IVec3]) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut vertices = vec![];

    for voxel in voxels.iter() {
        for side in voxel::SIDES {
            let side_idx = side as usize;

            for idx in mesh::VERTICES_INDICES[side_idx] {
                let v = &mesh::VERTICES[idx];

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
fn do_raycast_system(
    mut commands: Commands,
    keyboard: Res<Input<KeyCode>>,
    q_cam: Query<(&Transform, &FlyByCamera)>,
) {
    if !keyboard.just_pressed(KeyCode::F3) {
        return;
    }

    if let Ok((transform, camera)) = q_cam.single() {
        if !camera.active {
            return;
        }

        let raycast = RaycastDebug {
            origin: transform.translation,
            dir: transform.rotation.mul_vec3(Vec3::Z).normalize() * -1.0,
            range: 20.0, //TODO: Change this later
        };

        commands.spawn().insert(raycast);
    }
}

fn check_raycast_intersections_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_entities: Res<ChunkEntitiesRes>,
    q_raycast: Query<(Entity, &RaycastDebug), (Added<RaycastDebug>, Without<RaycastDebugNoPoint>)>,
    q_chunks: Query<&ChunkVoxels>,
) {
    for (e, raycast) in q_raycast.iter() {
        let res = raycast::intersect(raycast.origin, raycast.dir, raycast.range);

        for (chunk_hit, voxels_hit) in res.iter() {
            let chunk_entity = match chunk_entities.0.get(&chunk_hit.local) {
                Some(e) => e,
                None => continue,
            };

            if q_chunks.get(*chunk_entity).is_err() {
                warn!("Chunk {:?} wasn't found on query.", &chunk_hit.local);
                continue;
            }

            if voxels_hit.is_empty() {
                warn!(
                    "Raycast returned empty voxels list at {:?} ({:?})",
                    raycast, &chunk_hit.local
                );
                continue;
            }

            let mut voxels = vec![];
            for voxel_hit in voxels_hit.iter() {
                add_debug_ball(&mut commands, &mut meshes, voxel_hit.position);

                commands
                    .spawn()
                    .insert(RaycastDebug {
                        origin: voxel_hit.position,
                        dir: voxel_hit.normal.as_f32(),
                        range: 0.08,
                    })
                    .insert(RaycastDebugNoPoint);

                voxels.push(voxel_hit.local);
            }

            let offset = (raycast.origin
                - (chunk::to_world(chunk_hit.local) + voxels_hit[0].local.as_f32()))
                * -1.0;

            commands.entity(e).with_children(|c| {
                c.spawn().insert(DrawVoxels {
                    color: "pink".into(),
                    offset,
                    voxels,
                });
            });
        }
    }
}

fn add_debug_ball(commands: &mut Commands, meshes: &mut ResMut<Assets<Mesh>>, position: Vec3) {
    let mesh = Mesh::from(shape::UVSphere {
        radius: 0.01,
        sectors: 10,
        stacks: 10,
    });

    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(mesh),
        transform: Transform::from_translation(position),
        ..Default::default()
    });
}

fn draw_raycast_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterialsRes>,
    wireframe_pipeline_handle: Res<WireframePipelineRes>,
    q: Query<(Entity, &RaycastDebug), Without<Handle<Mesh>>>,
) {
    for (e, raycast) in q.iter() {
        let end = raycast.dir * raycast.range;

        let vertices = vec![Vec3::ZERO.to_array(), end.to_array()];
        let indices = vec![0, 1];

        let mut mesh = Mesh::new(PrimitiveTopology::LineList);
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.set_indices(Some(Indices::U32(indices)));

        let mesh_handle = meshes.add(mesh);

        commands
            .entity(e)
            .insert_bundle(MeshBundle {
                mesh: mesh_handle,
                transform: Transform::from_translation(raycast.origin),
                render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                    wireframe_pipeline_handle.0.clone(),
                )]),
                ..Default::default()
            })
            .insert(materials.get("pink"));
    }
}

fn remove_voxel_system(
    chunks: Res<ChunkEntitiesRes>,
    q_chunks: Query<&ChunkVoxels>,
    q_cam: Query<(&Transform, &FlyByCamera)>,
    mouse_input: Res<Input<MouseButton>>,
    mut set_voxel_writer: EventWriter<ChunkSetVoxelCmd>,
) {
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }

    if let Ok((transform, camera)) = q_cam.single() {
        if !camera.active {
            return;
        }

        let origin = transform.translation;
        let dir = transform.rotation.mul_vec3(Vec3::Z).normalize() * -1.0;
        let range = 100.0;

        let hit_results = raycast::intersect(origin, dir, range);

        for (chunk_hit, voxels_hit) in hit_results {
            let local = chunk_hit.local;

            let entity = match chunks.0.get(&local) {
                Some(e) => *e,
                None => continue,
            };

            let types = match q_chunks.get(entity) {
                Ok(t) => t,
                Err(_) => continue,
            };

            for voxel_hit in voxels_hit {
                let index = chunk::to_index_ivec3(voxel_hit.local);

                if types.0[index] == 0 {
                    continue;
                }

                debug!("Hit voxel at {:?} {:?}", local, voxel_hit.local);
                set_voxel_writer.send(ChunkSetVoxelCmd {
                    world_pos: voxel_hit.position,
                    new_value: 0,
                });

                return;
            }
        }
    }
}
