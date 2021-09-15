use bevy::{app::AppExit, prelude::*};

#[cfg(feature = "perf_counter")]
pub mod perf;

macro_rules! perf_fn {
    () => {{
        #[cfg(feature = "perf_counter")]
        {
            fn f() {}
            fn type_name_of<T>(_: T) -> &'static str {
                std::any::type_name::<T>()
            }
            let name = type_name_of(f);

            // Find and cut the rest of the path
            let fn_name = match &name[..name.len() - 3].rfind(':') {
                Some(pos) => &name[pos + 1..name.len() - 3],
                None => &name[..name.len() - 3],
            };
            PerfCounterGuard::new(fn_name)
        }
        #[cfg(not(feature = "perf_counter"))]
        ()
    }};
}

macro_rules! perf_scope {
    ($var:ident) => {
        #[cfg(feature = "perf_counter")]
        let _perf = $var.measure();
    };
}

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup_hold_est_to_exit)
            .add_system(hold_esc_to_exit);

        #[cfg(feature = "perf_counter")]
        app.add_plugin(perf::PerfCounterPlugin);
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
