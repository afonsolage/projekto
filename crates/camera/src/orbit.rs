use bevy::{
    ecs::query::QuerySingleError,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};

use std::f32::consts::PI;

/// Adds [`OrbitCameraPlugin`] resource and internals systems gated by [`is_active`] run criteria
/// grouped on [`CameraUpdate`] system set.
pub struct OrbitCameraPlugin;

impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitCameraConfig>().add_systems(
            Update,
            (
                target_moved,
                settings_changed,
                move_camera_keycode,
                move_camera_mouse,
            )
                .in_set(super::CameraUpdate)
                .run_if(is_active),
        );
    }
}

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

    /// Orbit up key binding, defaults to [`KeyCode::Up`].
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
            left: KeyCode::ArrowLeft,
            right: KeyCode::ArrowRight,
            up: KeyCode::ArrowUp,
            down: KeyCode::ArrowDown,
            zoom_in: KeyCode::PageUp,
            zoom_out: KeyCode::PageDown,
        }
    }
}

/// Allows to configure [`OrbitCamera`] behavior.
#[derive(Debug, Resource)]
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
            mouse_zoom_speed: 500.0,
        }
    }
}

/// Returns true when [`OrbitCameraConfig::active`] is true.
pub fn is_active(config: Res<OrbitCameraConfig>) -> bool {
    config.active
}

/// Calculates the spheric rotation around the target using [`OrbitCameraConfig`] settings.
///
/// This systems is guarded by [`is_active`] run criteria.
///
/// This does nothing if the [`Transform`] of an [`Entity`] with [`OrbitCameraTarget`] is not
/// [`Changed`].
#[allow(clippy::type_complexity)]
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
    let target = match target.single() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(_)) => {
            return;
        }
        Err(QuerySingleError::MultipleEntities(_)) => {
            panic!("Multiple orbit camera target detected.");
        }
    };

    if let Ok(mut camera_transform) = q.single_mut() {
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
    if !config.is_changed() {
        return;
    }

    let target = match target.single() {
        Ok(t) => t,
        Err(QuerySingleError::NoEntities(_)) => {
            return;
        }
        Err(QuerySingleError::MultipleEntities(_)) => {
            panic!("Multiple orbit camera target detected.");
        }
    };

    if let Ok(mut camera_transform) = q.single_mut() {
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
/// This system doesn't change the [`Transform`] directly, but instead, change spherical settings on
/// [`OrbitCameraConfig`]
fn move_camera_keycode(
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut config: ResMut<OrbitCameraConfig>,
) {
    let mut delta = Vec3::ZERO;

    if input.pressed(config.key_bindings.right) {
        delta.x = -config.key_rotate_speed * time.delta_secs();
    } else if input.pressed(config.key_bindings.left) {
        delta.x = config.key_rotate_speed * time.delta_secs();
    }

    if input.pressed(config.key_bindings.up) {
        delta.y = config.key_rotate_speed * time.delta_secs();
    } else if input.pressed(config.key_bindings.down) {
        delta.y = -config.key_rotate_speed * time.delta_secs();
    }

    if input.pressed(config.key_bindings.zoom_in) {
        delta.z = -config.key_zoom_speed * time.delta_secs();
    } else if input.pressed(config.key_bindings.zoom_out) {
        delta.z = config.key_zoom_speed * time.delta_secs();
    }

    if delta != Vec3::ZERO {
        config.apply_delta(delta);
    }
}

/// Move camera around using mouse.
/// This system is gated by [`is_active`] run criteria.
///
/// This system doesn't change the [`Transform`] directly, but instead, change spherical settings on
/// [`OrbitCameraConfig`]
fn move_camera_mouse(
    input: Res<ButtonInput<MouseButton>>,
    mut mouse_move: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    time: Res<Time>,
    mut config: ResMut<OrbitCameraConfig>,
) {
    let mut delta = Vec3::ZERO;

    if input.pressed(MouseButton::Right) {
        for evt in mouse_move.read() {
            delta.x += evt.delta.x * time.delta_secs() * config.mouse_rotate_speed;
            delta.y += evt.delta.y * time.delta_secs() * config.mouse_rotate_speed;
        }
    }

    for evt in mouse_wheel.read() {
        delta.z -= evt.y * time.delta_secs() * config.mouse_zoom_speed;
    }

    if delta != Vec3::ZERO {
        config.apply_delta(delta);
    }
}
