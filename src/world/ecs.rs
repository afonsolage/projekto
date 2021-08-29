use std::collections::{HashMap, VecDeque};

use bevy::{
    prelude::*,
    render::{
        mesh::Indices,
        pipeline::{PipelineDescriptor, PrimitiveTopology, RenderPipeline},
        shader::ShaderStages,
    },
};
use bracket_noise::prelude::{FastNoise, FractalType, NoiseType};

use crate::world::{mesh, voxel};

use super::{chunk, debug::WireframeDebugPlugin};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WireframeDebugPlugin)
            .add_event::<ChunkSpawnCmd>()
            .add_event::<ChunkDespawnCmd>()
            .add_event::<ChunkSetVoxelCmd>()
            .add_startup_system(setup_spawn_chunks)
            .add_startup_system(setup_render_pipeline)
            .add_system(spawn_chunk_system)
            .add_system(despawn_chunk_system)
            .add_system(set_voxel_system)
            .add_system(generate_chunk_system)
            .add_system(compute_voxel_occlusion_system)
            .add_system(compute_vertices_system)
            .add_system(generate_mesh_system);
    }
}

// Events
#[derive(Clone, Copy)]
pub struct ChunkSpawnCmd(IVec3);

pub struct ChunkDespawnCmd(IVec3);

pub struct ChunkSetVoxelCmd {
    pub world_pos: Vec3,
    pub new_value: u8,
}

// Resources
pub struct ChunkPipelineRes(Handle<PipelineDescriptor>);

#[derive(Debug, Default, Clone)]
pub struct ChunkEntitiesRes(pub HashMap<IVec3, Entity>);

// Components
struct ChunkBuilding;

pub struct ChunkDone;

pub struct ChunkLocal(IVec3);

pub struct ChunkVoxels(pub [u8; chunk::BUFFER_SIZE]);

struct ChunkVoxelOcclusion([[bool; 6]; chunk::BUFFER_SIZE]);

pub(super) struct ChunkVertices(pub [Vec<[f32; 3]>; 6]);

#[derive(Bundle)]
struct ChunkBuildPipelineBundle {
    occlusion: ChunkVoxelOcclusion,
    vertices: ChunkVertices,
    done: ChunkDone,
}

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

fn setup_spawn_chunks(mut command_writer: EventWriter<ChunkSpawnCmd>) {
    for x in -10..10 {
        for z in -10..10 {
            command_writer.send(ChunkSpawnCmd(IVec3::new(x, 0, z)));
        }
    }
}

fn spawn_chunk_system(
    mut commands: Commands,
    mut spawn_reader: EventReader<ChunkSpawnCmd>,
    mut chunk_entities: ResMut<ChunkEntitiesRes>,
    mut event_queue: Local<VecDeque<ChunkSpawnCmd>>,
) {
    // Copy all incoming events to local queue, so we don't miss any events
    for cmd in spawn_reader.iter() {
        event_queue.push_back(*cmd)
    }

    if let Some(cmd) = event_queue.pop_front() {
        debug!("Spawning chunk at {}", cmd.0);
        let entity = commands
            .spawn()
            .insert(ChunkBuilding)
            .insert(ChunkLocal(cmd.0))
            .id();

        chunk_entities.0.insert(cmd.0, entity);

        // TODO: Check this later, this is to limit to 1 chunk spawn per frame
        return;
    }
}

fn despawn_chunk_system(
    mut commands: Commands,
    mut despawn_reader: EventReader<ChunkDespawnCmd>,
    mut chunk_entities: ResMut<ChunkEntitiesRes>,
) {
    for cmd in despawn_reader.iter() {
        debug!("Despawning chunk at {}", cmd.0);

        match chunk_entities.0.remove(&cmd.0) {
            None => {
                warn!("Trying to despawn a non-existing chunk {}", cmd.0);
                return;
            }
            Some(e) => {
                commands.entity(e).despawn();
            }
        }
    }
}

fn set_voxel_system(
    mut commands: Commands,
    mut set_voxel_reader: EventReader<ChunkSetVoxelCmd>,
    chunk_entities: Res<ChunkEntitiesRes>,
    mut chunk_voxels: Query<&mut ChunkVoxels>,
) {
    for cmd in set_voxel_reader.iter() {
        let chunk_local = chunk::to_local(cmd.world_pos);

        let entity = match chunk_entities.0.get(&chunk_local) {
            None => {
                warn!(
                    "Trying to set voxel in a non-existing chunk {}",
                    chunk_local
                );
                return;
            }
            Some(e) => *e,
        };

        let mut voxels = match chunk_voxels.get_mut(entity) {
            Err(e) => {
                warn!("Failed to set voxel on chunk {}. Error: {}", chunk_local, e);
                return;
            }
            Ok(v) => v,
        };

        let voxel_local = voxel::to_local(cmd.world_pos);
        let index = chunk::to_index_ivec3(voxel_local);

        voxels.0[index] = cmd.new_value;

        debug!("Updating voxel at {} to {}", cmd.world_pos, cmd.new_value);

        commands
            .entity(entity)
            .insert(ChunkBuilding)
            .remove_bundle::<ChunkBuildPipelineBundle>();
    }
}

fn generate_chunk_system(
    mut commands: Commands,
    q: Query<(Entity, &ChunkLocal), (With<ChunkBuilding>, Without<ChunkVoxels>)>,
) {
    for (e, c) in q.iter() {
        let world = chunk::to_world(c.0);
        debug!("Generating chunk at {}", world);

        let mut voxels = [0; chunk::BUFFER_SIZE];

        let mut noise = FastNoise::seeded(15);
        noise.set_noise_type(NoiseType::SimplexFractal);
        noise.set_frequency(0.03);
        noise.set_fractal_type(FractalType::FBM);
        noise.set_fractal_octaves(3);
        noise.set_fractal_gain(0.9);
        noise.set_fractal_lacunarity(0.5);

        for x in 0..chunk::AXIS_SIZE {
            for z in 0..chunk::AXIS_SIZE {
                let h = noise.get_noise(world.x + x as f32, world.z + z as f32);
                let h = ((h + 1.0) / 2.0) * chunk::AXIS_SIZE as f32;    
                for y in 0..h as usize {
                    let index = chunk::to_index(x, y, z);

                    voxels[index] = 1;
                }
            }
        }

        commands.entity(e).insert(ChunkVoxels(voxels));
    }
}

fn compute_voxel_occlusion_system(
    mut commands: Commands,
    q: Query<(Entity, &ChunkVoxels), (With<ChunkBuilding>, Without<ChunkVoxelOcclusion>)>,
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

                if !chunk::is_within_bounds(neighbor_pos) {
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
    query: Query<(Entity, &ChunkVoxelOcclusion), (With<ChunkBuilding>, Without<ChunkVertices>)>,
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
    q: Query<(Entity, &ChunkLocal, &ChunkVertices), With<ChunkBuilding>>,
) {
    for (e, local, vertices) in q.iter() {
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

        let world_position = chunk::to_world(local.0);

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
            .remove::<ChunkBuilding>()
            .insert(ChunkDone);
    }
}
