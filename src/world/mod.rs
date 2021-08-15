#![allow(clippy::type_complexity)]

use bevy::{
    math,
    prelude::*,
    render::{
        mesh::Indices,
        pipeline::{PipelineDescriptor, RenderPipeline},
        shader::ShaderStages,
    },
};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup)
            .add_startup_system(setup_render_pipeline)
            .add_system(generate_chunk)
            .add_system(compute_voxel_occlusion)
            .add_system(compute_vertices)
            .add_system(generate_mesh);
    }
}

pub struct ChunkPipeline(Handle<PipelineDescriptor>);

fn setup_render_pipeline(
    mut commands: Commands,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    asset_server: Res<AssetServer>,
) {
    let pipeline_handle = pipelines.add(PipelineDescriptor::default_config(ShaderStages {
        vertex: asset_server.load("shaders/voxel.vert"),
        fragment: Some(asset_server.load("shaders/voxel.frag")),
    }));

    commands.insert_resource(ChunkPipeline(pipeline_handle));
}

fn setup(mut commands: Commands) {
    commands.spawn().insert(Chunk);
}

const CHUNK_AXIS_SIZE: usize = 16;
const CHUNK_AXIS_OFFSET: usize = CHUNK_AXIS_SIZE / 2;
const CHUNK_BUFFER_SIZE: usize = CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE;

const X_MASK: usize = 0b_1111_0000_0000;
const Z_MASK: usize = 0b_0000_1111_0000;
const Y_MASK: usize = 0b_0000_0000_1111;

const X_SHIFT: usize = 8;
const Z_SHIFT: usize = 4;
const Y_SHIFT: usize = 0;

struct Chunk;

struct ChunkTypes([u8; CHUNK_BUFFER_SIZE]);

fn generate_chunk(mut commands: Commands, q: Query<Entity, (With<Chunk>, Without<ChunkTypes>)>) {
    for e in q.iter() {
        //TODO: Generate the chunk based on noise. For now, just fill it all with 1
        commands
            .entity(e)
            .insert(ChunkTypes([1; CHUNK_BUFFER_SIZE]));
    }
}

#[derive(Clone, Copy, Debug)]
enum VoxelSides {
    Right = 0,
    Left = 1,
    Up = 2,
    Down = 3,
    Front = 4,
    Back = 5,
}

impl VoxelSides {
    // fn opposite(&self) -> VoxelSides {
    //     match self {
    //         VoxelSides::Right => VoxelSides::Left,
    //         VoxelSides::Left => VoxelSides::Right,
    //         VoxelSides::Up => VoxelSides::Down,
    //         VoxelSides::Down => VoxelSides::Up,
    //         VoxelSides::Front => VoxelSides::Back,
    //         VoxelSides::Back => VoxelSides::Front,
    //     }
    // }
}

const VOXEL_SIDES: [VoxelSides; 6] = [
    VoxelSides::Right,
    VoxelSides::Left,
    VoxelSides::Up,
    VoxelSides::Down,
    VoxelSides::Front,
    VoxelSides::Back,
];

struct ChunkVoxelOcclusion([[bool; 6]; CHUNK_BUFFER_SIZE]);

fn compute_voxel_occlusion(
    mut commands: Commands,
    q: Query<(Entity, &ChunkTypes), (With<Chunk>, Without<ChunkVoxelOcclusion>)>,
) {
    for (e, types) in q.iter() {
        let mut voxel_occlusions = [[false; 6]; CHUNK_BUFFER_SIZE];

        for (index, occlusion) in voxel_occlusions.iter_mut().enumerate() {
            let pos = to_xyz_ivec3(index);

            for side in VOXEL_SIDES {
                let dir = get_side_dir(side);
                let neighbor_pos = pos + dir;

                if !is_within_cubic_bounds(neighbor_pos, 0, CHUNK_AXIS_SIZE as i32 - 1) {
                    continue;
                }

                let neighbor_idx = to_index(
                    neighbor_pos.x as usize,
                    neighbor_pos.y as usize,
                    neighbor_pos.z as usize,
                );

                assert!(neighbor_idx < CHUNK_BUFFER_SIZE);

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

/*
     v3               v2
        +-----------+
  v7  / |      v6 / |
    +-----------+   |
    |   |       |   |
    |   +-------|---+
    | /  v0     | /  v1
    +-----------+
   v4           v5

   Y
   |
   +---X
  /
Z
*/

const VERTICES: [[f32; 3]; 8] = [
    [-0.5, -0.5, -0.5], //v0
    [0.5, -0.5, -0.5],  //v1
    [0.5, 0.5, -0.5],   //v2
    [-0.5, 0.5, -0.5],  //v3
    [-0.5, -0.5, 0.5],  //v4
    [0.5, -0.5, 0.5],   //v5
    [0.5, 0.5, 0.5],    //v6
    [-0.5, 0.5, 0.5],   //v7
];

const VERTICES_INDICES: [[usize; 4]; 6] = [
    [5, 1, 2, 6], //RIGHT
    [0, 4, 7, 3], //LEFT
    [7, 6, 2, 3], //UP
    [0, 1, 5, 4], //DOWN
    [4, 5, 6, 7], //FRONT
    [0, 3, 2, 1], //BACK
];

struct ChunkVertices([Vec<[f32; 3]>; 6]);

fn compute_vertices(
    mut commands: Commands,
    query: Query<(Entity, &ChunkVoxelOcclusion), (With<ChunkTypes>, Without<ChunkVertices>)>,
) {
    for (e, occlusions) in query.iter() {
        let mut computed_vertices: [Vec<[f32; 3]>; 6] =
            [vec![], vec![], vec![], vec![], vec![], vec![]];

        for (index, occlusion) in occlusions.0.iter().enumerate() {
            let pos = to_xyz_ivec3(index);

            for side in VOXEL_SIDES {
                if occlusion[side as usize] {
                    continue;
                }

                let side_idx = side as usize;

                for idx in VERTICES_INDICES[side_idx] {
                    let vertices = &VERTICES[idx];

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
    q: Query<(Entity, &ChunkVertices), (Added<ChunkVertices>, Without<ChunkMesh>)>,
) {
    for (e, vertices) in q.iter() {
        let mut mesh = Mesh::new(bevy::render::pipeline::PrimitiveTopology::TriangleList);

        let mut positions: Vec<[f32; 3]> = vec![];
        let mut normals: Vec<[f32; 3]> = vec![];

        for side in VOXEL_SIDES {
            let side_idx = side as usize;
            let side_vertices = &vertices.0[side_idx];

            positions.extend(side_vertices);
            normals.extend(vec![get_side_normal(side); side_vertices.len()])
        }

        let vertex_count = positions.len();

        mesh.set_indices(Some(Indices::U32(compute_indices(vertex_count))));
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

        commands
            .entity(e)
            .insert_bundle(MeshBundle {
                mesh: meshes.add(mesh),
                render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                    chunk_pipeline.0.clone(),
                )]),
                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                ..Default::default()
            })
            .insert(ChunkMesh);
    }
}

fn compute_indices(vertex_count: usize) -> Vec<u32> {
    // Each 4 vertex is a voxel face and each voxel face has 6 indices, so we can multiply the vertex count by 1.5
    let index_count = (vertex_count as f32 * 1.5) as usize;

    let mut res = vec![0; index_count];
    let mut i = 0u32;

    while i < vertex_count as u32 {
        res.push(i);
        res.push(i + 1);
        res.push(i + 2);

        res.push(i + 2);
        res.push(i + 3);
        res.push(i);

        i += 4;
    }

    res
}

fn get_side_normal(side: VoxelSides) -> [f32; 3] {
    match side {
        VoxelSides::Right => [1.0, 0.0, 0.0],
        VoxelSides::Left => [-1.0, 0.0, 0.0],
        VoxelSides::Up => [0.0, 1.0, 0.0],
        VoxelSides::Down => [0.0, -1.0, 0.0],
        VoxelSides::Front => [0.0, 0.0, 1.0],
        VoxelSides::Back => [0.0, 0.0, -1.0],
    }
}

// UTILITIES
fn to_xyz(index: usize) -> (usize, usize, usize) {
    (
        (index & X_MASK) >> X_SHIFT,
        (index & Y_MASK) >> Y_SHIFT,
        (index & Z_MASK) >> Z_SHIFT,
    )
}

fn to_xyz_ivec3(index: usize) -> IVec3 {
    let (x, y, z) = to_xyz(index);
    IVec3::new(x as i32, y as i32, z as i32)
}

fn to_index(x: usize, y: usize, z: usize) -> usize {
    x << X_SHIFT | y << Y_SHIFT | z << Z_SHIFT
}

fn get_side_dir(side: VoxelSides) -> IVec3 {
    match side {
        VoxelSides::Right => IVec3::X,
        VoxelSides::Left => -IVec3::X,
        VoxelSides::Up => IVec3::Y,
        VoxelSides::Down => -IVec3::Y,
        VoxelSides::Front => IVec3::Z,
        VoxelSides::Back => -IVec3::Z,
    }
}

fn is_within_cubic_bounds(pos: IVec3, min: i32, max: i32) -> bool {
    pos.min_element() >= min && pos.max_element() <= max
}

fn is_on_chunk_bounds(pos: IVec3) -> bool {
    is_within_cubic_bounds(pos, 0, CHUNK_AXIS_SIZE as i32 - 1)
}

struct RaycastTraversal {
    dir: Vec3,
    next: Vec3,
}

impl RaycastTraversal {
    fn new(origin: Vec3, dir: Vec3) -> Self {
        Self {
            dir: dir.normalize(),
            next: origin,
        }
    }
}

impl Iterator for RaycastTraversal {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        let current = to_ivec3(self.next);

        if !is_on_chunk_bounds(current) {
            None
        } else {
            let next_voxel_dir = IVec3::new(
                self.dir.x.signum() as i32,
                self.dir.y.signum() as i32,
                self.dir.z.signum() as i32,
            );
            let next_voxel = current + next_voxel_dir;

            let delta = (next_voxel.as_f32() - self.next) / self.dir;
            let length = get_min_abs_axis(delta);

            self.next += self.dir * length;

            Some(current)
        }
    }
}

fn to_ivec3(vec: Vec3) -> IVec3 {
    IVec3::new(
        vec.x.trunc() as i32,
        vec.y.trunc() as i32,
        vec.z.trunc() as i32,
    )
}

fn get_min_abs_axis(vec: Vec3) -> f32 {
    let abs = vec.abs();
    if abs.x < abs.y && abs.x < abs.z {
        vec.x
    } else if abs.y < abs.x && abs.y < abs.z {
        vec.y
    } else {
        vec.z
    }
}

fn to_unit_axis_ivec3(vec: Vec3) -> IVec3 {
    let abs = vec.normalize().abs();
    if abs.x > abs.y && abs.x > abs.z {
        (vec.x.signum() as i32) * IVec3::X
    } else if abs.y > abs.x && abs.y > abs.z {
        (vec.y.signum() as i32) * IVec3::Y
    } else {
        (vec.z.signum() as i32) * IVec3::Z
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::{IVec3, Vec3};

    use crate::world::to_unit_axis_ivec3;

    use super::{to_index, to_xyz, RaycastTraversal, CHUNK_AXIS_SIZE};

    #[test]
    fn index_to_xyz() {
        assert_eq!((0, 0, 0), to_xyz(0));
        assert_eq!((0, 1, 0), to_xyz(1));
        assert_eq!((0, 2, 0), to_xyz(2));

        assert_eq!((0, 0, 1), to_xyz(CHUNK_AXIS_SIZE));
        assert_eq!((0, 1, 1), to_xyz(CHUNK_AXIS_SIZE + 1));
        assert_eq!((0, 2, 1), to_xyz(CHUNK_AXIS_SIZE + 2));

        assert_eq!((1, 0, 0), to_xyz(CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE));
        assert_eq!((1, 1, 0), to_xyz(CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + 1));
        assert_eq!((1, 2, 0), to_xyz(CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + 2));

        assert_eq!(
            (1, 0, 1),
            to_xyz(CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + CHUNK_AXIS_SIZE)
        );
        assert_eq!(
            (1, 1, 1),
            to_xyz(CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + CHUNK_AXIS_SIZE + 1)
        );
        assert_eq!(
            (1, 2, 1),
            to_xyz(CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + CHUNK_AXIS_SIZE + 2)
        );
    }

    #[test]
    fn xyz_to_index() {
        assert_eq!(to_index(0, 0, 0), 0);
        assert_eq!(to_index(0, 1, 0), 1);
        assert_eq!(to_index(0, 2, 0), 2);

        assert_eq!(to_index(0, 0, 1), CHUNK_AXIS_SIZE);
        assert_eq!(to_index(0, 1, 1), CHUNK_AXIS_SIZE + 1);
        assert_eq!(to_index(0, 2, 1), CHUNK_AXIS_SIZE + 2);

        assert_eq!(to_index(1, 0, 0), CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE);
        assert_eq!(to_index(1, 1, 0), CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + 1);
        assert_eq!(to_index(1, 2, 0), CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + 2);

        assert_eq!(
            to_index(1, 0, 1),
            CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + CHUNK_AXIS_SIZE
        );
        assert_eq!(
            to_index(1, 1, 1),
            CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + CHUNK_AXIS_SIZE + 1
        );
        assert_eq!(
            to_index(1, 2, 1),
            CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE + CHUNK_AXIS_SIZE + 2
        );
    }

    #[test]
    fn test_to_unit_axis_ivec3() {
        assert_eq!(IVec3::X, to_unit_axis_ivec3(Vec3::new(0.8, 0.3, 0.3)));
        assert_eq!(IVec3::X, to_unit_axis_ivec3(Vec3::new(1.2, 1.1, 1.1999)));
        assert_eq!(
            IVec3::X,
            to_unit_axis_ivec3(Vec3::new(0.001, 0.0001, 0.0001))
        );

        assert_eq!(-IVec3::X, to_unit_axis_ivec3(Vec3::new(-0.8, 0.3, 0.3)));
        assert_eq!(-IVec3::X, to_unit_axis_ivec3(Vec3::new(-1.2, 1.1, 1.1999)));
        assert_eq!(
            -IVec3::X,
            to_unit_axis_ivec3(Vec3::new(-0.001, 0.0001, 0.0001))
        );

        assert_eq!(
            IVec3::Y,
            to_unit_axis_ivec3(Vec3::new(0.0001, 0.001, 0.0001))
        );
        assert_eq!(IVec3::Y, to_unit_axis_ivec3(Vec3::new(-3.0, 3.001, -3.0)));

        assert_eq!(
            -IVec3::Y,
            to_unit_axis_ivec3(Vec3::new(0.0001, -0.001, 0.0001))
        );
        assert_eq!(-IVec3::Y, to_unit_axis_ivec3(Vec3::new(-3.0, -3.001, -3.0)));

        assert_eq!(IVec3::Z, to_unit_axis_ivec3(Vec3::new(0.0001, 0.1, 1.0)));
        assert_eq!(IVec3::Z, to_unit_axis_ivec3(Vec3::new(0.0, 0.0, 1.0)));

        assert_eq!(-IVec3::Z, to_unit_axis_ivec3(Vec3::new(0.0001, 0.1, -1.0)));
        assert_eq!(-IVec3::Z, to_unit_axis_ivec3(Vec3::new(0.0, 0.0, -1.0)));

        assert_eq!(IVec3::Z, to_unit_axis_ivec3(Vec3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn test_raycast_traversal() {
        let raycast = RaycastTraversal::new(Vec3::new(0.2, 0.2, 0.2), Vec3::new(0.2, 0.3, -0.4));
        dbg!("asd");

        for pos in raycast {
            dbg!(pos);
        }
    }
}
