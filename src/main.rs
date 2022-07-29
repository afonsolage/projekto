//https://github.com/bevyengine/bevy/issues/4601
#![allow(clippy::forget_non_drop)]
#![feature(int_log)]
#![feature(test)]
#![feature(let_chains)]

use bevy::{prelude::*, window::PresentMode};

#[cfg(feature = "dev")]
use bevy_inspector_egui;

#[macro_use]
mod macros;

mod fly_by_camera;
use fly_by_camera::FlyByCameraPlugin;

mod debug;
use debug::DebugPlugin;

mod world;
use world::WorldPlugin;

mod ui;
use ui::UiPlugin;

fn main() {
    // env_logger::init();

    let mut app = App::new();

    app.insert_resource(WindowDescriptor {
        present_mode: PresentMode::Fifo,
        ..Default::default()
    })
    .insert_resource(Msaa { samples: 4 })
    .add_plugins(DefaultPlugins)
    .add_plugin(DebugPlugin)
    .add_plugin(FlyByCameraPlugin)
    .add_plugin(WorldPlugin)
    .add_plugin(UiPlugin)
    .add_startup_system(setup);

    #[cfg(feature = "dev")]
    app.add_plugin(bevy_inspector_egui::WorldInspectorPlugin::new());

    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // cube
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Cube { size: 0.1 })),
        material: materials.add(Color::rgb(0.3, 0.3, 0.3).into()),
        ..Default::default()
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
