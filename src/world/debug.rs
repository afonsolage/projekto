use std::collections::{HashMap, VecDeque};

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
    world::{mesh, pipeline::*, storage::*},
};

pub struct WireframeDebugPlugin;

impl Plugin for WireframeDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<DebugCmd>()
            .insert_resource(DebugWireframeStateRes::default())
            .add_startup_system(setup_wireframe_shader_system)
            .add_asset::<WireframeMaterial>()
            .add_system(toggle_mesh_wireframe_system)
            .add_system(toggle_chunk_voxels_wireframe_system)
            .add_system(toggle_landscape_pause_system)
            .add_system(draw_voxels_system)
            .add_system(do_raycast_system)
            .add_system(draw_raycast_system)
            .add_system(check_raycast_intersections_system)
            // .add_system(process_debug_cmd_system)
            .add_system(remove_voxel_system);
    }
}

pub struct DebugCmd(pub String);

// fn process_debug_cmd_system(
//     mut reader: EventReader<DebugCmd>,
//     // mut loaded_writer: EventWriter<CmdChunkLoad>,
//     // mut unloaded_writer: EventWriter<CmdChunkUnload>,
// ) {
//     for DebugCmd(cmd) in reader.iter() {
//         let args = cmd.split(" ").collect::<Vec<_>>();
//         match args[0] {
//             "load" => {
//                 let x = i32::from_str_radix(args[1], 2).expect("Invalid argument!");
//                 let y = i32::from_str_radix(args[2], 2).expect("Invalid argument!");
//                 let z = i32::from_str_radix(args[3], 2).expect("Invalid argument!");

//                 // loaded_writer.send(CmdChunkLoad((x, y, z).into()));
//             }
//             "unload" => {
//                 let x = i32::from_str_radix(args[1], 2).expect("Invalid argument!");
//                 let y = i32::from_str_radix(args[2], 2).expect("Invalid argument!");
//                 let z = i32::from_str_radix(args[3], 2).expect("Invalid argument!");

//                 // unloaded_writer.send(CmdChunkUnload((x, y, z).into()));
//             }
//             _ => {
//                 warn!("Unknown command: {}", cmd);
//             }
//         }
//     }
// }

// Resources
#[derive(Default)]
struct DebugWireframeStateRes {
    show_voxel: bool,
    wireframe: bool,
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
struct WireframeDraw {
    original_mesh: Handle<Mesh>,
    original_pipeline: RenderPipelines,
}

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

fn toggle_landscape_pause_system(
    keyboard: Res<Input<KeyCode>>,
    mut config: ResMut<LandscapeConfig>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }

    config.paused = !config.paused;
}

fn toggle_chunk_voxels_wireframe_system(
    mut commands: Commands,
    keyboard: Res<Input<KeyCode>>,
    mut wireframe_state: ResMut<DebugWireframeStateRes>,
    mut chunk_query: ChunkSystemQuery,
    q_chunks: Query<(Entity, &ChunkLocal)>,
    q_draws: Query<(Entity, &Parent), With<DrawVoxels>>,
) {
    if chunk_query.is_waiting() {
        if let Some(chunks) = chunk_query.fetch() {
            for (e, local) in q_chunks.iter() {
                let chunk = match chunks.get(&local.0) {
                    Some(c) => c,
                    None => continue,
                };

                let voxels = chunk::voxels()
                    .filter(|&v| !chunk.get(v).is_empty())
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
    } else {
        if !keyboard.just_pressed(KeyCode::F2) {
            return;
        }

        wireframe_state.show_voxel = !wireframe_state.show_voxel;

        if !wireframe_state.show_voxel {
            for (e, parent) in q_draws.iter() {
                // Remove only entities with DrawVoxels and with a Chunk as a parent
                if q_chunks.iter().any(|(c_e, _)| c_e.eq(&parent.0)) {
                    commands.entity(e).despawn();
                }
            }
        } else {
            let chunks = q_chunks.iter().map(|(_, c)| c.0).collect();
            chunk_query.query(chunks);
        }
    }
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
    mut debug_state: ResMut<DebugWireframeStateRes>,
    materials: Res<WireframeMaterialsRes>,
    pipeline_handle: Res<WireframePipelineRes>,
    keyboard: Res<Input<KeyCode>>,
    q: Query<(
        Entity,
        &Handle<Mesh>,
        &RenderPipelines,
        Option<&WireframeDraw>,
    )>,
) {
    if !keyboard.just_pressed(KeyCode::F1) {
        return;
    }

    debug_state.wireframe = !debug_state.wireframe;
    info!("Mesh wireframe was set to {}", debug_state.wireframe);

    for (e, mesh, pipelines, wireframe_draw_opt) in q.iter() {
        if !debug_state.wireframe {
            if let Some(wireframe_draw) = wireframe_draw_opt {
                commands
                    .entity(e)
                    .insert(wireframe_draw.original_mesh.clone())
                    .insert(wireframe_draw.original_pipeline.clone())
                    .remove::<WireframeDraw>()
                    .remove::<Handle<WireframeMaterial>>();
            }
        } else {
            if let None = wireframe_draw_opt {
                let mut wireframe_mesh = Mesh::new(PrimitiveTopology::LineList);

                if let Some(mesh_asset) = meshes.get_mut(mesh) {
                    let vertices = mesh_asset.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();

                    wireframe_mesh.set_indices(Some(Indices::U32(compute_wireframe_indices(
                        vertices.len(),
                    ))));
                    wireframe_mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, vertices.clone());

                    let wireframe_mesh_handle = meshes.add(wireframe_mesh);
                    let wireframe_pipelines =
                        RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                            pipeline_handle.0.clone(),
                        )]);

                    let wireframe_draw = WireframeDraw {
                        original_mesh: mesh.clone(),
                        original_pipeline: pipelines.clone(),
                    };

                    commands
                        .entity(e)
                        .insert(wireframe_mesh_handle) //The new wireframe mesh
                        .insert(wireframe_pipelines) //The new wireframe shader/pipeline
                        .insert(Visible::default()) //Why?
                        .insert(wireframe_draw)
                        .insert(materials.get("white")); //The old mesh and pipeline, so I can switch back to it
                }
            }
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
        if draw_voxels.voxels.is_empty() {
            debug!("Skipping draw voxels due to empty voxel list");
            continue;
        }

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
                    first_voxel.as_vec3() * -1.0 + draw_voxels.offset,
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

    if let Ok((transform, camera)) = q_cam.get_single() {
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

#[derive(Default)]
struct CheckRaycastIntersectionsSystemMeta {
    req_entity: Option<Entity>,
    pending: VecDeque<Entity>,
}

impl CheckRaycastIntersectionsSystemMeta {
    fn reset(&mut self) {
        self.req_entity = None;
    }
}

fn check_raycast_intersections_system(
    mut meta: Local<CheckRaycastIntersectionsSystemMeta>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunk_raycast: ChunkSystemRaycast,
    added_q: Query<Entity, (Added<RaycastDebug>, Without<RaycastDebugNoPoint>)>,
    raycast_q: Query<&RaycastDebug, Without<RaycastDebugNoPoint>>,
) {
    for e in added_q.iter() {
        meta.pending.push_back(e);
    }

    if chunk_raycast.is_waiting() {
        if let Some(result) = chunk_raycast.fetch() {
            let hits = result.hits();

            for (chunk_hit, voxels_hit) in hits.iter() {
                let chunk = match result.chunks.get(&chunk_hit.local) {
                    Some(c) => c,
                    None => continue,
                };

                if voxels_hit.is_empty() {
                    warn!(
                        "Raycast returned empty voxels list at {:?}",
                        &chunk_hit.local
                    );
                    continue;
                }

                let mut voxels = vec![];
                for voxel_hit in voxels_hit.iter() {
                    if chunk.get(voxel_hit.local).is_empty() {
                        continue;
                    }

                    add_debug_ball(&mut commands, &mut meshes, voxel_hit.position);

                    commands
                        .spawn()
                        .insert(RaycastDebug {
                            origin: voxel_hit.position,
                            dir: voxel_hit.normal.as_vec3(),
                            range: 0.08,
                        })
                        .insert(RaycastDebugNoPoint);

                    voxels.push(voxel_hit.local);
                }

                let offset = (result.origin
                    - (chunk::to_world(chunk_hit.local) + voxels_hit[0].local.as_vec3()))
                    * -1.0;

                commands
                    .entity(meta.req_entity.unwrap())
                    .with_children(|c| {
                        c.spawn().insert(DrawVoxels {
                            color: "pink".into(),
                            offset,
                            voxels,
                        });
                    });
            }

            meta.reset();
        }
    } else {
        if let Some(next) = meta.pending.pop_front() {
            if let Ok(debug_raycast) = raycast_q.get(next) {
                meta.req_entity = Some(next);
                chunk_raycast.raycast(debug_raycast.origin, debug_raycast.dir, debug_raycast.range);
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
    q_cam: Query<(&Transform, &FlyByCamera)>,
    mouse_input: Res<Input<MouseButton>>,
    mut set_voxel_writer: EventWriter<CmdChunkUpdate>,
    mut query: ChunkSystemRaycast,
) {
    if query.is_waiting() {
        if let Some(result) = query.fetch() {
            let hit_results = result.hits();

            for (chunk_hit, voxels_hit) in hit_results {
                let chunk = match result.chunks.get(&chunk_hit.local) {
                    Some(c) => c,
                    None => continue,
                };

                for voxel_hit in voxels_hit {
                    if chunk.get(voxel_hit.local).is_empty() {
                        continue;
                    }

                    debug!("Hit voxel at {:?} {:?}", chunk_hit.local, voxel_hit.local);
                    set_voxel_writer.send(CmdChunkUpdate(
                        chunk_hit.local,
                        vec![(voxel_hit.local, 0.into())],
                    ));

                    return;
                }
            }
        }
    } else {
        if !mouse_input.just_pressed(MouseButton::Left) {
            return;
        }

        if let Ok((transform, camera)) = q_cam.get_single() {
            if !camera.active {
                return;
            }

            let origin = transform.translation;
            let dir = transform.rotation.mul_vec3(Vec3::Z).normalize() * -1.0;
            let range = 100.0;

            query.raycast(origin, dir, range);
        }
    }
}
