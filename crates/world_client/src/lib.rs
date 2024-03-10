use bevy::{ecs::query::QueryFilter, prelude::*, utils::HashMap};
use material::ChunkMaterial;
use net::ServerConnection;
use projekto_core::{
    chunk::Chunk,
    voxel::{self},
};
use projekto_server::{
    bundle::{ChunkLocal, ChunkVertex},
    proto::WorldClientChannel,
};

mod material;
mod net;
mod set;

pub use set::PlayerLandscape;

pub struct WorldClientPlugin;

impl Plugin for WorldClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .register_type::<ChunkMaterial>()
            .configure_sets(PreUpdate, WorldClientSet::ReceiveMessages)
            .configure_sets(Update, WorldClientSet::Meshing)
            .configure_sets(
                PostUpdate,
                WorldClientSet::SendInput.run_if(resource_exists::<ServerConnection>),
            )
            .add_plugins(MaterialPlugin::<ChunkMaterial>::default())
            .add_plugins((
                net::NetPlugin,
                set::ReceiveMessagesPlugin,
                set::MeshingPlugin,
                set::SendInputPlugin,
            ))
            .add_systems(PreStartup, load_assets)
            .add_systems(
                Update,
                (remove_unloaded_chunks.run_if(any_chunk::<Changed<ChunkVertex>>),),
            );
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum WorldClientSet {
    ReceiveMessages,
    ChunkManagement,
    LandscapeUpdate,
    Meshing,
    SendInput,
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
