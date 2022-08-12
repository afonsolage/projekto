use bevy::{prelude::*, window::close_on_esc};
use bevy_inspector_egui::WorldInspectorPlugin;
use projekto_camera::{CameraPlugin, MainCamera};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(WorldInspectorPlugin::default())
        .add_plugin(CameraPlugin)
        .add_system(close_on_esc)
        .add_startup_system(setup_environment)
        .run();
}

fn setup_environment(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // camera
    commands
        .spawn_bundle(Camera3dBundle { ..default() })
        .insert(MainCamera)
        .insert(Transform::from_xyz(5.0, 20.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y));

    // target
    commands.spawn_bundle(PbrBundle {
        transform: Transform::from_xyz(0.0, 5.0, 0.0),
        mesh: meshes.add(Mesh::from(shape::Capsule {
            radius: 0.25,
            depth: 1.5,
            ..default()
        })),
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
