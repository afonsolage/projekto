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

use super::{chunk, debug::WireframeDebugPlugin, landscape};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WireframeDebugPlugin)
            .add_event::<ChunkSpawnCmd>()
            .add_event::<ChunkDespawnCmd>()
            .add_event::<ChunkSetVoxelCmd>()
            .add_event::<VoxelUpdated>()
            .add_event::<OcclusionDone>()
            .add_event::<VerticesDone>()
            .add_event::<Meshed>()
            .add_startup_system(setup_landscape)
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

struct VoxelUpdated(Entity);
struct OcclusionDone(Entity);
struct VerticesDone(Entity);
struct Meshed(Entity);

// Resources
pub struct ChunkPipelineRes(Handle<PipelineDescriptor>);

#[derive(Debug, Default, Clone)]
pub struct ChunkEntitiesRes(pub HashMap<IVec3, Entity>);

// Components
pub struct ChunkLocal(IVec3);

pub struct ChunkVoxels(pub [u8; chunk::BUFFER_SIZE]);

struct ChunkVoxelOcclusion([[bool; 6]; chunk::BUFFER_SIZE]);

pub(super) struct ChunkVertices(pub [Vec<[f32; 3]>; 6]);

#[derive(Bundle)]
struct ChunkBundle {
    voxels: ChunkVoxels,
    occlusion: ChunkVoxelOcclusion,
    vertices: ChunkVertices,
    #[bundle]
    mesh_bundle: MeshBundle,
    local: ChunkLocal,
}

impl Default for ChunkBundle {
    fn default() -> Self {
        Self {
            voxels: ChunkVoxels([0; chunk::BUFFER_SIZE]),
            occlusion: ChunkVoxelOcclusion([[false; 6]; chunk::BUFFER_SIZE]),
            vertices: ChunkVertices([vec![], vec![], vec![], vec![], vec![], vec![]]),
            local: ChunkLocal(IVec3::ZERO),
            mesh_bundle: MeshBundle::default(),
        }
    }
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

fn setup_landscape(mut command_writer: EventWriter<ChunkSpawnCmd>) {
    for x in landscape::BEGIN..landscape::END {
        for z in landscape::BEGIN..landscape::END {
            for y in landscape::BEGIN..landscape::END {
                command_writer.send(ChunkSpawnCmd(IVec3::new(x, y, z)));
            }
        }
    }
}

fn spawn_chunk_system(
    mut commands: Commands,
    mut spawn_reader: EventReader<ChunkSpawnCmd>,
    mut chunk_entities: ResMut<ChunkEntitiesRes>,
    mut event_queue: Local<VecDeque<ChunkSpawnCmd>>,
    chunk_pipeline: Res<ChunkPipelineRes>,
) {
    // Copy all incoming events to local queue, so we don't miss any events
    for cmd in spawn_reader.iter() {
        event_queue.push_back(*cmd)
    }

    if let Some(cmd) = event_queue.pop_front() {
        debug!("[spawn_chunk_system] Spawning chunk at {}", cmd.0);
        let entity = commands
            .spawn()
            .insert_bundle(ChunkBundle {
                local: ChunkLocal(cmd.0),
                mesh_bundle: MeshBundle {
                    render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                        chunk_pipeline.0.clone(),
                    )]),
                    transform: Transform::from_translation(chunk::to_world(cmd.0)),
                    ..Default::default()
                },
                ..Default::default()
            })
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
        debug!("[despawn_chunk_system] Despawning chunk at {}", cmd.0);

        match chunk_entities.0.remove(&cmd.0) {
            None => {
                warn!(
                    "[despawn_chunk_system] Trying to despawn a non-existing chunk {}",
                    cmd.0
                );
                return;
            }
            Some(e) => {
                commands.entity(e).despawn();
            }
        }
    }
}

fn set_voxel_system(
    mut event_reader: EventReader<ChunkSetVoxelCmd>,
    mut event_writer: EventWriter<VoxelUpdated>,
    chunk_entities: Res<ChunkEntitiesRes>,
    mut chunk_voxels: Query<&mut ChunkVoxels>,
) {
    for cmd in event_reader.iter() {
        let chunk_local = chunk::to_local(cmd.world_pos);

        let entity = match chunk_entities.0.get(&chunk_local) {
            None => {
                warn!(
                    "[set_voxel_system] Trying to set voxel in a non-existing chunk {}",
                    chunk_local
                );
                return;
            }
            Some(e) => *e,
        };

        let mut voxels = match chunk_voxels.get_mut(entity) {
            Err(e) => {
                warn!(
                    "[set_voxel_system] Failed to set voxel on chunk {}. Error: {}",
                    chunk_local, e
                );
                return;
            }
            Ok(v) => v,
        };

        let voxel_local = voxel::to_local(cmd.world_pos);
        let index = chunk::to_index_ivec3(voxel_local);

        voxels.0[index] = cmd.new_value;

        debug!(
            "[set_voxel_system] Updating voxel at {} to {}",
            cmd.world_pos, cmd.new_value
        );

        event_writer.send(VoxelUpdated(entity));
    }
}

fn generate_chunk_system(
    mut q: Query<(Entity, &ChunkLocal, &mut ChunkVoxels), Added<ChunkLocal>>,
    mut event_writer: EventWriter<VoxelUpdated>,
) {
    for (e, c, mut voxels) in q.iter_mut() {
        let world = chunk::to_world(c.0);
        trace!("[generate_chunk_system] Generating chunk at {}", world);

        // TODO: How to generate for negative height chunks?
        if world.y < 0.0 {
            continue;
        }

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
                let world_height = ((h + 1.0) / 2.0) * (2 * chunk::AXIS_SIZE) as f32;

                let height_local = world_height - world.y;

                if height_local < f32::EPSILON {
                    continue;
                }

                let end = usize::min(height_local as usize, chunk::AXIS_SIZE);

                for y in 0..end {
                    let index = chunk::to_index(x, y, z);

                    voxels.0[index] = 1;
                }
            }
        }

        event_writer.send(VoxelUpdated(e));
    }
}

fn compute_voxel_occlusion_system(
    mut event_reader: EventReader<VoxelUpdated>,
    mut event_writer: EventWriter<OcclusionDone>,
    mut q: Query<(&ChunkLocal, &ChunkVoxels, &mut ChunkVoxelOcclusion)>,
) {
    for evt in event_reader.iter() {
        let e = evt.0;

        let (local, voxels, mut voxel_occlusions) = match q.get_mut(e) {
            Err(err) => {
                warn!(
                    "[compute_voxel_occlusion_system] Failed to get entity {:?} ({})",
                    e, err
                );
                continue;
            }
            Ok(t) => t,
        };

        trace!(
            "[compute_voxel_occlusion_system] compute_voxel_occlusion {}",
            local.0
        );

        voxel_occlusions.0.fill([false; 6]);

        for (index, occlusion) in voxel_occlusions.0.iter_mut().enumerate() {
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
                    // TODO: Check neighborhood
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

        event_writer.send(OcclusionDone(e));
    }
}

fn compute_vertices_system(
    mut evt_reader: EventReader<OcclusionDone>,
    mut evt_writer: EventWriter<VerticesDone>,
    mut q: Query<(&ChunkLocal, &ChunkVoxelOcclusion, &mut ChunkVertices)>,
) {
    for evt in evt_reader.iter() {
        let e = evt.0;

        let (local, occlusions, mut computed_vertices) = match q.get_mut(e) {
            Err(err) => {
                warn!(
                    "[compute_vertices_system] Failed to get entity {:?} ({})",
                    e, err
                );
                continue;
            }
            Ok(t) => t,
        };

        trace!(
            "[compute_vertices_system] compute_vertices_system {}",
            local.0
        );

        for side in voxel::SIDES {
            computed_vertices.0[side as usize].clear();
        }

        for (index, occlusion) in occlusions.0.iter().enumerate() {
            let pos = chunk::to_xyz_ivec3(index);

            for side in voxel::SIDES {
                if occlusion[side as usize] {
                    continue;
                }

                let side_idx = side as usize;

                for idx in mesh::VERTICES_INDICES[side_idx] {
                    let vertices = &mesh::VERTICES[idx];

                    computed_vertices.0[side_idx].push([
                        vertices[0] + pos.x as f32,
                        vertices[1] + pos.y as f32,
                        vertices[2] + pos.z as f32,
                    ]);
                }
            }
        }

        evt_writer.send(VerticesDone(e));
    }
}

fn generate_mesh_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    q: Query<(&ChunkLocal, &ChunkVertices)>,
    mut evt_reader: EventReader<VerticesDone>,
) {
    for evt in evt_reader.iter() {
        let e = evt.0;

        let (local, vertices) = match q.get(e) {
            Err(err) => {
                warn!(
                    "[generate_mesh_system] Failed to get entity {:?} ({})",
                    e, err
                );
                continue;
            }
            Ok(t) => t,
        };

        trace!("[generate_mesh_system] generate_mesh_system {}", local.0);

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

        commands
            .entity(e)
            .insert(meshes.add(mesh))
            .insert(Visible::default());
    }
}
