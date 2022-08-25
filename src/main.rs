//https://github.com/bevyengine/bevy/issues/4601
#![allow(clippy::forget_non_drop)]
#![feature(int_log)]
#![feature(test)]
#![feature(once_cell)]
#![feature(is_some_with)]

use bevy::{
    asset::AssetServerSettings, prelude::*, render::texture::ImageSettings, window::PresentMode,
};

#[cfg(feature = "inspector")]
use bevy_inspector_egui;

mod debug;
use camera_controller::CameraControllerPlugin;
use character_controller::{CharacterController, CharacterControllerPlugin};
use debug::DebugPlugin;

mod world;
use projekto_camera::{
    fly_by::FlyByCamera,
    orbit::{OrbitCamera, OrbitCameraTarget},
    CameraPlugin,
};
use world::{rendering::LandscapeCenter, terraformation::TerraformationCenter, WorldPlugin};

mod ui;
use ui::UiPlugin;

mod camera_controller;
mod character_controller;

fn main() {
    // env_logger::init();

    let mut app = App::new();

    app.insert_resource(WindowDescriptor {
        present_mode: PresentMode::AutoNoVsync,
        ..Default::default()
    })
    .insert_resource(Msaa { samples: 4 })
    // This may cause problems later on. Ideally this setup should be done per image
    .insert_resource(ImageSettings::default_nearest())
    .insert_resource(AssetServerSettings {
        watch_for_changes: true,
        ..default()
    })
    .add_plugins(DefaultPlugins)
    .add_plugin(DebugPlugin)
    .add_plugin(CameraPlugin)
    .add_plugin(WorldPlugin)
    .add_plugin(UiPlugin)
    .add_plugin(CameraControllerPlugin)
    .add_plugin(CharacterControllerPlugin)
    // .add_system_to_stage(CoreStage::PreUpdate, limit_fps)
    .add_startup_system(setup);

    #[cfg(feature = "inspector")]
    app.add_plugin(bevy_inspector_egui::WorldInspectorPlugin::new());

    app.run();
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
    commands
        .spawn_bundle(Camera3dBundle::default())
        .insert(OrbitCamera)
        .insert(FlyByCamera)
        .insert(Transform::from_xyz(0.0, 22.0, 5.0).looking_at(Vec3::new(2.0, 20.0, 7.0), Vec3::Y))
        .insert(Name::new("Main Camera"));

    // focus
    commands
        .spawn_bundle(PbrBundle {
            transform: Transform::from_xyz(2.0, 20.0, 7.0),
            mesh: meshes.add(Mesh::from(shape::Capsule {
                radius: 0.25,
                depth: 1.5,
                ..default()
            })),
            material: materials.add(Color::rgb(0.3, 0.3, 0.3).into()),
            ..Default::default()
        })
        .insert(Name::new("Character"))
        .insert(TerraformationCenter)
        .insert(LandscapeCenter)
        .insert(OrbitCameraTarget)
        .insert(CharacterController)
        .with_children(|p| {
            p.spawn_bundle(PbrBundle {
                mesh: meshes.add(Mesh::from(shape::Box {
                    min_x: 0.0,
                    max_x: 0.05,
                    min_y: 0.0,
                    max_y: 0.05,
                    min_z: 0.0,
                    max_z: -0.5,
                })),
                material: materials.add(Color::rgb(1.0, 1.0, 1.0).into()),
                ..Default::default()
            });
        });

    //X axis
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Box {
            min_x: 0.0,
            max_x: 3.0,
            min_y: 0.0,
            max_y: 0.1,
            min_z: 0.0,
            max_z: 0.1,
        })),
        material: materials.add(Color::rgb(1.0, 0.3, 0.3).into()),
        ..Default::default()
    });

    //Y axis
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Box {
            min_x: 0.0,
            max_x: 0.1,
            min_y: 0.0,
            max_y: 3.0,
            min_z: 0.0,
            max_z: 0.1,
        })),
        material: materials.add(Color::rgb(0.3, 1.0, 0.3).into()),
        ..Default::default()
    });

    //Z axis
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Box {
            min_x: 0.0,
            max_x: 0.1,
            min_y: 0.0,
            max_y: 0.1,
            min_z: 0.0,
            max_z: 3.0,
        })),
        material: materials.add(Color::rgb(0.3, 0.3, 1.0).into()),
        ..Default::default()
    });

    commands.spawn_bundle(PointLightBundle {
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..Default::default()
    });
}
