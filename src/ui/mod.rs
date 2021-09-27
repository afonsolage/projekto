use bevy::{
    core::FixedTimestep,
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
};

// use bevy_egui::{egui, EguiContext, EguiPlugin};

// use crate::world::DebugCmd;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app
            // .add_plugin(EguiPlugin)
            .add_plugin(FrameTimeDiagnosticsPlugin::default())
            .add_startup_system(setup_fps_text)
            // .add_system(cmd_window)
            .add_system_set(
                SystemSet::new()
                    .with_run_criteria(FixedTimestep::step(0.5))
                    .with_system(update_fps_text_system),
            );

        #[cfg(feature = "mem_alloc")]
        app.add_startup_system(mem_alloc::setup_mem_text)
            .add_system(
                mem_alloc::update_mem_text_system.with_run_criteria(FixedTimestep::step(0.5)),
            );
    }
}

// #[derive(Default)]
// struct CmdWindowMeta {
//     cmd: String,
// }

// fn cmd_window(
//     egui_context: Res<EguiContext>,
//     mut meta: Local<CmdWindowMeta>,
//     mut writer: EventWriter<DebugCmd>,
// ) {
//     egui::Window::new("Commands").show(egui_context.ctx(), |ui| {
//         if ui.text_edit_singleline(&mut meta.cmd).lost_focus() {
//             if meta.cmd.is_empty() {
//                 return;
//             }

//             writer.send(DebugCmd(meta.cmd.clone()));
//             meta.cmd.clear();
//         }
//     });
// }

struct FpsCounterTag;
fn setup_fps_text(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/FiraSans-Bold.ttf");

    commands.spawn_bundle(UiCameraBundle::default());
    commands
        .spawn_bundle(TextBundle {
            style: Style {
                align_self: AlignSelf::FlexEnd,
                position_type: PositionType::Absolute,
                position: Rect {
                    top: Val::Px(5.0),
                    left: Val::Px(15.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            text: Text::with_section(
                "0",
                TextStyle {
                    font,
                    font_size: 30.0,
                    color: Color::YELLOW,
                },
                Default::default(),
            ),
            ..Default::default()
        })
        .insert(FpsCounterTag);
}

fn update_fps_text_system(
    diagnostics: Res<Diagnostics>,
    mut q: Query<&mut Text, With<FpsCounterTag>>,
) {
    if let Ok(mut t) = q.get_single_mut() {
        if let Some(fps) = diagnostics.get(FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(avg) = fps.average() {
                t.sections[0].value = format!("{:.0}", avg);
            }
        }
    }
}

#[cfg(feature = "mem_alloc")]
mod mem_alloc {
    use super::*;
    use crate::world::storage::chunk;

    pub(super) struct MemCounterTag;
    pub(super) fn setup_mem_text(mut commands: Commands, asset_server: Res<AssetServer>) {
        let font = asset_server.load("fonts/FiraSans-Bold.ttf");

        commands.spawn_bundle(UiCameraBundle::default());
        commands
            .spawn_bundle(TextBundle {
                style: Style {
                    align_self: AlignSelf::FlexEnd,
                    position_type: PositionType::Absolute,
                    position: Rect {
                        top: Val::Px(30.0),
                        left: Val::Px(15.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                text: Text::with_section(
                    "0",
                    TextStyle {
                        font,
                        font_size: 30.0,
                        color: Color::YELLOW,
                    },
                    Default::default(),
                ),
                ..Default::default()
            })
            .insert(MemCounterTag);
    }

    pub(super) fn update_mem_text_system(mut q: Query<&mut Text, With<MemCounterTag>>) {
        if let Ok(mut t) = q.get_single_mut() {
            let count = chunk::ALLOC_COUNT.load(std::sync::atomic::Ordering::Relaxed) as f32;
            let size = std::mem::size_of::<chunk::ChunkKind>() as f32;
            t.sections[0].value = format!("{:.2}mb", (count * size) / 1024.0 / 1024.0);
        }
    }
}
