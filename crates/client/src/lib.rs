use bevy::{
    camera::visibility::RenderLayers,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    platform::collections::HashMap,
    prelude::*,
};

#[cfg(not(feature = "ai"))]
use bevy::window::PresentMode;

#[cfg(feature = "ai")]
use bevy::{app::ScheduleRunnerPlugin, window::ExitCondition, winit::WinitPlugin};
use bundle::ChunkLocal;
use controller::{
    camera_controller::CameraControllerPlugin,
    character_controller::{CharacterController, CharacterControllerPlugin},
};
use material::ChunkMaterial;
use net::ServerConnection;
use projekto_camera::{
    CameraPlugin,
    first_person::{FirstPersonCamera, FirstPersonTarget},
    fly_by::FlyByCamera,
};
use projekto_core::{
    coords::Chunk,
    voxel::{self},
};
#[cfg(feature = "ai")]
use std::time::Duration;

#[cfg(feature = "ai")]
pub(crate) mod ai;

mod bundle;
mod controller;
#[cfg(feature = "debug-tools")]
pub mod debug;
mod material;
mod net;
mod set;

pub use set::PlayerLandscape;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        // This may cause problems later on. Ideally this setup should be done per image
        let default_plugins = DefaultPlugins
            .set(window_plugin())
            .set(ImagePlugin::default_nearest());

        #[cfg(feature = "ai")]
        let default_plugins = default_plugins.disable::<WinitPlugin>();

        app.add_plugins((
            default_plugins,
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .add_plugins((
            CameraPlugin,
            CameraControllerPlugin,
            CharacterControllerPlugin,
        ))
        .add_systems(Startup, (setup_mockup_scene, setup_hold_esc_to_exit))
        .add_systems(Update, hold_esc_to_exit);

        #[cfg(feature = "ai")]
        app.add_plugins((
            ScheduleRunnerPlugin::run_loop(Duration::from_secs(1)),
            ai::AiClientPlugin,
        ));

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
            .add_systems(PreStartup, load_assets);
    }
}

const ESC_HOLD_TIMEOUT: f32 = 0.2;

#[derive(Resource)]
struct EscHolding(f32);

fn setup_hold_esc_to_exit(mut commands: Commands) {
    commands.insert_resource(EscHolding(0.0));
}

fn hold_esc_to_exit(
    mut esc_holding: ResMut<EscHolding>,
    time: Res<Time>,
    input_keys: Res<ButtonInput<KeyCode>>,
    mut exit_writer: MessageWriter<bevy::app::AppExit>,
) {
    if input_keys.pressed(KeyCode::Escape) {
        esc_holding.0 += time.delta_secs();

        if esc_holding.0 >= ESC_HOLD_TIMEOUT {
            info!("Exiting app due to ESC holding...");
            exit_writer.write(bevy::app::AppExit::Success);
        }
    } else {
        esc_holding.0 = 0.0;
    }
}

fn camera_render_layers() -> RenderLayers {
    #[cfg(feature = "debug-tools")]
    {
        RenderLayers::from_layers(&[0, 1, 2])
    }
    #[cfg(not(feature = "debug-tools"))]
    {
        RenderLayers::from_layers(&[0, 1])
    }
}

fn first_person_render_layers() -> RenderLayers {
    #[cfg(feature = "debug-tools")]
    {
        RenderLayers::from_layers(&[0, 2])
    }
    #[cfg(not(feature = "debug-tools"))]
    {
        RenderLayers::from_layers(&[0])
    }
}

#[cfg(not(feature = "ai"))]
fn window_plugin() -> WindowPlugin {
    WindowPlugin {
        primary_window: Some(Window {
            present_mode: PresentMode::AutoNoVsync,
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(feature = "ai")]
fn window_plugin() -> WindowPlugin {
    WindowPlugin {
        primary_window: None,
        exit_condition: ExitCondition::DontExit,
        ..Default::default()
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
        camera_render_layers(),
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
                first_person_render_layers(),
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
