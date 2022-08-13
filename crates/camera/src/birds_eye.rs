use bevy::{ecs::query::QuerySingleError, prelude::*};

use std::f32::consts::PI;

use bevy::ecs::schedule::ShouldRun;

use super::MainCamera;

pub struct BirdsEyeCameraPlugin;

#[derive(SystemLabel)]
pub struct BirdsEyeCameraUpdate;

impl Plugin for BirdsEyeCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup).add_system_set(
            SystemSet::new()
                .with_run_criteria(is_active)
                .with_system(target_moved)
                .with_system(settings_changed)
                .with_system(move_camera)
                .label(BirdsEyeCameraUpdate),
        );
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct BirdsEyeCameraTarget;

#[derive(Debug)]
pub struct BirdsEyeCameraConfig {
    pub radial_distance: f32,
    pub polar_angle: f32,     // Y
    pub azimuthal_angle: f32, // X
    pub rotate_speed: f32,
    pub zoom_speed: f32,
    pub active: bool,
}

pub fn is_active(config: Res<BirdsEyeCameraConfig>) -> ShouldRun {
    if config.active {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

fn setup(mut commands: Commands) {
    commands.insert_resource(BirdsEyeCameraConfig {
        radial_distance: 10.0,
        polar_angle: 0.0,     // Up-Down
        azimuthal_angle: 0.0, // Left-Right
        rotate_speed: PI / 5.0,
        zoom_speed: 5.0,
        active: true,
    });
}

fn target_moved(
    config: Res<BirdsEyeCameraConfig>,
    target: Query<
        &Transform,
        (
            With<BirdsEyeCameraTarget>,
            Changed<Transform>,
            Without<MainCamera>,
        ),
    >,
    mut q: Query<&mut Transform, With<MainCamera>>,
) {
    let target = match target.get_single() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(msg)) => {
            trace!(msg);
            return;
        }
        Err(QuerySingleError::MultipleEntities(_)) => {
            panic!("Multiple birds eye camera target detected.");
        }
    };

    if let Ok(mut camera_transform) = q.get_single_mut() {
        look_and_move_around(&mut camera_transform, target.translation, &config);
    }
}

fn settings_changed(
    config: Res<BirdsEyeCameraConfig>,
    target: Query<&Transform, (With<BirdsEyeCameraTarget>, Without<MainCamera>)>,
    mut q: Query<&mut Transform, With<MainCamera>>,
) {
    if config.is_changed() == false {
        return;
    }

    let target = match target.get_single() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(msg)) => {
            trace!(msg);
            return;
        }
        Err(QuerySingleError::MultipleEntities(_)) => {
            panic!("Multiple birds eye camera target detected.");
        }
    };

    if let Ok(mut camera_transform) = q.get_single_mut() {
        look_and_move_around(&mut camera_transform, target.translation, &config);
    }
}

fn look_and_move_around(
    camera_transform: &mut Transform,
    target: Vec3,
    config: &BirdsEyeCameraConfig,
) {
    camera_transform.look_at(target, Vec3::Y);

    camera_transform.translation = spherical_to_cartesian(
        config.radial_distance,
        config.polar_angle,
        config.azimuthal_angle,
        target,
    );
}

fn spherical_to_cartesian(radius: f32, polar: f32, azimuth: f32, center: Vec3) -> Vec3 {
    let polar_cos = radius * polar.cos();

    Vec3::new(
        polar_cos * azimuth.cos(),
        radius * polar.sin(),
        polar_cos * azimuth.sin(),
    ) + center
}

fn move_camera(
    input: Res<Input<KeyCode>>,
    time: Res<Time>,
    mut config: ResMut<BirdsEyeCameraConfig>,
) {
    let mut delta = Vec3::ZERO;

    if input.pressed(KeyCode::Right) {
        delta.x = -config.rotate_speed * time.delta_seconds();
    } else if input.pressed(KeyCode::Left) {
        delta.x = config.rotate_speed * time.delta_seconds();
    }

    if input.pressed(KeyCode::Up) {
        delta.y = config.rotate_speed * time.delta_seconds();
    } else if input.pressed(KeyCode::Down) {
        delta.y = -config.rotate_speed * time.delta_seconds();
    }

    if input.pressed(KeyCode::PageUp) {
        delta.z = -config.zoom_speed * time.delta_seconds();
    } else if input.pressed(KeyCode::PageDown) {
        delta.z = config.zoom_speed * time.delta_seconds();
    }

    if delta == Vec3::ZERO {
        return;
    }

    config.azimuthal_angle += delta.x;
    config.polar_angle += delta.y;
    config.radial_distance += delta.z;

    use std::f32::consts;

    // config.azimuthal_angle = config.azimuthal_angle.clamp(0.0, consts::TAU);
    config.polar_angle = config.polar_angle.clamp(
        0.0 + consts::FRAC_PI_8,
        consts::FRAC_PI_2 - consts::FRAC_PI_8,
    );
    config.radial_distance = config.radial_distance.clamp(1.0, 30.0);
}
