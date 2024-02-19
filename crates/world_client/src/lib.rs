use bevy::{
    ecs::query::QueryFilter,
    prelude::*,
    render::{mesh::Indices, render_asset::RenderAssetUsages, render_resource::PrimitiveTopology},
    utils::HashMap,
};
use material::ChunkMaterial;
use projekto_core::{
    chunk::{self, Chunk},
    voxel::{self, Vertex},
};
use projekto_world_server::bundle::{ChunkLocal, ChunkVertex};

mod material;

pub struct WorldClientPlugin;

impl Plugin for WorldClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .register_type::<ChunkMaterial>()
            .add_plugins(MaterialPlugin::<ChunkMaterial>::default())
            .add_systems(PreStartup, load_assets)
            .add_systems(Startup, setup_material)
            .add_systems(
                Update,
                (
                    update_chunk_mesh.run_if(any_chunk::<Changed<ChunkVertex>>),
                    remove_unloaded_chunks.run_if(any_chunk::<Changed<ChunkVertex>>),
                ),
            );
    }
}

#[derive(Resource, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkMap(HashMap<Chunk, Entity>);

#[derive(Resource, Debug, Clone)]
pub struct ChunkMaterialHandle(pub Handle<ChunkMaterial>);

#[derive(Bundle, Default)]
struct ChunkBundle {
    chunk: ChunkLocal,
    mesh: MaterialMeshBundle<ChunkMaterial>,
}

fn any_chunk<T: QueryFilter>(q_changed_chunks: Query<(), (T, With<ChunkLocal>)>) -> bool {
    !q_changed_chunks.is_empty()
}

#[derive(Debug, Resource)]
pub struct KindsAtlasRes {
    pub atlas: Handle<Image>,
}

fn load_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    let kinds_path = format!("{}{}", env!("ASSETS_PATH"), "/voxels/kind.ron");
    let descs = voxel::KindsDescs::init(kinds_path);

    let atlas = asset_server.load(&descs.atlas_path);

    commands.insert_resource(KindsAtlasRes { atlas });
}

fn setup_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    kinds_res: Res<KindsAtlasRes>,
) {
    let material = materials.add(ChunkMaterial {
        texture: kinds_res.atlas.clone(),
        tile_texture_size: 1.0 / voxel::KindsDescs::get().count_tiles() as f32,
        show_back_faces: false,
    });

    commands.insert_resource(ChunkMaterialHandle(material));
}

fn remove_unloaded_chunks(
    mut commands: Commands,
    mut map: ResMut<ChunkMap>,
    q_vertex: Query<&ChunkLocal, With<ChunkVertex>>,
) {
    let server_chunks = q_vertex.iter().map(|l| **l).collect::<Vec<_>>();

    map.retain(|chunk, entity| {
        let retain = server_chunks.contains(chunk);
        if !retain {
            trace!("[remove_unloaded_chunks] despawning chunk [{}]", chunk);
            commands.entity(*entity).despawn();
        }
        retain
    });
}

fn update_chunk_mesh(
    mut commands: Commands,
    mut map: ResMut<ChunkMap>,
    q_vertex: Query<(&ChunkLocal, &ChunkVertex), Changed<ChunkVertex>>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<ChunkMaterialHandle>,
) {
    let mut spawned = 0;
    let mut updated = 0;
    for (chunk, vertex) in &q_vertex {
        let mesh_handler = meshes.add(generate_mesh(vertex));
        if let Some(&entity) = map.get(&**chunk) {
            commands.entity(entity).insert(mesh_handler);
            updated += 1;
        } else {
            trace!("Spawning chunk!");
            let entity = commands
                .spawn(ChunkBundle {
                    chunk: *chunk,
                    mesh: MaterialMeshBundle {
                        mesh: mesh_handler,
                        transform: Transform::from_translation(chunk::to_world(**chunk)),
                        material: material.0.clone(),
                        ..Default::default()
                    },
                })
                .insert(Name::new(format!("Client Chunk {}", **chunk)))
                .id();
            map.insert(**chunk, entity);
            spawned += 1;
        }
    }
    let count = spawned + updated;
    trace!(
        "[update_chunk_mesh] {count} chunks mesh updated. {spawned} spawned, {updated} updated."
    );
}

fn generate_mesh(vertices: &Vec<Vertex>) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    let mut positions: Vec<[f32; 3]> = vec![];
    let mut normals: Vec<[f32; 3]> = vec![];
    let mut uvs: Vec<[f32; 2]> = vec![];
    let mut tile_coord_start: Vec<[f32; 2]> = vec![];
    let mut lights: Vec<[f32; 3]> = vec![];

    let vertex_count = vertices.len();

    for vertex in vertices {
        positions.push(vertex.position.into());
        normals.push(vertex.normal.into());
        uvs.push(vertex.uv.into());
        tile_coord_start.push(vertex.tile_coord_start.into());
        lights.push(vertex.light.into());
    }

    mesh.insert_indices(Indices::U32(compute_indices(vertex_count)));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_TILE_COORD_START, tile_coord_start);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_LIGHT, lights);
    mesh
}

/// Computes indices of a triangle list mesh.
///
/// This function assumes 4 vertices per face, 3 indices per triangles and all vertices are placed
/// in CCW order.
///
/// It generates indices in the following order: _*0 1 2 2 3 0*_ where 0 is the first vertice and 3
/// is the last one
///
/// Returns** a list of indices in the CCW order
pub fn compute_indices(vertex_count: usize) -> Vec<u32> {
    // Each 4 vertex is a voxel face and each voxel face has 6 indices, so we can multiply the
    // vertex count by 1.5
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
