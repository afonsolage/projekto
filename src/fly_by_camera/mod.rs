use std::f32::consts::PI;

use bevy::{
    ecs::{schedule::ShouldRun, system::QuerySingleError},
    input::mouse::MouseMotion,
    prelude::*,
    render::camera::Camera,
};
use bevy_egui::EguiContext;

use crate::world::terraformation::TerraformationCenter;

pub struct FlyByCameraPlugin;

impl Plugin for FlyByCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup_fly_by_camera)
            .add_system(fly_by_camera_grab_mouse_system)
            .add_system_set(
                SystemSet::new()
                    .with_run_criteria(is_fly_by_camera_active)
                    .with_system(fly_by_camera_move_system)
                    .with_system(fly_by_camera_rotate_system),
            );
    }
}

#[derive(Component)]
pub struct FlyByCamera {
    pub move_speed: f32,
    pub move_speed_boost: f32,
    pub rotate_speed: f32,
    pub active: bool,
    rotation: Vec2,
}

impl Default for FlyByCamera {
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

// Systems

fn setup_fly_by_camera(mut commands: Commands, q: Query<Entity, With<Camera>>) {
    match q.get_single() {
        Ok(e) => {
            warn!("Camera already exists, adding FlyByCamera to it");
            commands
                .entity(e)
                .insert(FlyByCamera::default())
                .insert(TerraformationCenter);
        }
        Err(QuerySingleError::MultipleEntities(_)) => {
            error!("Multiple camera already exists. Unable to setup FlyByCamera");
        }
        Err(QuerySingleError::NoEntities(_)) => {
            commands
                .spawn_bundle(PerspectiveCameraBundle {
                    transform: Transform::from_xyz(-10.0, 25.0, 20.0),
                    ..Default::default()
                })
                .insert(FlyByCamera::default())
                .insert(TerraformationCenter);
        }
    }
}

fn is_fly_by_camera_active(q: Query<&FlyByCamera>) -> ShouldRun {
    match q.get_single() {
        Ok(cam) if cam.active => ShouldRun::Yes,
        _ => ShouldRun::No,
    }
}

fn fly_by_camera_move_system(
    time: Res<Time>,
    input: Res<Input<KeyCode>>,
    mut q: Query<(&mut Transform, &FlyByCamera)>,
) {
    if let Ok((mut transform, fly_by_camera)) = q.get_single_mut() {
        let input_vector = calc_input_vector(&input);

        let speed = if input.pressed(KeyCode::LShift) {
            fly_by_camera.move_speed * fly_by_camera.move_speed_boost
        } else {
            fly_by_camera.move_speed
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

fn fly_by_camera_rotate_system(
    time: Res<Time>,
    mut motion_evt: EventReader<MouseMotion>,
    mut q: Query<(&mut Transform, &mut FlyByCamera)>,
) {
    if let Ok((mut transform, mut fly_by_camera)) = q.get_single_mut() {
        let mut delta = Vec2::ZERO;
        for ev in motion_evt.iter() {
            delta += ev.delta;
        }

        if delta.length().abs() == 0.0 {
            return;
        }

        delta *= fly_by_camera.rotate_speed * time.delta_seconds();

        fly_by_camera.rotation += delta;
        fly_by_camera.rotation.y = fly_by_camera.rotation.y.clamp(-PI / 2.0, PI / 2.0);

        let pitch = Quat::from_axis_angle(Vec3::X, -fly_by_camera.rotation.y);
        let yaw = Quat::from_axis_angle(Vec3::Y, -fly_by_camera.rotation.x);

        transform.rotation = yaw * pitch;
    }
}

fn fly_by_camera_grab_mouse_system(
    mut windows: ResMut<Windows>,
    mouse_btn: Res<Input<MouseButton>>,
    key_btn: Res<Input<KeyCode>>,
    egui_context: Option<ResMut<EguiContext>>,
    mut q: Query<&mut FlyByCamera>,
) {
    if let Some(mut context) = egui_context {
        let ctx = context.ctx_mut();
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() {
            return;
        }
    }

    if let Some(window) = windows.get_primary_mut() {
        if let Ok(mut fly_by_cam) = q.get_single_mut() {
            if window.cursor_visible() && mouse_btn.just_pressed(MouseButton::Left) {
                window.set_cursor_visibility(false);
                window.set_cursor_lock_mode(true);
                fly_by_cam.active = true;
            } else if !window.cursor_visible() && key_btn.just_pressed(KeyCode::Escape) {
                window.set_cursor_visibility(true);
                window.set_cursor_lock_mode(false);
                fly_by_cam.active = false;
            }
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
