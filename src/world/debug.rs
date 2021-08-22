use std::collections::HashMap;

use bevy::{
    prelude::*,
    reflect::TypeUuid,
    render::{
        pipeline::PrimitiveTopology,
        render_graph::{base::node::MAIN_PASS, AssetRenderResourcesNode, RenderGraph},
        renderer::RenderResources,
    },
};

use crate::fly_by_camera::FlyByCamera;

use super::*;

pub struct WireframeDebugPlugin;

impl Plugin for WireframeDebugPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DebugWireframeState::default())
            .add_startup_system(setup_wireframe_shader)
            .add_asset::<WireframeMaterial>()
            // .add_system(draw_debug_line)
            .add_system(toggle_mesh_wireframe)
            .add_system(draw_voxels)
            .add_system(draw_raycast)
            .add_system(toggle_chunk_voxels_wireframe)
            .add_system(do_raycast)
            .add_system(check_raycast_intersections);
    }
}

#[derive(Default)]
struct DebugWireframeState {
    show_voxel: bool,
}

fn toggle_chunk_voxels_wireframe(
    mut commands: Commands,
    keyboard: Res<Input<KeyCode>>,
    mut wireframe_state: ResMut<DebugWireframeState>,
    q_chunks: Query<(Entity, &ChunkTypes)>,
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

struct WireframePipeline(Handle<PipelineDescriptor>);

#[derive(RenderResources, Default, TypeUuid)]
#[uuid = "1e08866c-0b8a-437e-8bce-37733b25127e"]
struct WireframeMaterial {
    pub color: Color,
}

#[derive(Default)]
struct WireframeMaterials(HashMap<String, Handle<WireframeMaterial>>);

impl WireframeMaterials {
    fn get(&self, color: &str) -> Handle<WireframeMaterial> {
        self.0.get(color).unwrap().clone()
    }

    fn add(&mut self, color: &str, handle: Handle<WireframeMaterial>) {
        self.0.insert(color.into(), handle);
    }
}

fn setup_wireframe_shader(
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

    commands.insert_resource(WireframePipeline(pipeline_handle));

    let mut wireframe_materials = WireframeMaterials::default();
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

struct MeshNPipelineBackup(Handle<Mesh>, RenderPipelines);

fn toggle_mesh_wireframe(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterials>,
    pipeline_handle: Res<WireframePipeline>,
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

#[derive(Default)]
pub struct DrawVoxels {
    color: String,
    voxels: Vec<IVec3>,
    offset: Vec3,
}

fn draw_voxels(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterials>,
    wireframe_pipeline_handle: Res<WireframePipeline>,
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

// fn delete_chunk_voxels(
//     mut commands: Commands,
//     q: Query<(Entity, &DrawVoxelDone), Without<DrawVoxels>>,
// ) {
//     for (e, draw_voxel_done) in q.iter() {
//         commands.entity(draw_voxel_done.0).despawn();
//         commands.entity(e).remove::<DrawVoxelDone>();
//     }
// }

#[derive(Debug)]
struct RaycastDebug {
    origin: Vec3,
    dir: Vec3,
    range: f32,
}

struct RaycastDebugNoPoint;

fn do_raycast(
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
            range: 100.0, //TODO: Change this later
        };

        commands.spawn().insert(raycast);
    }
}

fn check_raycast_intersections(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_entities: Res<ChunkEntities>,
    q_raycast: Query<(Entity, &RaycastDebug), (Added<RaycastDebug>, Without<RaycastDebugNoPoint>)>,
    q_chunks: Query<(&Chunk, &ChunkTypes)>,
) {
    for (e, raycast) in q_raycast.iter() {
        let (hit_chunks, hit_pos, _) = super::raycast(raycast.origin, raycast.dir, raycast.range);

        for (idx, chunk_pos) in hit_chunks.iter().enumerate() {
            if let Some(chunk_entity) = chunk_entities.0.get(chunk_pos) {
                if q_chunks.get(*chunk_entity).is_err() {
                    warn!("Chunk {:?} wasn't found on query.", chunk_pos);
                    continue;
                }

                let (voxels, hit_points, hit_normals) = chunk::raycast(hit_pos[idx], raycast.dir);

                if voxels.is_empty() {
                    warn!(
                        "Raycast returned empty voxels list at {:?} ({:?})",
                        raycast, chunk_pos
                    );
                    continue;
                }

                for p in hit_points.iter() {
                    add_debug_ball(&mut commands, &mut meshes, *p);
                }

                for (i, n) in hit_normals.iter().enumerate() {
                    commands
                        .spawn()
                        .insert(RaycastDebug {
                            origin: hit_points[i],
                            dir: n.as_f32(),
                            range: 0.08,
                        })
                        .insert(RaycastDebugNoPoint);
                }

                let offset =
                    (raycast.origin - (chunk::to_world(chunk_pos) + voxels[0].as_f32())) * -1.0;

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

fn draw_raycast(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterials>,
    wireframe_pipeline_handle: Res<WireframePipeline>,
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
