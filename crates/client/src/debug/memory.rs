use std::time::Instant;

use bevy::prelude::*;
use projekto_diagnostics::{ProcessMemory, ProcessMemoryPlugin, format_bytes};
use projekto_messages::DebugMemory;
use projekto_proto::RegisterMessageHandler;

const SERVER_REPORT_TIMEOUT_SECS: f32 = 3.0;
const OVERLAY_REFRESH_SECS: f32 = 0.5;

pub(crate) struct MemoryOverlayPlugin;

impl Plugin for MemoryOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ProcessMemoryPlugin)
            .init_resource::<ServerMemory>()
            .add_message_handler(update_server_memory)
            .add_systems(Startup, spawn_overlay)
            .add_systems(Update, (toggle_overlay, update_overlay));
    }
}

#[derive(Component)]
struct MemoryOverlay;

#[derive(Resource, Default)]
struct ServerMemory {
    rss_bytes: u64,
    peak_rss_bytes: u64,
    last_report: Option<Instant>,
}

fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        IsDefaultUiCamera,
        Name::new("Memory Overlay Camera"),
    ));

    commands.spawn((
        MemoryOverlay,
        Text::new("Client RAM: --\nServer RAM: --"),
        TextFont::from_font_size(14.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            padding: UiRect::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        Visibility::Visible,
    ));
}

fn update_server_memory(In(report): In<DebugMemory>, mut memory: ResMut<ServerMemory>) {
    memory.rss_bytes = report.rss_bytes;
    memory.peak_rss_bytes = report.peak_rss_bytes;
    memory.last_report = Some(Instant::now());
}

fn toggle_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut overlay: Query<&mut Visibility, With<MemoryOverlay>>,
) {
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }

    if let Ok(mut visibility) = overlay.single_mut() {
        *visibility = if *visibility == Visibility::Hidden {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn update_overlay(
    time: Res<Time>,
    client_memory: Res<ProcessMemory>,
    server_memory: Res<ServerMemory>,
    mut elapsed: Local<f32>,
    mut overlay: Query<&mut Text, With<MemoryOverlay>>,
) {
    *elapsed += time.delta_secs();

    if *elapsed < OVERLAY_REFRESH_SECS {
        return;
    }

    *elapsed = 0.0;

    let client = format!(
        "Client RAM: {} (peak {})",
        format_bytes(client_memory.rss_bytes),
        format_bytes(client_memory.peak_rss_bytes)
    );

    let server = match server_memory.last_report {
        Some(report) if report.elapsed().as_secs_f32() <= SERVER_REPORT_TIMEOUT_SECS => format!(
            "Server RAM: {} (peak {})",
            format_bytes(server_memory.rss_bytes),
            format_bytes(server_memory.peak_rss_bytes)
        ),
        _ => "Server RAM: --".to_string(),
    };

    if let Ok(mut text) = overlay.single_mut() {
        text.0 = format!("{client}\n{server}");
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;
    use projekto_proto::{ClientId, RunMessageHandlers};

    use super::*;

    #[test]
    fn plugin_registers_handler_and_spawns_overlay() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(MemoryOverlayPlugin);
        app.update();

        let mut cameras = app.world_mut().query::<&IsDefaultUiCamera>();
        assert_eq!(
            cameras.iter(app.world()).count(),
            1,
            "the overlay needs a dedicated UI camera"
        );

        let mut overlays = app.world_mut().query::<(&Text, &MemoryOverlay)>();
        assert_eq!(
            overlays.iter(app.world()).count(),
            1,
            "the overlay text must be spawned"
        );

        app.world_mut().run_handlers::<DebugMemory>(
            ClientId::default(),
            Box::new(DebugMemory {
                rss_bytes: 123,
                peak_rss_bytes: 456,
            }),
        );
        app.update();

        let memory = app.world().resource::<ServerMemory>();
        assert_eq!(memory.rss_bytes, 123);
        assert_eq!(memory.peak_rss_bytes, 456);
        assert!(memory.last_report.is_some());
    }
}
