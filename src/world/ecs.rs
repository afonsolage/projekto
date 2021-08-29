use std::collections::HashMap;

use bevy::{
    prelude::*,
    render::{
        mesh::Indices,
        pipeline::{PipelineDescriptor, PrimitiveTopology, RenderPipeline},
        shader::ShaderStages,
    },
};

use crate::world::{mesh, voxel};

use super::{chunk, debug::WireframeDebugPlugin};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WireframeDebugPlugin)
            .add_startup_system(setup_spawn_chunks)
            .add_startup_system(setup_render_pipeline)
            .add_system(chunk_entities_sync_system)
            .add_system(generate_chunk_system)
            .add_system(compute_voxel_occlusion_system)
            .add_system(compute_vertices_system)
            .add_system(generate_mesh_system);
    }
}

// Events
pub enum ChunkCommand {
    Spawn(IVec3),
    ComputeOcclusion(IVec3),
    GenerateVertices(IVec3),
    GenerateMesh(IVec3),
}

// Resources
pub struct ChunkPipelineRes(Handle<PipelineDescriptor>);

#[derive(Debug, Default, Clone)]
pub struct ChunkEntitiesRes(pub HashMap<IVec3, Entity>);

// Components
#[derive(Debug, Default, Clone, Copy)]
pub struct Chunk {
    local_pos: IVec3,
}

pub struct ChunkMesh;

pub struct ChunkVoxels(pub [u8; chunk::BUFFER_SIZE]);

pub struct ChunkVoxelOcclusion([[bool; 6]; chunk::BUFFER_SIZE]);

pub struct ChunkVertices(pub [Vec<[f32; 3]>; 6]);

// Systems

fn setup_render_pipeline(
    mut commands: Commands,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    asset_server: Res<AssetServer>,
) {
    let pipeline_handle = pipelines.add(PipelineDescriptor {
        // primitive: PrimitiveState {
        //     topology: PrimitiveTopology::TriangleList,
        //     strip_index_format: None,
        //     front_face: FrontFace::Ccw,
        //     cull_mode: Some(Face::Back),
        //     polygon_mode: PolygonMode::Fill,
        //     clamp_depth: false,
        //     conservative: false,
        // },
        ..PipelineDescriptor::default_config(ShaderStages {
            vertex: asset_server.load("shaders/voxel.vert"),
            fragment: Some(asset_server.load("shaders/voxel.frag")),
        })
    });

    commands.insert_resource(ChunkPipelineRes(pipeline_handle));
    commands.insert_resource(ChunkEntitiesRes::default());
}

fn setup_spawn_chunks(mut commands: Commands) {
    commands.spawn().insert(Chunk {
        local_pos: IVec3::ZERO,
    });
    // commands.spawn().insert(Chunk {
    //     local_pos: IVec3::new(1, 0, 0),
    // });
}

fn chunk_entities_sync_system(
    mut chunk_map: ResMut<ChunkEntitiesRes>,
    q_added: Query<(Entity, &Chunk), Added<Chunk>>,
    q_existing_entities: Query<Entity, With<Chunk>>,
) {
    for (e, c) in q_added.iter() {
        debug!("Adding {:?}", &c);
        chunk_map.0.insert(c.local_pos, e);
    }

    let before = chunk_map.0.len();

    chunk_map.0.retain(|_, e| {
        q_existing_entities
            .iter()
            .any(|existing_e| existing_e == *e)
    });

    if before != chunk_map.0.len() {
        debug!("Removed {} chunk(s)", before - chunk_map.0.len());
    }
}

fn generate_chunk_system(mut commands: Commands, q: Query<Entity, (With<Chunk>, Without<ChunkVoxels>)>) {
    for e in q.iter() {
        //TODO: Generate the chunk based on noise. For now, just fill it all with 1
        commands
            .entity(e)
            .insert(ChunkVoxels([1; chunk::BUFFER_SIZE]));
    }
}

fn compute_voxel_occlusion_system(
    mut commands: Commands,
    q: Query<(Entity, &ChunkVoxels), (With<Chunk>, Without<ChunkVoxelOcclusion>)>,
) {
    for (e, voxels) in q.iter() {
        trace!("compute_voxel_occlusion {:?}", e);
        let mut voxel_occlusions = [[false; 6]; chunk::BUFFER_SIZE];

        for (index, occlusion) in voxel_occlusions.iter_mut().enumerate() {
            let pos = chunk::to_xyz_ivec3(index);

            if voxels.0[index] == 0 {
                for s in occlusion {
                    *s = true;
                }
                continue;
            }

            for side in voxel::SIDES {
                let dir = voxel::get_side_dir(side);
                let neighbor_pos = pos + dir;

                if !chunk::is_whitin_bounds(neighbor_pos) {
                    continue;
                }

                let neighbor_idx = chunk::to_index(
                    neighbor_pos.x as usize,
                    neighbor_pos.y as usize,
                    neighbor_pos.z as usize,
                );

                assert!(neighbor_idx < chunk::BUFFER_SIZE);

                if voxels.0[neighbor_idx] == 1 {
                    occlusion[side as usize] = true;
                }
            }
        }

        commands
            .entity(e)
            .insert(ChunkVoxelOcclusion(voxel_occlusions));
    }
}

fn compute_vertices_system(
    mut commands: Commands,
    query: Query<(Entity, &ChunkVoxelOcclusion), (With<ChunkVoxels>, Without<ChunkVertices>)>,
) {
    for (e, occlusions) in query.iter() {
        trace!("compute_vertices {:?}", e);
        let mut computed_vertices: [Vec<[f32; 3]>; 6] =
            [vec![], vec![], vec![], vec![], vec![], vec![]];

        for (index, occlusion) in occlusions.0.iter().enumerate() {
            let pos = chunk::to_xyz_ivec3(index);

            for side in voxel::SIDES {
                if occlusion[side as usize] {
                    continue;
                }

                let side_idx = side as usize;

                for idx in mesh::VERTICES_INDICES[side_idx] {
                    let vertices = &mesh::VERTICES[idx];

                    computed_vertices[side_idx].push([
                        vertices[0] + pos.x as f32,
                        vertices[1] + pos.y as f32,
                        vertices[2] + pos.z as f32,
                    ]);
                }
            }
        }

        commands.entity(e).insert(ChunkVertices(computed_vertices));
    }
}

fn generate_mesh_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_pipeline: Res<ChunkPipelineRes>,
    q: Query<(Entity, &Chunk, &ChunkVertices), (Added<ChunkVertices>, Without<ChunkMesh>)>,
) {
    for (e, c, vertices) in q.iter() {
        trace!("generate_mesh {:?}", e);
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        let mut positions: Vec<[f32; 3]> = vec![];
        let mut normals: Vec<[f32; 3]> = vec![];

        for side in voxel::SIDES {
            let side_idx = side as usize;
            let side_vertices = &vertices.0[side_idx];

            positions.extend(side_vertices);
            normals.extend(vec![voxel::get_side_normal(side); side_vertices.len()])
        }

        let vertex_count = positions.len();

        mesh.set_indices(Some(Indices::U32(mesh::compute_indices(vertex_count))));
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

        let world_position = chunk::to_world(c.local_pos);

        commands
            .entity(e)
            .insert_bundle(MeshBundle {
                mesh: meshes.add(mesh),
                render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                    chunk_pipeline.0.clone(),
                )]),
                transform: Transform::from_translation(world_position),
                ..Default::default()
            })
            .insert(ChunkMesh);
    }
}
