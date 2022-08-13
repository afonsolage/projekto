use bevy::{ecs::query::QuerySingleError, prelude::*};

use std::f32::consts::PI;

use bevy::ecs::schedule::ShouldRun;

pub struct BirdsEyeCameraPlugin;

#[derive(SystemLabel)]
pub struct CameraUpdate;

impl Plugin for BirdsEyeCameraPlugin {
    fn build(&self, app: &mut App) {
        let camera_system_set = SystemSet::new()
            .with_run_criteria(is_active)
            .with_system(target_moved)
            .with_system(settings_changed)
            .label(CameraUpdate);

        #[cfg(feature = "birdseye_controls")]
        let camera_system_set = camera_system_set.with_system(move_camera);

        app.add_startup_system(setup)
            .add_system_set(camera_system_set);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct BirdsEyeCamera;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct BirdsEyeCameraTarget;

#[cfg(feature = "birdseye_controls")]
#[derive(Debug)]
pub struct KeyBindings {
    pub left: KeyCode,
    pub right: KeyCode,
    pub up: KeyCode,
    pub down: KeyCode,
    pub zoom_in: KeyCode,
    pub zoom_out: KeyCode,
}

#[cfg(feature = "birdseye_controls")]
impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            left: KeyCode::Left,
            right: KeyCode::Right,
            up: KeyCode::Up,
            down: KeyCode::Down,
            zoom_in: KeyCode::PageUp,
            zoom_out: KeyCode::PageDown,
        }
    }
}

#[derive(Debug)]
pub struct BirdsEyeCameraConfig {
    pub active: bool,

    pub radial_distance: f32,
    pub min_distance: f32,
    pub max_distance: f32,

    pub polar_angle: f32,
    pub azimuthal_angle: f32,

    pub rotate_speed: f32,
    pub zoom_speed: f32,

    #[cfg(feature = "birdseye_controls")]
    pub bindings: KeyBindings,
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
        active: false,

        radial_distance: 10.0,
        min_distance: 3.0,
        max_distance: 30.0,

        polar_angle: std::f32::consts::FRAC_PI_6,
        azimuthal_angle: 0.0,

        rotate_speed: PI / 5.0,
        zoom_speed: 5.0,

        #[cfg(feature = "birdseye_controls")]
        bindings: KeyBindings::default(),
    });
}

fn target_moved(
    config: Res<BirdsEyeCameraConfig>,
    target: Query<
        &Transform,
        (
            With<BirdsEyeCameraTarget>,
            Changed<Transform>,
            Without<BirdsEyeCamera>,
        ),
    >,
    mut q: Query<&mut Transform, With<BirdsEyeCamera>>,
) {
    let target = match target.get_single() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(_)) => {
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
    target: Query<&Transform, (With<BirdsEyeCameraTarget>, Without<BirdsEyeCamera>)>,
    mut q: Query<&mut Transform, With<BirdsEyeCamera>>,
) {
    if config.is_changed() == false {
        return;
    }

    let target = match target.get_single() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(_)) => {
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
    camera_transform.translation = spherical_to_cartesian(
        config.radial_distance,
        config.polar_angle,
        config.azimuthal_angle,
        target,
    );

    camera_transform.look_at(target, Vec3::Y);
}

fn spherical_to_cartesian(radius: f32, polar: f32, azimuth: f32, center: Vec3) -> Vec3 {
    let polar_cos = radius * polar.cos();

    Vec3::new(
        polar_cos * azimuth.cos(),
        radius * polar.sin(),
        polar_cos * azimuth.sin(),
    ) + center
}

#[cfg(feature = "birdseye_controls")]
fn move_camera(
    input: Res<Input<KeyCode>>,
    time: Res<Time>,
    mut config: ResMut<BirdsEyeCameraConfig>,
) {
    let mut delta = Vec3::ZERO;

    if input.pressed(config.bindings.right) {
        delta.x = -config.rotate_speed * time.delta_seconds();
    } else if input.pressed(config.bindings.left) {
        delta.x = config.rotate_speed * time.delta_seconds();
    }

    if input.pressed(config.bindings.up) {
        delta.y = config.rotate_speed * time.delta_seconds();
    } else if input.pressed(config.bindings.down) {
        delta.y = -config.rotate_speed * time.delta_seconds();
    }

    if input.pressed(config.bindings.zoom_in) {
        delta.z = -config.zoom_speed * time.delta_seconds();
    } else if input.pressed(config.bindings.zoom_out) {
        delta.z = config.zoom_speed * time.delta_seconds();
    }

    if delta == Vec3::ZERO {
        return;
    }

    config.azimuthal_angle += delta.x;
    config.polar_angle += delta.y;
    config.radial_distance += delta.z;

    use std::f32::consts;

    config.polar_angle = config.polar_angle.clamp(
        0.0 + consts::FRAC_PI_8,
        consts::FRAC_PI_2 - consts::FRAC_PI_8,
    );

    config.radial_distance = config
        .radial_distance
        .clamp(config.min_distance, config.max_distance);
}
