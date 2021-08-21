#![allow(clippy::type_complexity)]

mod chunk;
mod debug;
mod math;
mod mesh;
mod voxel;

use std::collections::HashMap;

use bevy::{
    prelude::*,
    render::{
        mesh::Indices,
        pipeline::{
            FrontFace, PipelineDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
            RenderPipeline,
        },
        shader::ShaderStages,
    },
};

use self::debug::WireframeDebugPlugin;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WireframeDebugPlugin)
            .add_startup_system(setup)
            .add_startup_system(setup_render_pipeline)
            .add_system(chunk_entities_sync)
            .add_system(generate_chunk)
            .add_system(compute_voxel_occlusion)
            .add_system(compute_vertices)
            .add_system(generate_mesh);
    }
}

struct ChunkPipeline(Handle<PipelineDescriptor>);

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

    commands.insert_resource(ChunkPipeline(pipeline_handle));
    commands.insert_resource(ChunkEntities::default());
}

fn setup(mut commands: Commands) {
    commands.spawn().insert(Chunk {
        local_pos: IVec3::ZERO,
    });
}

#[derive(Debug, Default, Clone, Copy)]
struct Chunk {
    local_pos: IVec3,
}

#[derive(Debug, Default, Clone)]
struct ChunkEntities(HashMap<IVec3, Entity>);

fn chunk_entities_sync(
    mut chunk_map: ResMut<ChunkEntities>,
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
struct ChunkTypes([u8; chunk::BUFFER_SIZE]);

fn generate_chunk(mut commands: Commands, q: Query<Entity, (With<Chunk>, Without<ChunkTypes>)>) {
    for e in q.iter() {
        //TODO: Generate the chunk based on noise. For now, just fill it all with 1
        commands
            .entity(e)
            .insert(ChunkTypes([1; chunk::BUFFER_SIZE]));
    }
}

struct ChunkVoxelOcclusion([[bool; 6]; chunk::BUFFER_SIZE]);

fn compute_voxel_occlusion(
    mut commands: Commands,
    q: Query<(Entity, &ChunkTypes), (With<Chunk>, Without<ChunkVoxelOcclusion>)>,
) {
    for (e, types) in q.iter() {
        let mut voxel_occlusions = [[false; 6]; chunk::BUFFER_SIZE];

        for (index, occlusion) in voxel_occlusions.iter_mut().enumerate() {
            let pos = chunk::to_xyz_ivec3(index);

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

                if types.0[neighbor_idx] == 1 {
                    occlusion[side as usize] = true;
                }
            }
        }

        commands
            .entity(e)
            .insert(ChunkVoxelOcclusion(voxel_occlusions));
    }
}

struct ChunkVertices([Vec<[f32; 3]>; 6]);

fn compute_vertices(
    mut commands: Commands,
    query: Query<(Entity, &ChunkVoxelOcclusion), (With<ChunkTypes>, Without<ChunkVertices>)>,
) {
    for (e, occlusions) in query.iter() {
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

struct ChunkMesh;
fn generate_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_pipeline: Res<ChunkPipeline>,
    q: Query<(Entity, &Chunk, &ChunkVertices), (Added<ChunkVertices>, Without<ChunkMesh>)>,
) {
    for (e, c, vertices) in q.iter() {
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

pub fn raycast(origin: Vec3, dir: Vec3, length: f32) -> (Vec<IVec3>, Vec<Vec3>, Vec<IVec3>) {
    let mut visited_chunks = vec![];
    let mut visited_positions = vec![];
    let mut visited_normals = vec![];

    let mut current_pos = origin;
    let mut current_chunk = chunk::to_local(origin);
    let mut last_chunk = current_chunk;

    let grid_dir = math::to_grid_dir(dir);
    let tile_offset = IVec3::new(
        if dir.x >= 0.0 { 1 } else { 0 },
        if dir.y >= 0.0 { 1 } else { 0 },
        if dir.z >= 0.0 { 1 } else { 0 },
    );

    while current_pos.distance(origin) < length {
        visited_chunks.push(current_chunk);
        visited_positions.push(current_pos);
        visited_normals.push(last_chunk - current_chunk);

        last_chunk = current_chunk;

        let next_chunk = current_chunk + tile_offset;
        let delta = (chunk::to_world(next_chunk) - current_pos) / dir;
        let distance = if delta.x < delta.y && delta.x < delta.z {
            current_chunk.x += grid_dir.x;
            delta.x
        } else if delta.y < delta.x && delta.y < delta.z {
            current_chunk.y += grid_dir.y;
            delta.y
        } else {
            current_chunk.z += grid_dir.z;
            delta.z
        };

        current_pos += distance * dir * 1.01;
    }

    (visited_chunks, visited_positions, visited_normals)
}

#[cfg(test)]
mod tests {
    // use bevy::math::{IVec3, Vec3};

    #[test]
    fn to_xyz() {
        use super::chunk;

        assert_eq!((0, 0, 0), chunk::to_xyz(0));
        assert_eq!((0, 1, 0), chunk::to_xyz(1));
        assert_eq!((0, 2, 0), chunk::to_xyz(2));

        assert_eq!((0, 0, 1), chunk::to_xyz(chunk::AXIS_SIZE));
        assert_eq!((0, 1, 1), chunk::to_xyz(chunk::AXIS_SIZE + 1));
        assert_eq!((0, 2, 1), chunk::to_xyz(chunk::AXIS_SIZE + 2));

        assert_eq!(
            (1, 0, 0),
            chunk::to_xyz(chunk::AXIS_SIZE * chunk::AXIS_SIZE)
        );
        assert_eq!(
            (1, 1, 0),
            chunk::to_xyz(chunk::AXIS_SIZE * chunk::AXIS_SIZE + 1)
        );
        assert_eq!(
            (1, 2, 0),
            chunk::to_xyz(chunk::AXIS_SIZE * chunk::AXIS_SIZE + 2)
        );

        assert_eq!(
            (1, 0, 1),
            chunk::to_xyz(chunk::AXIS_SIZE * chunk::AXIS_SIZE + chunk::AXIS_SIZE)
        );
        assert_eq!(
            (1, 1, 1),
            chunk::to_xyz(chunk::AXIS_SIZE * chunk::AXIS_SIZE + chunk::AXIS_SIZE + 1)
        );
        assert_eq!(
            (1, 2, 1),
            chunk::to_xyz(chunk::AXIS_SIZE * chunk::AXIS_SIZE + chunk::AXIS_SIZE + 2)
        );
    }

    #[test]
    fn to_index() {
        use super::chunk;

        assert_eq!(chunk::to_index(0, 0, 0), 0);
        assert_eq!(chunk::to_index(0, 1, 0), 1);
        assert_eq!(chunk::to_index(0, 2, 0), 2);

        assert_eq!(chunk::to_index(0, 0, 1), chunk::AXIS_SIZE);
        assert_eq!(chunk::to_index(0, 1, 1), chunk::AXIS_SIZE + 1);
        assert_eq!(chunk::to_index(0, 2, 1), chunk::AXIS_SIZE + 2);

        assert_eq!(
            chunk::to_index(1, 0, 0),
            chunk::AXIS_SIZE * chunk::AXIS_SIZE
        );
        assert_eq!(
            chunk::to_index(1, 1, 0),
            chunk::AXIS_SIZE * chunk::AXIS_SIZE + 1
        );
        assert_eq!(
            chunk::to_index(1, 2, 0),
            chunk::AXIS_SIZE * chunk::AXIS_SIZE + 2
        );

        assert_eq!(
            chunk::to_index(1, 0, 1),
            chunk::AXIS_SIZE * chunk::AXIS_SIZE + chunk::AXIS_SIZE
        );
        assert_eq!(
            chunk::to_index(1, 1, 1),
            chunk::AXIS_SIZE * chunk::AXIS_SIZE + chunk::AXIS_SIZE + 1
        );
        assert_eq!(
            chunk::to_index(1, 2, 1),
            chunk::AXIS_SIZE * chunk::AXIS_SIZE + chunk::AXIS_SIZE + 2
        );
    }

    // #[test]
    // fn to_unit_axis_ivec3() {
    //     use super::math;

    //     assert_eq!(IVec3::X, math::to_unit_axis_ivec3(Vec3::new(0.8, 0.3, 0.3)));
    //     assert_eq!(IVec3::X, math::to_unit_axis_ivec3(Vec3::new(1.2, 1.1, 1.1999)));
    //     assert_eq!(
    //         IVec3::X,
    //         math::to_unit_axis_ivec3(Vec3::new(0.001, 0.0001, 0.0001))
    //     );

    //     assert_eq!(-IVec3::X, math::to_unit_axis_ivec3(Vec3::new(-0.8, 0.3, 0.3)));
    //     assert_eq!(-IVec3::X, math::to_unit_axis_ivec3(Vec3::new(-1.2, 1.1, 1.1999)));
    //     assert_eq!(
    //         -IVec3::X,
    //         math::to_unit_axis_ivec3(Vec3::new(-0.001, 0.0001, 0.0001))
    //     );

    //     assert_eq!(
    //         IVec3::Y,
    //         math::to_unit_axis_ivec3(Vec3::new(0.0001, 0.001, 0.0001))
    //     );
    //     assert_eq!(IVec3::Y, math::to_unit_axis_ivec3(Vec3::new(-3.0, 3.001, -3.0)));

    //     assert_eq!(
    //         -IVec3::Y,
    //         math::to_unit_axis_ivec3(Vec3::new(0.0001, -0.001, 0.0001))
    //     );
    //     assert_eq!(-IVec3::Y, math::to_unit_axis_ivec3(Vec3::new(-3.0, -3.001, -3.0)));

    //     assert_eq!(IVec3::Z, math::to_unit_axis_ivec3(Vec3::new(0.0001, 0.1, 1.0)));
    //     assert_eq!(IVec3::Z, math::to_unit_axis_ivec3(Vec3::new(0.0, 0.0, 1.0)));

    //     assert_eq!(-IVec3::Z, math::to_unit_axis_ivec3(Vec3::new(0.0001, 0.1, -1.0)));
    //     assert_eq!(-IVec3::Z, math::to_unit_axis_ivec3(Vec3::new(0.0, 0.0, -1.0)));

    //     assert_eq!(IVec3::Z, math::to_unit_axis_ivec3(Vec3::new(0.0, 0.0, 0.0)));
    // }

    #[test]
    fn test_raycast_traversal() {
        // dbg!("asd");
        // let origin = Vec3::new(0.2, 0.2, 0.2);
        // let dir = Vec3::new(0.2, 0.0, 0.0);
        // let (voxels, points) = chunk_raycast(origin, dir);

        // assert_eq!(voxels.len(), points.len());
        // assert_eq!(voxels[0], to_ivec3(origin));
        // assert_eq!(points[0], origin)
    }
}
