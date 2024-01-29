use bevy::{prelude::*, window::PrimaryWindow};
use projekto_camera::fly_by::FlyByCameraConfig;

pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera)
            .add_systems(Update, grab_mouse);
        // .add_startup_system(setup_camera)
        // .add_system(toggle_camera)
        // .add_system(grab_mouse);
    }
}

fn setup_camera(mut flyby_config: ResMut<FlyByCameraConfig>) {
    flyby_config.rotate_speed = 1.0;
}

fn grab_mouse(
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
    mouse_btn: Res<Input<MouseButton>>,
    key_btn: Res<Input<KeyCode>>,
    mut config: ResMut<FlyByCameraConfig>,
    // #[cfg(feature = "inspector")] egui_context: EguiContexts,
) {
    // #[cfg(feature = "inspector")]
    // if let Some(mut context) = egui_context {
    //     let ctx = context.ctx_mut();
    //     if ctx.is_pointer_over_area() || ctx.is_using_pointer() {
    //         return;
    //     }
    // }

    let Ok(mut window) = primary_window.get_single_mut() else {
        return;
    };

    if window.cursor.visible && mouse_btn.just_pressed(MouseButton::Left) {
        window.cursor.visible = false;
        window.cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        config.active = true;
    } else if !window.cursor.visible && key_btn.just_pressed(KeyCode::Escape) {
        window.cursor.visible = true;
        window.cursor.grab_mode = bevy::window::CursorGrabMode::None;
        config.active = false;
    }
}
