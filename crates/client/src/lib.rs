use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    ecs::query::QueryFilter,
    platform::collections::HashMap,
    prelude::*,
    render::view::RenderLayers,
    window::PresentMode,
};
use bundle::{ChunkLocal, ChunkVertex};
use controller::{
    camera_controller::CameraControllerPlugin,
    character_controller::{CharacterController, CharacterControllerPlugin},
};
use debug::DebugPlugin;
use material::ChunkMaterial;
use net::ServerConnection;
use projekto_camera::{
    first_person::{FirstPersonCamera, FirstPersonTarget},
    fly_by::FlyByCamera,
    CameraPlugin,
};
use projekto_core::{
    chunk::Chunk,
    voxel::{self},
};

mod bundle;
mod controller;
mod debug;
mod material;
mod net;
mod set;

pub use set::PlayerLandscape;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app
            // This may cause problems later on. Ideally this setup should be done per image
            .add_plugins((
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            present_mode: PresentMode::AutoNoVsync,
                            ..Default::default()
                        }),
                        ..Default::default()
                    })
                    .set(ImagePlugin::default_nearest()),
                LogDiagnosticsPlugin::default(),
                FrameTimeDiagnosticsPlugin::default(),
            ))
            .add_plugins((
                CameraPlugin,
                DebugPlugin,
                CameraControllerPlugin,
                CharacterControllerPlugin,
            ))
            .add_systems(Startup, setup_mockup_scene);

        // World setup
        app.init_resource::<ChunkMap>()
            .register_type::<ChunkMaterial>()
            .configure_sets(PreUpdate, ClientSet::ReceiveMessages)
            .configure_sets(Update, ClientSet::Meshing)
            .configure_sets(
                PostUpdate,
                ClientSet::SendInput.run_if(resource_exists::<ServerConnection>),
            )
            .add_plugins(MaterialPlugin::<ChunkMaterial>::default())
            .add_plugins((
                net::NetPlugin,
                set::ReceiveMessagesPlugin,
                set::MeshingPlugin,
                set::SendInputPlugin,
            ))
            .add_systems(Startup, setup_material)
            .add_systems(PreStartup, load_assets)
            .add_systems(
                Update,
                (remove_unloaded_chunks.run_if(any_chunk::<Changed<ChunkVertex>>),),
            );
    }
}

fn setup_mockup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(128.0, 256.0, 128.0).looking_at(Vec3::new(0.0, 128.0, 0.0), Vec3::Y),
        RenderLayers::from_layers(&[0, 1]),
        FlyByCamera,
        Name::new("FlyByCamera"),
    ));

    // character
    commands
        .spawn((
            Transform::from_xyz(2.0, 20.0, 7.0),
            Mesh3d(meshes.add(Capsule3d {
                radius: 0.25,
                half_length: 0.75,
            })),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.3, 0.3))),
            Name::new("Character"),
            CharacterController,
            FirstPersonTarget,
        ))
        .with_children(|p| {
            // Front indicator
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.05, 0.05, -0.5))),
                MeshMaterial3d(materials.add(Color::srgb(1.0, 1.0, 1.0))),
                RenderLayers::from_layers(&[1]),
            ));
            p.spawn((
                Camera3d::default(),
                Camera {
                    is_active: false,
                    ..Default::default()
                },
                Name::new("FirstPersonCamera"),
                FirstPersonCamera,
            ));
        });

    // X axis
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(3.0, 0.1, 0.1))),
        MeshMaterial3d(materials.add(Color::srgb(1.0, 0.3, 0.3))),
    ));

    // Y axis
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.1, 3.0, 0.1))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 1.0, 0.3))),
    ));

    // Z axis
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.1, 0.1, 3.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.3, 1.0))),
    ));

    commands.spawn((PointLight::default(), Transform::from_xyz(4.0, 8.0, 4.0)));
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum ClientSet {
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
    material: MeshMaterial3d<ChunkMaterial>,
    mesh: Mesh3d,
    transform: Transform,
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
