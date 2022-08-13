use bevy::{prelude::*, window};
use bevy_inspector_egui::WorldInspectorPlugin;
use projekto_camera::{
    orbit::{OrbitCamera, OrbitCameraTarget, OrbitCameraConfig},
    CameraPlugin,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(WorldInspectorPlugin::default())
        .add_plugin(CameraPlugin)
        .add_system(window::close_on_esc)
        .add_system(move_target)
        .add_startup_system(setup_environment)
        .run();
}

fn move_target(
    input: Res<Input<KeyCode>>,
    time: Res<Time>,
    mut q: Query<&mut Transform, With<OrbitCameraTarget>>,
) {
    let input_vec = calc_input_vector(&input);
    if input_vec == Vec3::ZERO {
        return;
    }

    if let Ok(mut transform) = q.get_single_mut() {
        transform.translation += input_vec * time.delta_seconds() * 5.0;
    }
}

fn setup_environment(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut config: ResMut<OrbitCameraConfig>,
) {
    // camera
    commands
        .spawn_bundle(Camera3dBundle { ..default() })
        .insert(OrbitCamera)
        // .insert(Transform::from_xyz(5.0, 20.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y))
        ;

    // target
    commands
        .spawn_bundle(PbrBundle {
            transform: Transform::from_xyz(3.0, 5.0, 3.0),
            mesh: meshes.add(Mesh::from(shape::Capsule {
                radius: 0.25,
                depth: 1.5,
                ..default()
            })),
            material: materials.add(Color::rgb(0.3, 0.3, 0.3).into()),
            ..Default::default()
        })
        .insert(OrbitCameraTarget)
        .insert(Name::new("Target"));

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

    config.active = true;
}

fn calc_input_vector(input: &Res<Input<KeyCode>>) -> Vec3 {
    let mut res = Vec3::ZERO;

    if input.pressed(KeyCode::W) {
        res.z += 1.0
    }

    if input.pressed(KeyCode::S) {
        res.z -= 1.0
    }

    if input.pressed(KeyCode::D) {
        res.x += 1.0
    }

    if input.pressed(KeyCode::A) {
        res.x -= 1.0
    }

    if input.pressed(KeyCode::Space) {
        res.y += 1.0
    }

    if input.pressed(KeyCode::LControl) {
        res.y -= 1.0
    }

    res
}
