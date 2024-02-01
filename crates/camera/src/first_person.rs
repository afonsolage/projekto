use std::f32::consts::PI;

use bevy::prelude::*;

use bevy::input::mouse::MouseMotion;

/// Adds [`FirstPersonCameraConfig`] resource and internals systems gated by [`is_active`] run
/// criteria grouped on [`CameraUpdate`] system set.
pub struct FirstPersonCameraPlugin;

impl Plugin for FirstPersonCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FirstPersonCameraConfig>().add_systems(
            Update,
            rotate_camera.in_set(super::CameraUpdate).run_if(is_active),
        );
    }
}

/// Component used to tag entity camera.
/// There can be only one Entity with this component.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct FirstPersonCamera;

/// Allows to configure [`FirstPersonCamera`] behavior.
#[derive(Debug, Resource)]
pub struct FirstPersonCameraConfig {
    /// Enable or disable internal systems. This flag is used by [`is_active`] run criteria.
    pub active: bool,

    /// Rotate speed in units.
    pub rotate_speed: f32,
}

impl Default for FirstPersonCameraConfig {
    fn default() -> Self {
        Self {
            rotate_speed: PI / 25.0,
            active: false,
        }
    }
}

/// Returns [`ShouldRun::Yes`] when [`FirstPersonCameraConfig::active`] is true.
pub fn is_active(config: Res<FirstPersonCameraConfig>) -> bool {
    config.active
}

/// Rotate camera using [`FirstPersonCameraConfig`] configuration settings.
/// This system is gated by [`is_active`] run criteria.
fn rotate_camera(
    time: Res<Time>,
    mut motion_evt: EventReader<MouseMotion>,
    config: Res<FirstPersonCameraConfig>,
    mut q: Query<&mut Transform, With<FirstPersonCamera>>,
) {
    if let Ok(mut transform) = q.get_single_mut() {
        let mut delta = Vec2::ZERO;
        for ev in motion_evt.read() {
            delta += ev.delta;
        }

        if delta.length().abs() == 0.0 {
            return;
        }

        delta *= config.rotate_speed * time.delta_seconds();

        let (pitch, yaw, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let mut rotation = Vec2::new(pitch, yaw) - delta;

        use std::f32::consts::FRAC_PI_2;
        rotation.y = rotation.y.clamp(-FRAC_PI_2, FRAC_PI_2);

        let pitch = Quat::from_axis_angle(Vec3::X, rotation.y);
        let yaw = Quat::from_axis_angle(Vec3::Y, rotation.x);

        transform.rotation = yaw * pitch;
    }
}
