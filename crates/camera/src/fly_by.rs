use std::f32::consts::PI;

use bevy::{ecs::schedule::ShouldRun, prelude::*};

#[cfg(feature = "flyby_controls")]
use bevy::input::mouse::MouseMotion;

pub struct FlyByCameraPlugin;

impl Plugin for FlyByCameraPlugin {
    fn build(&self, app: &mut App) {
        let camera_system_set = SystemSet::new()
            .with_run_criteria(is_active)
            .label(CameraUpdate);

        #[cfg(feature = "flyby_controls")]
        let camera_system_set = camera_system_set
            .with_system(move_camera)
            .with_system(rotate_camera);

        app.add_startup_system(setup)
            .add_system_set(camera_system_set);
    }
}

#[derive(SystemLabel)]
pub struct CameraUpdate;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct FlyByCamera;

#[cfg(feature = "flyby_controls")]
#[derive(Debug)]
pub struct KeyBindings {
    pub forward: KeyCode,
    pub backward: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub up: KeyCode,
    pub down: KeyCode,
    pub boost: KeyCode,
}

#[cfg(feature = "flyby_controls")]
impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            forward: KeyCode::W,
            backward: KeyCode::S,
            left: KeyCode::A,
            right: KeyCode::D,
            up: KeyCode::Space,
            down: KeyCode::LControl,
            boost: KeyCode::LShift,
        }
    }
}

#[derive(Debug)]
pub struct FlyByCameraConfig {
    pub active: bool,
    
    pub move_speed: f32,
    pub move_speed_boost: f32,
    pub rotate_speed: f32,

    #[cfg(feature = "flyby_controls")]
    pub bindings: KeyBindings,
}

impl Default for FlyByCameraConfig {
    fn default() -> Self {
        Self {
            move_speed: 10.0,
            move_speed_boost: 10.0,
            rotate_speed: PI / 25.0,
            active: false,

            #[cfg(feature = "flyby_controls")]
            bindings: KeyBindings::default(),
        }
    }
}

fn setup(mut commands: Commands) {
    commands.insert_resource(FlyByCameraConfig::default());
}

pub fn is_active(config: Res<FlyByCameraConfig>) -> ShouldRun {
    if config.active {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

#[cfg(feature = "flyby_controls")]
fn move_camera(
    time: Res<Time>,
    input: Res<Input<KeyCode>>,
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

            transform.translation += speed * time.delta_seconds() * move_vector;
        }
    }
}

#[cfg(feature = "flyby_controls")]
fn rotate_camera(
    time: Res<Time>,
    mut motion_evt: EventReader<MouseMotion>,
    config: Res<FlyByCameraConfig>,
    mut q: Query<&mut Transform, With<FlyByCamera>>,
) {
    if let Ok(mut transform) = q.get_single_mut() {
        let mut delta = Vec2::ZERO;
        for ev in motion_evt.iter() {
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

#[cfg(feature = "flyby_controls")]
fn calc_input_vector(input: &Res<Input<KeyCode>>, bindings: &KeyBindings) -> Vec3 {
    let mut res = Vec3::ZERO;

    if input.pressed(bindings.forward) {
        res.z += 1.0
    }

    if input.pressed(bindings.backward) {
        res.z -= 1.0
    }

    if input.pressed(bindings.right) {
        res.x += 1.0
    }

    if input.pressed(bindings.left) {
        res.x -= 1.0
    }

    if input.pressed(bindings.up) {
        res.y += 1.0
    }

    if input.pressed(bindings.down) {
        res.y -= 1.0
    }

    res
}
