use bevy::{
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

struct ChunkVertices([Vec<[f32; 3]>; 6]);

fn compute_vertices(
    mut commands: Commands,
    q: Query<(Entity, &ChunkTypes), (With<ChunkTypes>, Without<ChunkVertices>)>,
) {
    for (e, _) in q.iter() {
        let mut vertices: [Vec<[f32; 3]>; 6] = [vec![], vec![], vec![], vec![], vec![], vec![]];

        for i in 0..CHUNK_BUFFER_SIZE {
            let x = (i & X_MASK) >> X_SHIFT;
            let y = (i & Y_MASK) >> Y_SHIFT;
            let z = (i & Z_MASK) >> Z_SHIFT;

            for side in VOXEL_SIDES {
                // TODO: Check if side is ocludded

                let side_idx = side as usize;

                for idx in VERTICES_INDICES[side_idx] {
                    let v = &VERTICES[idx];

                    vertices[side_idx].push([v[0] + x as f32, v[1] + y as f32, v[2] + z as f32]);
                }
            }
        }

        commands.entity(e).insert(ChunkVertices(vertices));
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

        let mut v: Vec<[f32; 3]> = vec![];
        for side in VOXEL_SIDES {
            let side_idx = side as usize;

            v.extend(&vertices.0[side_idx]);
        }
        
        let vertex_count = v.len();

        mesh.set_indices(Some(Indices::U32(compute_indices(vertex_count))));
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, v);

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
