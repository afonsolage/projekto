use bevy::{app::AppExit, prelude::*};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup_hold_est_to_exit);
        app.add_system(hold_esc_to_exit);
    }
}

const ESC_HOLD_TIMEOUT: f32 = 0.2;
struct EscHolding(f32);

fn setup_hold_est_to_exit(mut commands: Commands) {
    commands.insert_resource(EscHolding(0.0));
}

fn hold_esc_to_exit(
    mut esc_holding: ResMut<EscHolding>,
    time: Res<Time>,
    input_keys: Res<Input<KeyCode>>,
    mut exit_writer: EventWriter<AppExit>,
) {
    if input_keys.pressed(KeyCode::Escape) {
        esc_holding.0 += time.delta_seconds();

        if esc_holding.0 >= ESC_HOLD_TIMEOUT {
            info!("Exiting app due to ESC holding...");
            exit_writer.send(AppExit);
        }
    } else {
        esc_holding.0 = 0.0;
    }
}
