use std::f32::consts::PI;

use bevy::prelude::*;

use bevy::input::mouse::MouseMotion;

/// Adds [`FlyByCameraConfig`] resource and internals systems gated by [`is_active`] run criteria
/// grouped on [`CameraUpdate`] system set.
pub struct FlyByCameraPlugin;

impl Plugin for FlyByCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlyByCameraConfig>().add_systems(
            Update,
            (move_camera, rotate_camera)
                .in_set(super::CameraUpdate)
                .run_if(is_active),
        );
    }
}

/// Component used to tag entity camera.
/// There can be only one Entity with this component.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct FlyByCamera;

/// Key bindings used internal systems to move and rotate camera.
#[derive(Debug)]
pub struct KeyBindings {
    /// Forwards move key binding, defaults to [`KeyCode::W`].
    pub forward: KeyCode,

    /// Backwards move key binding, defaults to [`KeyCode::S`].
    pub backward: KeyCode,

    /// Leftwards move key binding, defaults to [`KeyCode::A`].
    pub left: KeyCode,

    /// Rightwards move key binding, defaults to [`KeyCode::D`].
    pub right: KeyCode,

    /// Upwards move key binding, defaults to [`KeyCode::Space`].
    pub up: KeyCode,

    /// Downwards move key binding, defaults to [`KeyCode::LControl`].
    pub down: KeyCode,

    /// Move speed boost key binding, defaults to [`KeyCode::LShift`].
    pub boost: KeyCode,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            forward: KeyCode::KeyW,
            backward: KeyCode::KeyS,
            left: KeyCode::KeyA,
            right: KeyCode::KeyD,
            up: KeyCode::Space,
            down: KeyCode::ControlLeft,
            boost: KeyCode::ShiftLeft,
        }
    }
}

/// Allows to configure [`FlyByCamera`] behavior.
#[derive(Debug, Resource)]
pub struct FlyByCameraConfig {
    /// Enable or disable internal systems. This flag is used by [`is_active`] run criteria.
    pub active: bool,

    /// Move speed in units.
    pub move_speed: f32,

    /// Move speed when [`KeyBindings::boost`] is enabled
    pub move_speed_boost: f32,

    /// Rotate speed in units.
    pub rotate_speed: f32,

    /// Key bindings used by camera. See [`KeyBindings`] for more info.
    pub bindings: KeyBindings,
}

impl Default for FlyByCameraConfig {
    fn default() -> Self {
        Self {
            move_speed: 10.0,
            move_speed_boost: 10.0,
            rotate_speed: PI / 25.0,
            active: false,

            bindings: KeyBindings::default(),
        }
    }
}

/// Returns [`ShouldRun::Yes`] when [`FlyByCameraConfig::active`] is true.
pub fn is_active(config: Res<FlyByCameraConfig>) -> bool {
    config.active
}

/// Move camera around using [`FlyByCameraConfig`] configuration settings.
/// This system is gated by [`is_active`] run criteria.
fn move_camera(
    time: Res<Time>,
    input: Res<ButtonInput<KeyCode>>,
    config: Res<FlyByCameraConfig>,
    mut q: Query<&mut Transform, With<FlyByCamera>>,
) {
    if let Ok(mut transform) = q.get_single_mut() {
        let input_vector = calc_input_vector(&input, &config.bindings);

        let speed = if input.pressed(config.bindings.boost) {
            config.move_speed * config.move_speed_boost
        } else {
            config.move_speed
        };

        if input_vector.length().abs() > 0.0 {
            let forward_vector = transform.forward() * input_vector.z;
            let right_vector = transform.right() * input_vector.x;
            let up_vector = Vec3::Y * input_vector.y;

            let move_vector = forward_vector + right_vector + up_vector;

            transform.translation += speed * time.delta_secs() * move_vector;
        }
    }
}

/// Rotate camera using [`FlyByCameraConfig`] configuration settings.
/// This system is gated by [`is_active`] run criteria.
fn rotate_camera(
    time: Res<Time>,
    mut motion_evt: EventReader<MouseMotion>,
    config: Res<FlyByCameraConfig>,
    mut q: Query<&mut Transform, With<FlyByCamera>>,
) {
    if let Ok(mut transform) = q.get_single_mut() {
        let mut delta = Vec2::ZERO;
        for ev in motion_evt.read() {
            delta += ev.delta;
        }

        if delta.length().abs() == 0.0 {
            return;
        }

        delta *= config.rotate_speed * time.delta_secs();

        let (pitch, yaw, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let mut rotation = Vec2::new(pitch, yaw) - delta;

        use std::f32::consts::FRAC_PI_2;
        rotation.y = rotation.y.clamp(-FRAC_PI_2, FRAC_PI_2);

        let pitch = Quat::from_axis_angle(Vec3::X, rotation.y);
        let yaw = Quat::from_axis_angle(Vec3::Y, rotation.x);

        transform.rotation = yaw * pitch;
    }
}

fn calc_input_vector(input: &Res<ButtonInput<KeyCode>>, bindings: &KeyBindings) -> Vec3 {
    let mut res = Vec3::ZERO;

    if input.pressed(bindings.forward) {
        res.z += 1.0;
    }

    if input.pressed(bindings.backward) {
        res.z -= 1.0;
    }

    if input.pressed(bindings.right) {
        res.x += 1.0;
    }

    if input.pressed(bindings.left) {
        res.x -= 1.0;
    }

    if input.pressed(bindings.up) {
        res.y += 1.0;
    }

    if input.pressed(bindings.down) {
        res.y -= 1.0;
    }

    res
}
