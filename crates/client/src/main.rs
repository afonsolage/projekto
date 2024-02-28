#![allow(clippy::type_complexity)]
#![feature(test)]

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::view::RenderLayers,
    window::PresentMode,
};

mod debug;
use camera_controller::CameraControllerPlugin;
use character_controller::{CharacterController, CharacterControllerPlugin};
use debug::DebugPlugin;

use projekto_camera::{
    first_person::{FirstPersonCamera, FirstPersonTarget},
    fly_by::FlyByCamera,
    CameraPlugin,
};
use projekto_world_client::WorldClientPlugin;

// mod ui;
// use ui::UiPlugin;

mod camera_controller;
mod character_controller;

fn main() {
    let mut app = App::new();

    app.insert_resource(Msaa::Sample4)
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
            FrameTimeDiagnosticsPlugin,
        ))
        .add_plugins((
            DebugPlugin,
            CameraPlugin,
            CameraControllerPlugin,
            CharacterControllerPlugin,
            WorldClientPlugin,
        ))
        // .add_system_to_stage(CoreStage::PreUpdate, limit_fps)
        .add_systems(Startup, setup_mockup_scene);

    app.run();
}

fn setup_mockup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(128.0, 256.0, 128.0)
                .looking_at(Vec3::new(0.0, 128.0, 0.0), Vec3::Y),
            ..Default::default()
        },
        RenderLayers::from_layers(&[0, 1]),
        FlyByCamera,
        Name::new("FlyByCamera"),
    ));

    // character
    commands
        .spawn((
            PbrBundle {
                transform: Transform::from_xyz(2.0, 20.0, 7.0),
                mesh: meshes.add(Capsule3d {
                    radius: 0.25,
                    half_length: 0.75,
                }),
                material: materials.add(Color::rgb(0.3, 0.3, 0.3)),
                ..Default::default()
            },
            Name::new("Character"),
            CharacterController,
            FirstPersonTarget,
        ))
        .with_children(|p| {
            // Front indicator
            p.spawn((
                PbrBundle {
                    mesh: meshes.add(Cuboid::new(0.05, 0.05, -0.5)),
                    material: materials.add(Color::rgb(1.0, 1.0, 1.0)),
                    ..Default::default()
                },
                RenderLayers::from_layers(&[1]),
            ));
            p.spawn((
                Camera3dBundle {
                    camera: Camera {
                        is_active: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Name::new("FirstPersonCamera"),
                FirstPersonCamera,
            ));
        });

    // X axis
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(3.0, 0.1, 0.1)),
        material: materials.add(Color::rgb(1.0, 0.3, 0.3)),
        ..Default::default()
    });

    // Y axis
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(0.1, 3.0, 0.1)),
        material: materials.add(Color::rgb(0.3, 1.0, 0.3)),
        ..Default::default()
    });

    // Z axis
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(0.1, 0.1, 3.0)),
        material: materials.add(Color::rgb(0.3, 0.3, 1.0)),
        ..Default::default()
    });

    commands.spawn(PointLightBundle {
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..Default::default()
    });
}
