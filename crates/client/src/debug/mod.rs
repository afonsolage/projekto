use bevy::{app::AppExit, prelude::*};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_hold_est_to_exit)
            // .add_system(slow_down_fps)
            .add_systems(Update, hold_esc_to_exit);
    }
}

const ESC_HOLD_TIMEOUT: f32 = 0.2;

#[derive(Resource)]
struct EscHolding(f32);

fn setup_hold_est_to_exit(mut commands: Commands) {
    commands.insert_resource(EscHolding(0.0));
}

fn hold_esc_to_exit(
    mut esc_holding: ResMut<EscHolding>,
    time: Res<Time>,
    input_keys: Res<ButtonInput<KeyCode>>,
    mut exit_writer: EventWriter<AppExit>,
) {
    if input_keys.pressed(KeyCode::Escape) {
        esc_holding.0 += time.delta_secs();

        if esc_holding.0 >= ESC_HOLD_TIMEOUT {
            info!("Exiting app due to ESC holding...");
            exit_writer.send(AppExit::Success);
        }
    } else {
        esc_holding.0 = 0.0;
    }
}

// fn slow_down_fps() {
//     std::thread::sleep(std::time::Duration::from_millis(200));
// }
