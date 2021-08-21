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
            .add_system(draw_chunk_voxels)
            .add_system(delete_chunk_voxels)
            .add_system(draw_raycast)
            .add_system(toggle_voxel_wireframe)
            .add_system(do_raycast)
            .add_system(check_raycast_intersections);
    }
}

#[derive(Default)]
struct DebugWireframeState {
    voxel_wireframe: bool,
}

fn toggle_voxel_wireframe(
    mut commands: Commands,
    keyboard: Res<Input<KeyCode>>,
    mut wireframe_state: ResMut<DebugWireframeState>,
    q_add: Query<Entity, (With<ChunkTypes>, Without<DrawVoxels>)>,
    q_remove: Query<Entity, With<DrawVoxels>>,
) {
    if !keyboard.just_pressed(KeyCode::F2) {
        return;
    }

    if wireframe_state.voxel_wireframe {
        for e in q_remove.iter() {
            commands.entity(e).remove::<DrawVoxels>();
        }
    } else {
        for e in q_add.iter() {
            commands.entity(e).insert(DrawVoxels);
        }
    }

    wireframe_state.voxel_wireframe = !wireframe_state.voxel_wireframe;
}

struct WireframePipeline(Handle<PipelineDescriptor>);

#[derive(RenderResources, Default, TypeUuid)]
#[uuid = "1e08866c-0b8a-437e-8bce-37733b25127e"]
struct WireframeMaterial {
    pub color: Color,
}

struct WireframeMaterials {
    pink: Handle<WireframeMaterial>,
    white: Handle<WireframeMaterial>,
    red: Handle<WireframeMaterial>,
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
    commands.insert_resource(WireframeMaterials {
        pink: materials.add(WireframeMaterial { color: Color::PINK }),
        white: materials.add(WireframeMaterial {
            color: Color::WHITE,
        }),
        red: materials.add(WireframeMaterial { color: Color::RED }),
    });
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
                .insert(materials.white.clone()); //The old mesh and pipeline, so I can switch back to it
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

pub struct DrawVoxels;
struct DrawVoxelsFilter(Vec<IVec3>);

struct DrawVoxelDone(Entity);

fn draw_chunk_voxels(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<WireframeMaterials>,
    wireframe_pipeline_handle: Res<WireframePipeline>,
    q: Query<(Entity, &ChunkTypes, Option<&DrawVoxelsFilter>), Added<DrawVoxels>>,
) {
    for (e, types, filter) in q.iter() {
        let (vertices, indices) = generate_voxel_edges_mesh(&types.0, filter.map(|f| &f.0));

        let mut mesh = Mesh::new(PrimitiveTopology::LineList);
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.set_indices(Some(Indices::U32(indices)));

        let mesh_handle = meshes.add(mesh);

        let child = commands
            .spawn_bundle(MeshBundle {
                mesh: mesh_handle,
                render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                    wireframe_pipeline_handle.0.clone(),
                )]),
                ..Default::default()
            })
            .insert(materials.red.clone())
            .id();

        commands
            .entity(e)
            .insert(DrawVoxelDone(child))
            .push_children(&[child]);
    }
}

fn generate_voxel_edges_mesh(
    types: &[u8; chunk::BUFFER_SIZE],
    filter: Option<&Vec<IVec3>>,
) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut vertices = vec![];

    for (idx, type_idx) in types.iter().enumerate() {
        // TODO: Change this to Enum or better type representation
        if *type_idx == 0u8 {
            continue;
        }

        let pos = chunk::to_xyz_ivec3(idx);

        if filter.is_some() && !filter.unwrap().contains(&pos) {
            continue;
        }

        for side in voxel::SIDES {
            let side_idx = side as usize;

            for idx in mesh::VERTICES_INDICES[side_idx] {
                let v = &mesh::VERTICES[idx];

                vertices.push([
                    v[0] + pos.x as f32,
                    v[1] + pos.y as f32,
                    v[2] + pos.z as f32,
                ]);
            }
        }
    }

    let indices = compute_wireframe_indices(vertices.len());

    (vertices, indices)
}

fn delete_chunk_voxels(
    mut commands: Commands,
    q: Query<(Entity, &DrawVoxelDone), Without<DrawVoxels>>,
) {
    for (e, draw_voxel_done) in q.iter() {
        commands.entity(draw_voxel_done.0).despawn();
        commands.entity(e).remove::<DrawVoxelDone>();
    }
}

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
    q_raycast: Query<&RaycastDebug, (Added<RaycastDebug>, Without<RaycastDebugNoPoint>)>,
    q_chunks: Query<(Entity, &Chunk, &ChunkTypes)>,
) {
    for raycast in q_raycast.iter() {
        // let grid_dir = math::to_grid_dir(raycast.dir);
        let current_chunk = math::to_ivec3(raycast.origin) / chunk::AXIS_SIZE as i32;

        for (e, c, _) in q_chunks.iter() {
            if c.0 != current_chunk {
                continue;
            }

            let (voxels, positions, normals) = chunk::raycast(raycast.origin, raycast.dir);

            for p in positions.iter() {
                add_debug_ball(&mut commands, &mut meshes, *p);
            }

            for (i, n) in normals.iter().enumerate() {
                commands
                    .spawn()
                    .insert(RaycastDebug {
                        origin: positions[i],
                        dir: n.as_f32(),
                        range: 0.08,
                    })
                    .insert(RaycastDebugNoPoint);
            }

            commands.entity(e).insert(DrawVoxelsFilter(voxels));
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

// #[derive(Debug)]
// struct DebugLine(Vec3, Vec3);
// fn draw_debug_line(
//     mut commands: Commands,
//     mut meshes: ResMut<Assets<Mesh>>,
//     materials: Res<WireframeMaterials>,
//     wireframe_pipeline_handle: Res<WireframePipeline>,
//     q: Query<(Entity, &DebugLine), Added<DebugLine>>,
// ) {
//     for (e, debug_line) in q.iter() {
//         dbg!(debug_line);

//         let vertices = vec![Vec3::ZERO.to_array(), debug_line.1.to_array()];
//         let indices = vec![0, 1];

//         let mut mesh = Mesh::new(PrimitiveTopology::LineList);
//         mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
//         mesh.set_indices(Some(Indices::U32(indices)));

//         commands
//             .entity(e)
//             .insert_bundle(MeshBundle {
//                 mesh: meshes.add(mesh),
//                 transform: Transform::from_translation(debug_line.0),
//                 render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
//                     wireframe_pipeline_handle.0.clone(),
//                 )]),
//                 ..Default::default()
//             })
//             .insert(materials.pink.clone());
//     }
// }

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
            .insert(materials.pink.clone());
    }
}
