use bevy::prelude::*;
use projekto_camera::{
    orbit::{OrbitCamera, OrbitCameraConfig, OrbitCameraTarget},
    CameraPlugin,
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CameraPlugin))
        .add_systems(Update, move_target)
        .add_systems(Startup, setup_environment)
        .run();
}

fn move_target(
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<&mut Transform, With<OrbitCameraTarget>>,
) {
    let input_vec = calc_input_vector(&input);
    if input_vec == Vec3::ZERO {
        return;
    }

    if let Ok(mut transform) = q.single_mut() {
        transform.translation += input_vec * time.delta_secs() * 5.0;
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
        .spawn(Camera3d::default())
        .insert(OrbitCamera)
        // .insert(Transform::from_xyz(5.0, 20.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y))
        ;

    // target
    commands
        .spawn((
            Transform::from_xyz(3.0, 5.0, 3.0),
            Mesh3d(meshes.add(Capsule3d {
                radius: 0.25,
                half_length: 0.75,
            })),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.3, 0.3))),
        ))
        .insert(OrbitCameraTarget)
        .insert(Name::new("Target"));

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

    config.active = true;
}

fn calc_input_vector(input: &Res<ButtonInput<KeyCode>>) -> Vec3 {
    let mut res = Vec3::ZERO;

    if input.pressed(KeyCode::KeyW) {
        res.z += 1.0;
    }

    if input.pressed(KeyCode::KeyS) {
        res.z -= 1.0;
    }

    if input.pressed(KeyCode::KeyD) {
        res.x += 1.0;
    }

    if input.pressed(KeyCode::KeyA) {
        res.x -= 1.0;
    }

    if input.pressed(KeyCode::Space) {
        res.y += 1.0;
    }

    if input.pressed(KeyCode::ControlLeft) {
        res.y -= 1.0;
    }

    res
}
