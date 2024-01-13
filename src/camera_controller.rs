use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
// use bevy_egui::EguiContexts;
use projekto_camera::{fly_by::FlyByCameraConfig, orbit::OrbitCameraConfig};

pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentCameraType>()
            .add_systems(Startup, setup_camera)
            .add_systems(Update, (toggle_camera, grab_mouse));
        // .add_startup_system(setup_camera)
        // .add_system(toggle_camera)
        // .add_system(grab_mouse);
    }
}

fn setup_camera(
    mut orbit_config: ResMut<OrbitCameraConfig>,
    mut flyby_config: ResMut<FlyByCameraConfig>,
) {
    orbit_config.key_rotate_speed = 1.0;
    orbit_config.max_polar_angle = std::f32::consts::FRAC_PI_2 - 0.001;
    flyby_config.rotate_speed = 1.0;
}

#[derive(Default, Debug, Resource)]
enum CurrentCameraType {
    #[default]
    Orbit,
    FlyBy,
}

#[derive(SystemParam)]
struct CameraConfig<'w, 's> {
    orbit: ResMut<'w, OrbitCameraConfig>,
    flyby: ResMut<'w, FlyByCameraConfig>,
    cam_type: ResMut<'w, CurrentCameraType>,

    #[system_param(ignore)]
    _pd: std::marker::PhantomData<&'s ()>,
}

impl<'w, 's> CameraConfig<'w, 's> {
    fn toggle(&mut self) {
        trace!("Toggling cameras");

        match *self.cam_type {
            CurrentCameraType::Orbit => {
                *self.cam_type = CurrentCameraType::FlyBy;
                self.flyby.active = true;
                self.orbit.active = false;
            }
            CurrentCameraType::FlyBy => {
                *self.cam_type = CurrentCameraType::Orbit;
                self.flyby.active = false;
                self.orbit.active = true;
            }
        }
    }

    fn set_active(&mut self, active: bool) {
        match *self.cam_type {
            CurrentCameraType::Orbit => self.orbit.active = active,
            CurrentCameraType::FlyBy => self.flyby.active = active,
        }
    }
}

fn toggle_camera(input: Res<Input<KeyCode>>, mut config: CameraConfig) {
    if input.just_pressed(KeyCode::F9) {
        config.toggle();
    }
}

fn grab_mouse(
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
    mouse_btn: Res<Input<MouseButton>>,
    key_btn: Res<Input<KeyCode>>,
    mut config: CameraConfig,
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
        config.set_active(true);
    } else if !window.cursor.visible && key_btn.just_pressed(KeyCode::Escape) {
        window.cursor.visible = true;
        window.cursor.grab_mode = bevy::window::CursorGrabMode::None;
        config.set_active(false);
    }
}
