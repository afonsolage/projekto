use bevy::{
    ecs::system::SystemParam, input::common_conditions::input_just_pressed, prelude::*,
    window::PrimaryWindow,
};
use bevy_egui::EguiContexts;
use projekto_camera::{first_person::FirstPersonCameraConfig, fly_by::FlyByCameraConfig};

pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveCamera>()
            .add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                (
                    toggle_cam.run_if(input_just_pressed(KeyCode::F9)),
                    grab_mouse,
                ),
            );
    }
}

fn setup_camera(
    mut flyby_config: ResMut<FlyByCameraConfig>,
    mut fp_config: ResMut<FirstPersonCameraConfig>,
) {
    flyby_config.rotate_speed = 1.0;
    fp_config.rotate_speed = 1.0;
}

#[derive(Default, Debug, Resource)]
enum ActiveCamera {
    #[default]
    FlyBy,
    FirstPerson,
}

#[derive(SystemParam)]
struct CameraConfig<'w> {
    flyby: ResMut<'w, FlyByCameraConfig>,
    first_person: ResMut<'w, FirstPersonCameraConfig>,
    active_cam: ResMut<'w, ActiveCamera>,
}

impl<'w> CameraConfig<'w> {
    fn toggle(&mut self) {
        trace!("Toggling cameras");

        match *self.active_cam {
            ActiveCamera::FlyBy => {
                *self.active_cam = ActiveCamera::FirstPerson;
                self.flyby.active = false;
                self.first_person.active = true;
            }
            ActiveCamera::FirstPerson => {
                *self.active_cam = ActiveCamera::FlyBy;
                self.flyby.active = true;
                self.first_person.active = false;
            }
        }
    }

    fn set_active(&mut self, active: bool) {
        match *self.active_cam {
            ActiveCamera::FlyBy => self.flyby.active = active,
            ActiveCamera::FirstPerson => self.first_person.active = active,
        }
    }
}

fn toggle_cam(mut config: CameraConfig) {
    config.toggle()
}

fn grab_mouse(
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
    mouse_btn: Res<Input<MouseButton>>,
    key_btn: Res<Input<KeyCode>>,
    mut config: CameraConfig,
    #[cfg(feature = "inspector")] mut egui_context: EguiContexts,
) {
    #[cfg(feature = "inspector")]
    {
        let ctx = egui_context.ctx_mut();
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() {
            return;
        }
    }

    let Ok(mut window) = primary_window.get_single_mut() else {
        return;
    };

    if window.cursor.visible && mouse_btn.just_pressed(MouseButton::Left) {
        window.cursor.visible = false;
        window.cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        config.set_active(true);
    } else if !window.cursor.visible && key_btn.just_pressed(KeyCode::Escape) {
        window.cursor.visible = true;
        window.cursor.grab_mode = bevy::window::CursorGrabMode::None;
        config.set_active(false);
    }
}
