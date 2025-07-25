use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use projekto_camera::{
    first_person::{FirstPersonCamera, FirstPersonCameraConfig},
    fly_by::{FlyByCamera, FlyByCameraConfig},
};

use crate::controller::character_controller::CharacterControllerConfig;

pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveCamera>()
            .add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                (
                    switch_camera.run_if(input_any_just_pressed([
                        KeyCode::KeyI,
                        KeyCode::KeyO,
                        KeyCode::KeyP,
                    ])),
                    grab_mouse,
                ),
            );
    }
}

fn input_any_just_pressed<T>(
    inputs: impl IntoIterator<Item = T> + Copy,
) -> impl Fn(Res<'_, ButtonInput<T>>) -> bool + Clone
where
    T: Clone + Copy + Eq + std::hash::Hash + Send + Sync + 'static,
{
    move |input: Res<ButtonInput<T>>| input.any_just_pressed(inputs)
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
struct CameraConfig<'w, 's> {
    flyby: ResMut<'w, FlyByCameraConfig>,
    first_person: ResMut<'w, FirstPersonCameraConfig>,
    q: ParamSet<
        'w,
        's,
        (
            Query<'w, 's, &'static mut Camera, With<FlyByCamera>>,
            Query<'w, 's, &'static mut Camera, With<FirstPersonCamera>>,
        ),
    >,
    active_cam: ResMut<'w, ActiveCamera>,
    character_controller: ResMut<'w, CharacterControllerConfig>,
}

impl<'w, 's> CameraConfig<'w, 's> {
    fn set_cam(&mut self, active_camera: ActiveCamera) {
        trace!("Toggling cameras");

        self.first_person.active = false;
        self.flyby.active = false;
        self.character_controller.active = false;
        self.q.p0().single_mut().is_active = false;
        self.q.p1().single_mut().is_active = false;

        *self.active_cam = active_camera;
        match *self.active_cam {
            ActiveCamera::FlyBy => {
                self.flyby.active = true;
                self.q.p0().single_mut().is_active = true;
            }
            ActiveCamera::FirstPerson => {
                self.character_controller.active = true;
                self.first_person.active = true;
                self.q.p1().single_mut().is_active = true;
            }
        }
    }

    fn set_active(&mut self, active: bool) {
        match *self.active_cam {
            ActiveCamera::FlyBy => self.flyby.active = active,
            ActiveCamera::FirstPerson => {
                self.character_controller.active = active;
                self.first_person.active = active;
            }
        }
    }
}

fn switch_camera(key_btn: Res<ButtonInput<KeyCode>>, mut config: CameraConfig) {
    if key_btn.just_pressed(KeyCode::KeyI) {
        config.set_cam(ActiveCamera::FlyBy);
    } else if key_btn.just_pressed(KeyCode::KeyP) {
        config.set_cam(ActiveCamera::FirstPerson);
    }
}

fn grab_mouse(
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
    mouse_btn: Res<ButtonInput<MouseButton>>,
    key_btn: Res<ButtonInput<KeyCode>>,
    mut config: CameraConfig,
) {
    let Ok(mut window) = primary_window.get_single_mut() else {
        return;
    };

    if window.cursor_options.visible && mouse_btn.just_pressed(MouseButton::Left) {
        window.cursor_options.visible = false;
        window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
        config.set_active(true);
    } else if !window.cursor_options.visible && key_btn.just_pressed(KeyCode::Escape) {
        window.cursor_options.visible = true;
        window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
        config.set_active(false);
    }
}
