use std::f32::consts::PI;

use bevy::{ecs::schedule::ShouldRun, input::mouse::MouseMotion, prelude::*};

#[cfg(feature = "inspector")]
use bevy_egui::EguiContext;

use super::MainCamera;

pub struct FlyByCameraPlugin;

impl Plugin for FlyByCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup)
            .add_system(grab_mouse)
            .add_system_set(
                SystemSet::new()
                    .with_run_criteria(is_active)
                    .with_system(move_camera)
                    .with_system(rotate_camera),
            );
    }
}

pub struct FlyByCameraConfig {
    pub move_speed: f32,
    pub move_speed_boost: f32,
    pub rotate_speed: f32,
    pub active: bool,
    rotation: Vec2,
}

impl Default for FlyByCameraConfig {
    fn default() -> Self {
        Self {
            move_speed: 10.0,
            move_speed_boost: 10.0,
            rotate_speed: PI / 25.0,
            rotation: Vec2::ZERO,
            active: false,
        }
    }
}

fn setup(mut commands: Commands) {
    commands.insert_resource(FlyByCameraConfig::default());

    // match q.get_single() {
    //     Ok(e) => {
    //         warn!("Camera already exists, adding FlyByCamera to it");
    //         commands
    //             .entity(e)
    //             .insert(FlyByCamera::default())
    //             .insert(TerraformationCenter);
    //     }
    //     Err(QuerySingleError::MultipleEntities(_)) => {
    //         error!("Multiple camera already exists. Unable to setup FlyByCamera");
    //     }
    //     Err(QuerySingleError::NoEntities(_)) => {
    //         commands
    //             .spawn_bundle(Camera3dBundle {
    //                 transform: Transform::from_xyz(-10.0, 25.0, 20.0),
    //                 ..Default::default()
    //             })
    //             .insert(FlyByCamera::default())
    //             .insert(TerraformationCenter);
    //     }
    // }
}

pub fn is_active(config: Res<FlyByCameraConfig>) -> ShouldRun {
    if config.active {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}

fn move_camera(
    time: Res<Time>,
    input: Res<Input<KeyCode>>,
    config: Res<FlyByCameraConfig>,
    mut q: Query<&mut Transform, With<MainCamera>>,
) {
    if let Ok(mut transform) = q.get_single_mut() {
        let input_vector = calc_input_vector(&input);

        let speed = if input.pressed(KeyCode::LShift) {
            config.move_speed * config.move_speed_boost
        } else {
            config.move_speed
        };

        if input_vector.length().abs() > 0.0 {
            let forward_vector = calc_forward_vector(&transform) * input_vector.z;
            let right_vector = calc_right_vector(&transform) * input_vector.x;
            let up_vector = Vec3::Y * input_vector.y;

            let move_vector = forward_vector + right_vector + up_vector;

            transform.translation += speed * time.delta_seconds() * move_vector;
        }
    }
}

fn rotate_camera(
    time: Res<Time>,
    mut motion_evt: EventReader<MouseMotion>,
    mut config: ResMut<FlyByCameraConfig>,
    mut q: Query<&mut Transform, With<MainCamera>>,
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

        config.rotation += delta;
        config.rotation.y = config.rotation.y.clamp(-PI / 2.0, PI / 2.0);

        let pitch = Quat::from_axis_angle(Vec3::X, -config.rotation.y);
        let yaw = Quat::from_axis_angle(Vec3::Y, -config.rotation.x);

        transform.rotation = yaw * pitch;
    }
}

fn grab_mouse(
    mut windows: ResMut<Windows>,
    mouse_btn: Res<Input<MouseButton>>,
    key_btn: Res<Input<KeyCode>>,
    mut config: ResMut<FlyByCameraConfig>,
    #[cfg(feature = "inspector")] egui_context: Option<ResMut<EguiContext>>,
) {
    #[cfg(feature = "inspector")]
    if let Some(mut context) = egui_context {
        let ctx = context.ctx_mut();
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() {
            return;
        }
    }

    if let Some(window) = windows.get_primary_mut() {
        if window.cursor_visible() && mouse_btn.just_pressed(MouseButton::Left) {
            window.set_cursor_visibility(false);
            window.set_cursor_lock_mode(true);
            config.active = true;
        } else if !window.cursor_visible() && key_btn.just_pressed(KeyCode::Escape) {
            window.set_cursor_visibility(true);
            window.set_cursor_lock_mode(false);
            config.active = false;
        }
    }
}

fn calc_forward_vector(t: &Transform) -> Vec3 {
    t.rotation.mul_vec3(Vec3::Z).normalize() * -1.0
}

fn calc_right_vector(t: &Transform) -> Vec3 {
    t.rotation.mul_vec3(Vec3::X).normalize()
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
