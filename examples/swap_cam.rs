use std::marker::PhantomData;

use bevy::{ecs::system::SystemParam, prelude::*};
use projekto_camera::{
    fly_by::{FlyByCamera, FlyByCameraConfig},
    orbit::{OrbitCamera, OrbitCameraConfig, OrbitCameraTarget},
    CameraPlugin,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(CameraPlugin)
        .init_resource::<CurrentCameraType>()
        .add_startup_system(setup_environment)
        .add_system(grab_mouse)
        .add_system(toggle_camera)
        .run();
}

#[derive(Default, Debug)]
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
    _pd: PhantomData<&'s ()>,
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
    if input.just_pressed(KeyCode::F1) {
        config.toggle();
    }
}

fn grab_mouse(
    mut windows: ResMut<Windows>,
    mouse_btn: Res<Input<MouseButton>>,
    key_btn: Res<Input<KeyCode>>,
    mut config: CameraConfig,
    #[cfg(feature = "inspector")] egui_context: Option<ResMut<bevy_egui::EguiContext>>,
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
            config.set_active(true);
        } else if !window.cursor_visible() && key_btn.just_pressed(KeyCode::Escape) {
            window.set_cursor_visibility(true);
            window.set_cursor_lock_mode(false);
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
        .spawn_bundle(Camera3dBundle { ..default() })
        .insert(OrbitCamera)
        .insert(FlyByCamera)
        // .insert(Transform::from_xyz(5.0, 20.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y))
        ;

    // target
    commands
        .spawn_bundle(PbrBundle {
            transform: Transform::from_xyz(3.0, 5.0, 3.0),
            mesh: meshes.add(Mesh::from(shape::Capsule {
                radius: 0.25,
                depth: 1.5,
                ..default()
            })),
            material: materials.add(Color::rgb(0.3, 0.3, 0.3).into()),
            ..Default::default()
        })
        .insert(OrbitCameraTarget)
        .insert(Name::new("Target"));

    // X axis
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Box {
            min_x: 0.0,
            max_x: 3.0,
            min_y: 0.0,
            max_y: 0.1,
            min_z: 0.0,
            max_z: 0.1,
        })),
        material: materials.add(Color::rgb(1.0, 0.3, 0.3).into()),
        ..Default::default()
    });

    // Y axis
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Box {
            min_x: 0.0,
            max_x: 0.1,
            min_y: 0.0,
            max_y: 3.0,
            min_z: 0.0,
            max_z: 0.1,
        })),
        material: materials.add(Color::rgb(0.3, 1.0, 0.3).into()),
        ..Default::default()
    });

    // Z axis
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Box {
            min_x: 0.0,
            max_x: 0.1,
            min_y: 0.0,
            max_y: 0.1,
            min_z: 0.0,
            max_z: 3.0,
        })),
        material: materials.add(Color::rgb(0.3, 0.3, 1.0).into()),
        ..Default::default()
    });

    commands.spawn_bundle(PointLightBundle {
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..Default::default()
    });
}
