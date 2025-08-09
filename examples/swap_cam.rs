use std::marker::PhantomData;

use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use projekto_camera::{
    CameraPlugin,
    fly_by::{FlyByCamera, FlyByCameraConfig},
    orbit::{OrbitCamera, OrbitCameraConfig, OrbitCameraTarget},
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CameraPlugin))
        .init_resource::<CurrentCameraType>()
        .add_systems(Startup, setup_environment)
        .add_systems(Update, (grab_mouse, toggle_camera))
        .run();
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

    _marker: PhantomData<&'s ()>,
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

fn toggle_camera(input: Res<ButtonInput<KeyCode>>, mut config: CameraConfig) {
    if input.just_pressed(KeyCode::F1) {
        config.toggle();
    }
}

fn grab_mouse(
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
    mouse_btn: Res<ButtonInput<MouseButton>>,
    key_btn: Res<ButtonInput<KeyCode>>,
    mut config: CameraConfig,
    // #[cfg(feature = "inspector")] egui_context: Option<ResMut<bevy_egui::EguiContext>>,
) {
    // #[cfg(feature = "inspector")]
    // if let Some(mut context) = egui_context {
    //     let ctx = context.ctx_mut();
    //     if ctx.is_pointer_over_area() || ctx.is_using_pointer() {
    //         return;
    //     }
    // }

    if let Ok(mut window) = primary_window.single_mut() {
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
}

fn setup_environment(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // camera
    commands
        .spawn(Camera3d::default())
        .insert(OrbitCamera)
        .insert(FlyByCamera)
        // .insert(Transform::from_xyz(5.0, 20.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y))
        ;

    // target
    commands
        .spawn((
            Transform::from_xyz(3.0, 5.0, 3.0),
            Mesh3d(meshes.add(Capsule3d {
                radius: 0.25,
                half_length: 0.75,
            })),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.3, 0.3))),
        ))
        .insert(OrbitCameraTarget)
        .insert(Name::new("Target"));

    // X axis
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(3.0, 0.1, 0.1))),
        MeshMaterial3d(materials.add(Color::srgb(1.0, 0.3, 0.3))),
    ));

    // Y axis
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.1, 3.0, 0.1))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 1.0, 0.3))),
    ));

    // Z axis
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.1, 0.1, 3.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.3, 1.0))),
    ));

    commands.spawn((PointLight::default(), Transform::from_xyz(4.0, 8.0, 4.0)));
}
