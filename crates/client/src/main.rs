#![allow(clippy::type_complexity)]
#![feature(test)]

use bevy::{prelude::*, render::view::RenderLayers, window::PresentMode};

mod debug;
use camera_controller::CameraControllerPlugin;
use character_controller::{CharacterController, CharacterControllerPlugin};
use debug::DebugPlugin;

use projekto_camera::{
    first_person::{FirstPersonCamera, FirstPersonTarget},
    fly_by::FlyByCamera,
    CameraPlugin,
};
use projekto_core::chunk::Chunk;
use projekto_world_client::WorldClientPlugin;
use projekto_world_server::{app::RunAsync, set::Landscape};

// mod ui;
// use ui::UiPlugin;

mod camera_controller;
mod character_controller;

fn main() {
    projekto_world_server::app::create().run_async();

    let mut app = App::new();

    app.insert_resource(Msaa::Sample4)
        // This may cause problems later on. Ideally this setup should be done per image
        .add_plugins((DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    present_mode: PresentMode::AutoNoVsync,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .set(ImagePlugin::default_nearest()),))
        .add_plugins((
            DebugPlugin,
            CameraPlugin,
            CameraControllerPlugin,
            CharacterControllerPlugin,
            WorldClientPlugin,
        ))
        .insert_resource(Landscape {
            radius: 1,
            ..Default::default()
        })
        .register_type::<Landscape>()
        // .add_system_to_stage(CoreStage::PreUpdate, limit_fps)
        .add_systems(Update, update_landscape_center)
        .add_systems(Startup, setup);

    app.run();
}

fn update_landscape_center(
    mut landscape: ResMut<Landscape>,
    character: Query<&Transform, With<CharacterController>>,
) {
    let pos: Chunk = character.single().translation.into();
    let center: Chunk = landscape.center.into();

    if pos != center {
        landscape.center = pos.into();
    }
}

// fn limit_fps(time: Res<Time>) {
//     let target_fps = 60.0f32;
//     let frame_time = target_fps.recip();

//     let sleep = frame_time - time.delta_seconds();
//     if sleep > f32::EPSILON {
//         std::thread::sleep(std::time::Duration::from_secs_f32(sleep));
//     }
// }

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 22.0, 5.0)
                .looking_at(Vec3::new(2.0, 20.0, 7.0), Vec3::Y),
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