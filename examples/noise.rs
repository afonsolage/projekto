use std::{cell::RefCell, rc::Rc};

use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    reflect::{ReflectRef, VariantField},
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
        texture::ImageSampler,
    },
    tasks::{block_on, poll_once, AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use noise::NoiseFn;
use projekto_server::gen::noise::{NoiseFnSpec, NoiseStack, NoiseStackLoader};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .init_asset::<NoiseStack>()
        .init_asset_loader::<NoiseStackLoader>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                bevy::window::close_on_esc,
                update_noise_tree,
                update_noise_images,
                update_noise_specs,
                move_panel_node.run_if(on_event::<MouseMotion>()),
                zoom_root_node.run_if(on_event::<MouseWheel>()),
            ),
        )
        .run();
}

#[derive(Resource, Debug, Default)]
struct NoiseStackHandle(Handle<NoiseStack>);

#[derive(Component, Debug, Default, Reflect)]
struct NoiseImage(String);

#[derive(Component, Debug, Default, Reflect)]
struct NoiseSpec(String);

#[derive(Component, Debug, Default)]
struct PanelNode;

#[derive(Component, Debug, Default)]
struct RootNode;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2dBundle::default());
    commands
        .spawn((
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                ..Default::default()
            },
            Name::new("Root"),
            RootNode,
        ))
        .with_children(|parent| {
            parent.spawn((
                NodeBundle {
                    style: Style {
                        display: Display::Grid,
                        grid_template_columns: vec![
                            GridTrack::flex(1.0),
                            GridTrack::px(5.0),
                            GridTrack::min_content(),
                        ],
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        position_type: PositionType::Absolute,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Name::new("Panel"),
                PanelNode,
            ));
        });

    let stack = asset_server.load("noises/world_surface.ron");
    commands.insert_resource(NoiseStackHandle(stack));
}

fn update_noise_tree(
    mut commands: Commands,
    q_panel: Query<Entity, With<PanelNode>>,
    mut events: EventReader<AssetEvent<NoiseStack>>,
    assets: Res<Assets<NoiseStack>>,
) {
    let Ok(panel) = q_panel.get_single() else {
        return;
    };

    for evt in events.read() {
        match evt {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                commands.entity(panel).despawn_descendants();
                let Some(stack) = assets.get(*id) else {
                    continue;
                };

                spawn_noise_ui_dependency_tree(panel, 0.0, 0.0, 0.0, "main", stack, &mut commands);
            }
            _ => continue,
        }
    }
}

fn spawn_noise_ui_dependency_tree(
    parent: Entity,
    offset: f32,
    x: f32,
    y: f32,
    name: impl ToString,
    stack: &NoiseStack,
    commands: &mut Commands,
) {
    let card_width = 512.0;
    let card_height = 300.0;

    let tree = Tree::new(stack, "main");

    let name = name.to_string();
    commands.entity(parent).with_children(|parent| {
        parent
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        width: Val::Auto,
                        height: Val::Auto,
                        left: Val::Px(card_width * (x + offset)),
                        top: Val::Px(card_height * y),
                        display: Display::Grid,
                        grid_template_columns: vec![
                            GridTrack::flex(1.0),
                            GridTrack::px(5.0),
                            GridTrack::min_content(),
                        ],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Name::new(format!("Noise {name}").to_string()),
            ))
            .with_children(|parent| {
                parent.spawn((
                    NodeBundle {
                        style: Style {
                            width: Val::Px(200.0),
                            height: Val::Auto,
                            ..Default::default()
                        },
                        background_color: Color::BLACK.into(),
                        ..Default::default()
                    },
                    Name::new("Noise Spec"),
                    NoiseSpec(name.to_string()),
                ));

                parent.spawn((
                    NodeBundle {
                        style: Style {
                            height: Val::Auto,
                            ..Default::default()
                        },
                        background_color: Color::GRAY.into(),
                        ..Default::default()
                    },
                    Name::new("Noise Spacer"),
                ));

                parent.spawn((
                    NodeBundle {
                        style: Style {
                            width: Val::Px(256.0),
                            height: Val::Px(256.0),
                            ..Default::default()
                        },
                        background_color: Color::DARK_GRAY.into(),
                        ..Default::default()
                    },
                    Name::new("Noise Image"),
                    NoiseImage(name.to_string()),
                ));
            });
    });

    let dependencies = stack.get_spec(&name).unwrap().dependencies();
    let dep_count = dependencies.len() as f32;

    // // TODO: Compute offset
    // let mut offset = -compute_node_width(stack, &name) / 2.0;
    //
    // for (i, dependency) in dependencies.into_iter().enumerate() {
    //     // let x = if dep_count > 1.0 {
    //     //     x + i as f32 - (dep_count / 2.0 - 0.5)
    //     // } else {
    //     //     x + i as f32
    //     // };
    //
    //     spawn_noise_ui_dependency_tree(
    //         parent,
    //         offset,
    //         x + offset,
    //         y + 1.0,
    //         dependency,
    //         stack,
    //         commands,
    //     );
    //
    //     let dep_width = compute_node_width(stack, dependency);
    //     offset += dep_width;
    // }
}

fn zoom_root_node(
    mut mouse_wheel: EventReader<MouseWheel>,
    mut q: Query<&mut Transform, With<RootNode>>,
) {
    let Ok(ref mut transform) = q.get_single_mut() else {
        return;
    };

    for MouseWheel { y, .. } in mouse_wheel.read() {
        let scaled = transform.scale + y * 0.1;
        transform.scale = scaled.clamp(Vec3::splat(0.1), Vec3::splat(10.0));
    }
}

fn move_panel_node(
    mut q: Query<&mut Style, With<PanelNode>>,
    mut q_root: Query<&Transform, With<RootNode>>,
    input: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
) {
    let Ok(ref mut style) = q.get_single_mut() else {
        return;
    };

    let Ok(root_transform) = q_root.get_single_mut() else {
        return;
    };

    if input.pressed(MouseButton::Middle) {
        for &MouseMotion { delta } in mouse_motion.read() {
            let scale_factor = 1.0 / root_transform.scale.x;

            let left = -delta.x * scale_factor;
            let top = -delta.y * scale_factor;

            move_left_top(style, left, top);
        }
    }
}

fn move_left_top(style: &mut Style, left: f32, top: f32) {
    let Val::Px(current_left) = style.left else {
        unreachable!()
    };

    let Val::Px(current_top) = style.top else {
        unreachable!()
    };

    style.left = Val::Px(current_left - left);
    style.top = Val::Px(current_top - top);
}

fn update_noise_specs(
    mut commands: Commands,
    q: Query<(Entity, &NoiseSpec), Added<NoiseSpec>>,
    handle: Res<NoiseStackHandle>,
    assets: Res<Assets<NoiseStack>>,
) {
    let Some(stack) = assets.get(&handle.0) else {
        return;
    };

    for (entity, NoiseSpec(name)) in &q {
        commands.entity(entity).despawn_descendants();

        let spec = stack.get_spec(name).unwrap();

        create_spec_node(&mut commands, entity, name, spec);
    }
}

fn create_spec_node(commands: &mut Commands, parent: Entity, name: &str, spec: &NoiseFnSpec) {
    let panel = commands
        .spawn((
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    display: Display::Grid,
                    grid_template_columns: vec![GridTrack::auto()],
                    grid_template_rows: vec![GridTrack::min_content()],
                    margin: UiRect::all(Val::Px(3.0)),
                    align_self: AlignSelf::Start,
                    ..Default::default()
                },
                ..Default::default()
            },
            Name::new("Noise Spec Panel"),
        ))
        .with_children(|parent| {
            parent.spawn(TextBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    ..Default::default()
                },
                text: Text::from_section(
                    name,
                    TextStyle {
                        font_size: 20.0,
                        color: Color::WHITE,
                        ..Default::default()
                    },
                ),
                ..Default::default()
            });

            parent.spawn((
                NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        height: Val::Px(2.0),
                        ..Default::default()
                    },
                    background_color: Color::GRAY.into(),
                    ..Default::default()
                },
                Name::new("Spacer"),
            ));
        })
        .id();

    for item in create_spec_items(commands, spec) {
        commands.entity(panel).add_child(item);
    }

    commands.entity(parent).add_child(panel);
}

fn create_spec_items(commands: &mut Commands, spec: &NoiseFnSpec) -> Vec<Entity> {
    let ReflectRef::Enum(spec_ref) = spec.reflect_ref() else {
        unreachable!();
    };

    let type_item = commands
        .spawn(TextBundle {
            style: Style {
                display: Display::Grid,
                width: Val::Percent(100.0),
                ..Default::default()
            },
            text: Text::from_section(
                format!("type: {}", spec_ref.variant_name()),
                TextStyle {
                    ..Default::default()
                },
            ),
            ..Default::default()
        })
        .id();

    std::iter::once(type_item)
        .chain(
            spec_ref
                .iter_fields()
                .map(|field| create_spec_item_field(commands, field)),
        )
        .collect()
}

fn create_spec_item_field(commands: &mut Commands, field: VariantField) -> Entity {
    let name = field.name().unwrap();

    let mut sections = match field.value().reflect_ref() {
        ReflectRef::List(list) => list
            .iter()
            .map(|value| TextSection::new(format!("\n\t{:?}", value), TextStyle::default()))
            .collect(),
        _ => {
            vec![TextSection::new(
                format!("{:?}", field.value()),
                TextStyle::default(),
            )]
        }
    };

    sections.insert(
        0,
        TextSection::new(format!("{name}: "), TextStyle::default()),
    );

    commands
        .spawn(TextBundle {
            style: Style {
                display: Display::Grid,
                width: Val::Percent(100.0),
                ..Default::default()
            },
            text: Text::from_sections(sections),
            ..Default::default()
        })
        .id()
}

fn update_noise_images(
    mut commands: Commands,
    q: Query<(Entity, &NoiseImage), Added<NoiseImage>>,
    mut images: ResMut<Assets<Image>>,
    handle: Res<NoiseStackHandle>,
    assets: Res<Assets<NoiseStack>>,
    mut running_tasks: Local<HashMap<Entity, Task<Vec<u8>>>>,
) {
    for (entity, task) in running_tasks.iter_mut() {
        if let Some(buffer) = block_on(poll_once(task)) {
            let handle = images.add(create_image(512, 512, buffer));

            commands
                .entity(*entity)
                .insert((UiImage::new(handle), BackgroundColor(Color::WHITE)));
        }
    }

    running_tasks.retain(|_, task| !task.is_finished());

    let Some(stack) = assets.get(&handle.0) else {
        return;
    };

    for (entity, NoiseImage(name)) in &q {
        info!("Loading noise {name}");

        let noise = stack.build(name);

        let task = AsyncComputeTaskPool::get().spawn(async move {
            // 512w, 512h, 1 byte per color, 4 color channel (RGBA)
            let mut buffer = vec![0; 4 * 512 * 512];
            let mut min = u8::MAX;
            let mut max = 0;

            for w in 0..512 {
                for h in 0..512 {
                    let i = ((w * 512 + h) * 4) as usize;
                    let b = &mut buffer[i..i + 4];
                    let x = w as f64 / 512.0;
                    let y = h as f64 / 512.0;

                    let noise = noise.get([x, y, 0.0]) as f32;
                    let height = (((noise + 1.0) / 2.0) * 255.0) as u8;

                    if height > max {
                        max = height;
                    } else if height < min {
                        min = height;
                    }

                    b.copy_from_slice(&[height, height, height, u8::MAX]);
                }
            }

            buffer
        });

        running_tasks.insert(entity, task);
    }
}

fn create_image(width: u32, height: u32, buffer: Vec<u8>) -> Image {
    Image {
        data: buffer,
        texture_descriptor: TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                ..Default::default()
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            view_formats: &[],
            usage: TextureUsages::TEXTURE_BINDING,
        },
        sampler: ImageSampler::nearest(),
        asset_usage: RenderAssetUsages::RENDER_WORLD,
        ..Default::default()
    }
}

#[derive(Debug, Default)]
struct Tree<'n>(HashMap<&'n str, TreeNode<'n>>);

#[derive(Default, Debug, Clone)]
struct TreeNode<'n> {
    pub name: &'n str,
    pub x: f32,
    pub y: f32,
    pub children: Vec<&'n str>,
    pub parent: Option<&'n str>,
    x_offset: f32,
}

impl<'n> Tree<'n> {
    pub fn new(stack: &'n NoiseStack, root: &'n str) -> Tree<'n> {
        let mut tree = Tree::default();
        tree.add_spec_node(stack, root, None, 0);

        tree.compute_layout();

        tree
    }

    fn compute_layout(&mut self) {
        self.compute_local_x();
    }

    fn compute_local_x(&mut self) {}

    fn add_spec_node(
        &mut self,
        stack: &'n NoiseStack,
        name: &'n str,
        parent: Option<&'n str>,
        depth: u32,
    ) {
        let spec = stack.get_spec(name).unwrap();

        let children = spec
            .dependencies()
            .into_iter()
            .map(|dep| {
                self.add_spec_node(stack, dep, Some(dep), depth + 1);
                dep
            })
            .collect();

        let node = TreeNode {
            name,
            parent,
            y: depth as f32,
            children,
            ..Default::default()
        };

        if let Some(name) = self.0.insert(name, node) {
            // Derrota
        }
    }
}
