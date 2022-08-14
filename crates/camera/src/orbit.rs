use bevy::{
    ecs::query::QuerySingleError,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};

use std::f32::consts::PI;

use bevy::ecs::schedule::ShouldRun;

/// Adds [`OrbitCameraPlugin`] resource and internals systems gated by [`is_active`] run criteria
/// grouped on [`CameraUpdate`] system set.
pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitCameraConfig>().add_system_set(
            SystemSet::new()
                .with_run_criteria(is_active)
                .with_system(target_moved)
                .with_system(settings_changed)
                .with_system(move_camera_keycode)
                .with_system(move_camera_mouse)
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
pub struct OrbitCamera;

/// Component used to tag which entity this camera will orbit around.
/// There can be only one Entity with this component.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct OrbitCameraTarget;

/// Key bindings used internal systems to orbit camera around target.
#[derive(Debug)]
pub struct KeyBindings {
    /// Orbit left key binding, defaults to [`KeyCode::Left`].
    pub left: KeyCode,

    /// Orbit right key binding, defaults to [`KeyCode::Right`].
    pub right: KeyCode,

    /// RotaOrbitte up key binding, defaults to [`KeyCode::Up`].
    pub up: KeyCode,

    /// Orbit down key binding, defaults to [`KeyCode::Down`].
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

/// Allows to configure [`OrbitCamera`] behavior.
#[derive(Debug)]
pub struct OrbitCameraConfig {
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

    /// Minium polar angle to keep when orbiting downwards.
    pub min_polar_angle: f32,

    /// Maxium polar angle to keep when orbiting downwards.
    pub max_polar_angle: f32,

    /// Rotation, in radius, around the azimuthal angle (up-down) of the target.
    pub azimuthal_angle: f32,

    /// Rotation speed in units when using keys.
    pub key_rotate_speed: f32,

    /// Zoom speed in units when using keys.
    pub key_zoom_speed: f32,

    /// Key bindings used by camera. See [`KeyBindings`] for more info.
    pub key_bindings: KeyBindings,

    /// Rotation speed in units when using mouse.
    pub mouse_rotate_speed: f32,

    /// Zoom speed in units when using mouse.
    pub mouse_zoom_speed: f32,
}

impl OrbitCameraConfig {
    fn apply_delta(&mut self, delta: Vec3) {
        self.azimuthal_angle += delta.x;
        self.polar_angle += delta.y;
        self.radial_distance += delta.z;

        self.polar_angle = self
            .polar_angle
            .clamp(self.min_polar_angle, self.max_polar_angle);

        self.radial_distance = self
            .radial_distance
            .clamp(self.min_distance, self.max_distance);
    }
}

impl Default for OrbitCameraConfig {
    fn default() -> Self {
        OrbitCameraConfig {
            active: false,

            radial_distance: 10.0,
            min_distance: 3.0,
            max_distance: 30.0,

            polar_angle: std::f32::consts::FRAC_PI_4,
            min_polar_angle: std::f32::consts::FRAC_PI_4,
            max_polar_angle: std::f32::consts::FRAC_PI_4,

            azimuthal_angle: 0.0,

            key_rotate_speed: PI / 5.0,
            key_zoom_speed: 5.0,
            key_bindings: KeyBindings::default(),

            mouse_rotate_speed: PI / 5.0,
            mouse_zoom_speed: 50.0,
        }
    }
}

/// Returns [`ShouldRun::Yes`] when [`OrbitCameraConfig::active`] is true.
pub fn is_active(config: Res<OrbitCameraConfig>) -> ShouldRun {
    if config.active {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

/// Calculates the spheric rotation around the target using [`OrbitCameraConfig`] settings.
///
/// This systems is guarded by [`is_active`] run criteria.
///
/// This does nothing if the [`Transform`] of an [`Entity`] with [`OrbitCameraTarget`] is not [`Changed`].
fn target_moved(
    config: Res<OrbitCameraConfig>,
    target: Query<
        &Transform,
        (
            With<OrbitCameraTarget>,
            Changed<Transform>,
            Without<OrbitCamera>,
        ),
    >,
    mut q: Query<&mut Transform, With<OrbitCamera>>,
) {
    let target = match target.get_single() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(_)) => {
            return;
        }
        Err(QuerySingleError::MultipleEntities(_)) => {
            panic!("Multiple orbit camera target detected.");
        }
    };

    if let Ok(mut camera_transform) = q.get_single_mut() {
        look_and_move_around(&mut camera_transform, target.translation, &config);
    }
}

/// Calculates the spheric rotation around the target using [`OrbitCameraConfig`] settings.
///
/// This systems is guarded by [`is_active`] run criteria.
///
/// This system does nothing if the [`OrbitCameraConfig`] is not [`Changed`].
fn settings_changed(
    config: Res<OrbitCameraConfig>,
    target: Query<&Transform, (With<OrbitCameraTarget>, Without<OrbitCamera>)>,
    mut q: Query<&mut Transform, With<OrbitCamera>>,
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
            panic!("Multiple orbit camera target detected.");
        }
    };

    if let Ok(mut camera_transform) = q.get_single_mut() {
        look_and_move_around(&mut camera_transform, target.translation, &config);
    }
}

fn look_and_move_around(
    camera_transform: &mut Transform,
    target: Vec3,
    config: &OrbitCameraConfig,
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

/// Move camera around using key binds in[`OrbitCameraConfig`] configuration settings.
/// This system is gated by [`is_active`] run criteria.
///
/// This system doesn't change the [`Transform`] directly, but instead, change spherical settings on [`OrbitCameraConfig`]
fn move_camera_keycode(
    input: Res<Input<KeyCode>>,
    time: Res<Time>,
    mut config: ResMut<OrbitCameraConfig>,
) {
    let mut delta = Vec3::ZERO;

    if input.pressed(config.key_bindings.right) {
        delta.x = -config.key_rotate_speed * time.delta_seconds();
    } else if input.pressed(config.key_bindings.left) {
        delta.x = config.key_rotate_speed * time.delta_seconds();
    }

    if input.pressed(config.key_bindings.up) {
        delta.y = config.key_rotate_speed * time.delta_seconds();
    } else if input.pressed(config.key_bindings.down) {
        delta.y = -config.key_rotate_speed * time.delta_seconds();
    }

    if input.pressed(config.key_bindings.zoom_in) {
        delta.z = -config.key_zoom_speed * time.delta_seconds();
    } else if input.pressed(config.key_bindings.zoom_out) {
        delta.z = config.key_zoom_speed * time.delta_seconds();
    }

    if delta != Vec3::ZERO {
        config.apply_delta(delta);
    }
}

/// Move camera around using mouse.
/// This system is gated by [`is_active`] run criteria.
///
/// This system doesn't change the [`Transform`] directly, but instead, change spherical settings on [`OrbitCameraConfig`]
fn move_camera_mouse(
    input: Res<Input<MouseButton>>,
    mut mouse_move: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    time: Res<Time>,
    mut config: ResMut<OrbitCameraConfig>,
) {
    let mut delta = Vec3::ZERO;

    if input.pressed(MouseButton::Right) {
        for evt in mouse_move.iter() {
            delta.x += evt.delta.x * time.delta_seconds() * config.mouse_rotate_speed;
            delta.y += evt.delta.y * time.delta_seconds() * config.mouse_rotate_speed;
        }
    }

    for evt in mouse_wheel.iter() {
        delta.z -= evt.y * time.delta_seconds() * config.mouse_zoom_speed;
    }

    if delta != Vec3::ZERO {
        config.apply_delta(delta);
    }
}
