use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
    time::FixedTimestep,
};
use projekto_widgets::widget::WidgetPlugin;

use self::console::ConsolePlugin;

// use bevy_egui::{egui, EguiContext, EguiPlugin};

// use crate::world::DebugCmd;

mod console;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WidgetPlugin)
            .add_plugin(ConsolePlugin)
            // .add_plugin(EguiPlugin)
            .add_plugin(FrameTimeDiagnosticsPlugin::default())
            .add_startup_system(setup_fps_text)
            // .add_startup_system(setup_meshing_text)
            // .add_system(cmd_window)
            .add_system_set(
                SystemSet::new()
                    .with_run_criteria(FixedTimestep::step(0.5))
                    // .with_system(show_chunk_material_clip_map)
                    .with_system(update_fps_text_system),
            );

        #[cfg(feature = "mem_alloc")]
        app.add_startup_system(mem_alloc::setup_mem_text)
            .add_system(
                mem_alloc::update_mem_text_system.with_run_criteria(FixedTimestep::step(0.5)),
            );
    }
}

#[derive(Component)]
struct FpsCounterTag;
fn setup_fps_text(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/FiraSans-Bold.ttf");

    commands
        .spawn(TextBundle {
            style: Style {
                align_self: AlignSelf::FlexEnd,
                position_type: PositionType::Absolute,
                position: UiRect {
                    top: Val::Px(5.0),
                    left: Val::Px(15.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            text: Text::from_section(
                "0",
                TextStyle {
                    font,
                    font_size: 25.0,
                    color: Color::YELLOW,
                },
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
                t.sections[0].value = format!("{avg:.0}");
            }
        }
    }
}

// fn show_chunk_material_clip_map(
//     mut commands: Commands,
//     clip_map: Res<ChunkMaterialImage>,
//     mut images: ResMut<Assets<Image>>,
//     mut meta: Local<Option<Entity>>,
// ) {
//     if meta.is_none() {
//         *meta = Some(
//             commands
//                 .spawn(ImageBundle {
//                     style: Style {
//                         position: UiRect {
//                             bottom: Val::Px(20.0),
//                             left: Val::Px(15.0),
//                             ..Default::default()
//                         },
//                         size: Size::new(Val::Px(200.0), Val::Px(200.0)),
//                         position_type: PositionType::Absolute,
//                         ..default()
//                     },

//                     ..default()
//                 })
//                 .insert(Name::new("Clip Map"))
//                 .id(),
//         );
//     }

//     const WIDTH: usize = landscape::HORIZONTAL_SIZE * chunk::X_AXIS_SIZE;
//     const HEIGHT: usize = landscape::HORIZONTAL_SIZE * chunk::Z_AXIS_SIZE;

//     let clip_map_data = match images.get(&clip_map) {
//         Some(img) => img.data.clone(),
//         None => return,
//     };

//     let mut data = vec![];

//     for b in clip_map_data {
//         let b = if b > 0 { 255 } else { b };

//         data.push(b);
//         data.push(b);
//         data.push(b);
//         data.push(255);
//     }

//     let is_visible = data.iter().max().unwrap_or(&0u8) > &0u8;

//     if !is_visible {
//         commands
//             .entity(meta.unwrap())
//             .insert(Visibility { is_visible });
//     } else {
//         let image = Image::new(
//             Extent3d {
//                 width: WIDTH as u32,
//                 height: HEIGHT as u32,
//                 ..Default::default()
//             },
//             TextureDimension::D2,
//             data,
//             TextureFormat::Bgra8UnormSrgb,
//         );

//         commands
//             .entity(meta.unwrap())
//             .insert(UiImage(images.add(image)));
//     }
// }

#[cfg(feature = "mem_alloc")]
mod mem_alloc {
    use super::*;
    use crate::world::storage::chunk;

    pub(super) struct MemCounterTag;
    pub(super) fn setup_mem_text(mut commands: Commands, asset_server: Res<AssetServer>) {
        let font = asset_server.load("fonts/FiraSans-Bold.ttf");

        commands.spawn(UiCameraBundle::default());
        commands
            .spawn(TextBundle {
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
