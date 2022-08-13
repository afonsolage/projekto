use bevy::{ecs::query::QuerySingleError, prelude::*};

use std::f32::consts::PI;

use bevy::ecs::schedule::ShouldRun;

/// Adds [`BirdsEyeCameraPlugin`] resource and internals systems gated by [`is_active`] run criteria
/// grouped on [`CameraUpdate`] system set.
pub struct BirdsEyeCameraPlugin;

impl Plugin for BirdsEyeCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BirdsEyeCameraConfig>().add_system_set(
            SystemSet::new()
                .with_run_criteria(is_active)
                .with_system(target_moved)
                .with_system(settings_changed)
                .with_system(move_camera)
                .label(CameraUpdate),
        );
    }
}

/// [`SystemLabel`] used by internals systems.
#[derive(SystemLabel)]
pub struct CameraUpdate;

/// Component used to tag entity camera.
/// There can be only one Entity with this component.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct BirdsEyeCamera;

/// Component used to tag which entity this camera will orbit around.
/// There can be only one Entity with this component.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct BirdsEyeCameraTarget;

/// Key bindings used internal systems to rotate camera around target.
#[derive(Debug)]
pub struct KeyBindings {
    /// Rotate left key binding, defaults to [`KeyCode::Left`].
    pub left: KeyCode,

    /// Rotate right key binding, defaults to [`KeyCode::Right`].
    pub right: KeyCode,

    /// Rotate up key binding, defaults to [`KeyCode::Up`].
    pub up: KeyCode,

    /// Rotate down key binding, defaults to [`KeyCode::Down`].
    pub down: KeyCode,

    /// Zoom into the target key binding, defaults to [`KeyCode::PageUp`].
    pub zoom_in: KeyCode,

    /// Zoom out of the target key binding, defaults to [`KeyCode::PageDown`].
    pub zoom_out: KeyCode,
}

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

/// Allows to configure [`BirdsEyeCamera`] behavior.
#[derive(Debug)]
pub struct BirdsEyeCameraConfig {
    /// Enable or disable internal systems. This flag is used by [`is_active`] run criteria.
    pub active: bool,

    /// Distance, in radius, to keep from the target.
    /// Higher values will make camera orbit closer to the target.
    pub radial_distance: f32,

    /// Minimum distance to keep from the target when zooming in.
    pub min_distance: f32,

    /// Maximum distance to keep from the target when zooming in.
    pub max_distance: f32,

    /// Rotation, in radius, around the polar angle (left-right) of the target.
    pub polar_angle: f32,

    /// Rotation, in radius, around the azimuthal angle (up-down) of the target.
    pub azimuthal_angle: f32,

    /// Rotation speed in units.
    pub rotate_speed: f32,

    /// Zoom speed in units.
    pub zoom_speed: f32,

    /// Key bindings used by camera. See [`KeyBindings`] for more info.
    pub bindings: KeyBindings,
}

impl Default for BirdsEyeCameraConfig {
    fn default() -> Self {
        BirdsEyeCameraConfig {
            active: false,

            radial_distance: 10.0,
            min_distance: 3.0,
            max_distance: 30.0,

            polar_angle: std::f32::consts::FRAC_PI_6,
            azimuthal_angle: 0.0,

            rotate_speed: PI / 5.0,
            zoom_speed: 5.0,

            bindings: KeyBindings::default(),
        }
    }
}

/// Returns [`ShouldRun::Yes`] when [`BirdsEyeCameraConfig::active`] is true.
pub fn is_active(config: Res<BirdsEyeCameraConfig>) -> ShouldRun {
    if config.active {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

/// Calculates the spheric rotation around the target using [`BirdsEyeCameraConfig`] settings.
///
/// This systems is guarded by [`is_active`] run criteria.
///
/// This does nothing if the [`Transform`] of an [`Entity`] with [`BirdsEyeCameraTarget`] is not [`Changed`].
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

/// Calculates the spheric rotation around the target using [`BirdsEyeCameraConfig`] settings.
///
/// This systems is guarded by [`is_active`] run criteria.
///
/// This system does nothing if the [`BirdsEyeCameraConfig`] is not [`Changed`].
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

/// Move camera around using [`BirdsEyeCameraConfig`] configuration settings.
/// This system is gated by [`is_active`] run criteria.
///
/// This system doesn't change the [`Transform`] directly, but instead, change spherical settings on [`BirdsEyeCameraConfig`]
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
