use bevy::prelude::*;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup)
            .add_system(generate_chunk)
            .add_system(compute_vertices);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn().insert(Chunk);
}

const CHUNK_AXIS_SIZE: usize = 16;
const CHUNK_BUFFER_SIZE: usize = CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE;

const X_MASK: usize = 0b_1111_0000_0000;
const Z_MASK: usize = 0b_0000_1111_0000;
const Y_MASK: usize = 0b_0000_0000_1111;

const X_SHIFT: usize = 8;
const Z_SHIFT: usize = 4;
const Y_SHIFT: usize = 0;

struct Chunk;

struct ChunkTypes([u8; CHUNK_BUFFER_SIZE]);

fn generate_chunk(mut commands: Commands, q: Query<Entity, (Added<Chunk>, Without<ChunkTypes>)>) {
    for e in q.iter() {
        //TODO: Generate the chunk based on noise. For now, just fill it all with 1
        commands
            .entity(e)
            .insert(ChunkTypes([1; CHUNK_BUFFER_SIZE]));
    }
}

enum VoxelSides {
    RIGHT = 0,
    LEFT = 1,
    UP = 2,
    DOWN = 3,
    FRONT = 4,
    BACK = 5,
}

const VOXEL_SIDES: [VoxelSides; 6] = [
    VoxelSides::RIGHT,
    VoxelSides::LEFT,
    VoxelSides::UP,
    VoxelSides::DOWN,
    VoxelSides::FRONT,
    VoxelSides::BACK,
];

struct ChunkVertices([Vec<f32>; 6]);

fn compute_vertices(
    mut commands: Commands,
    q: Query<(Entity, &ChunkTypes), (Added<ChunkTypes>, Without<ChunkVertices>)>,
) {
    for (e, _) in q.iter() {
        let mut vertices: [Vec<f32>; 6] = [vec![], vec![], vec![], vec![], vec![], vec![]];

        for i in 0..CHUNK_BUFFER_SIZE {
            let x = (i & X_MASK) >> X_SHIFT;
            let y = (i & Y_MASK) >> Y_SHIFT;
            let z = (i & Z_MASK) >> Z_SHIFT;

            for side in VOXEL_SIDES {
                // TODO: Check if side is ocludded

                let side_idx = side as usize;

                for idx in VERTICES_INDICES[side_idx] {
                    let v = &VERTICES[idx];

                    vertices[side_idx].push(v[0] + x as f32);
                    vertices[side_idx].push(v[1] + y as f32);
                    vertices[side_idx].push(v[2] + z as f32);
                }
            }
        }

        commands.entity(e).insert(ChunkVertices(vertices));
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
    [0, 4, 5, 1], //DOWN
    [4, 5, 6, 7], //FRONT
    [0, 3, 2, 1], //BACK
];
