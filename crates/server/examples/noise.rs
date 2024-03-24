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
    utils::HashMap,
};
use bevy_inspector_egui::{
    inspector_options::ReflectInspectorOptions,
    quick::{ResourceInspectorPlugin, WorldInspectorPlugin},
    InspectorOptions,
};
use noise::{utils::NoiseMapBuilder, NoiseFn};
use projekto_server::gen::noise::{NoiseFnSpec, NoiseStack};

fn main() {
    App::new()
        .init_resource::<NoiseStackRes>()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            WorldInspectorPlugin::new(),
            ResourceInspectorPlugin::<NoiseStackRes>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                bevy::window::close_on_esc,
                update_noise_images.run_if(resource_changed::<NoiseStackRes>),
                update_noise_specs.run_if(resource_changed::<NoiseStackRes>),
                move_panel_node.run_if(on_event::<MouseMotion>()),
                zoom_root_node.run_if(on_event::<MouseWheel>()),
            ),
        )
        .run();
}

#[derive(Resource, Debug, Default, Reflect, InspectorOptions, Deref, DerefMut)]
#[reflect(Resource, InspectorOptions)]
struct NoiseStackRes(NoiseStack);

#[derive(Component, Debug, Default, Reflect)]
struct NoiseImage(String);

#[derive(Component, Debug, Default, Reflect)]
struct NoiseSpec(String);

#[derive(Component, Debug, Default)]
struct PanelNode;

#[derive(Component, Debug, Default)]
struct RootNode;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());

    let content = commands
        .spawn((
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
        ))
        .id();

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
        .add_child(content);

    let stack = create_terrain_stack();
    add_noise_ui_dependency_tree(content, 0.0, 0.0, "main", &stack, &mut commands);

    commands.insert_resource(NoiseStackRes(stack));
}

fn add_noise_ui_dependency_tree(
    parent: Entity,
    x: f32,
    y: f32,
    name: impl ToString,
    stack: &NoiseStack,
    commands: &mut Commands,
) {
    let card_width = 512.0;
    let card_height = 300.0;

    let name = name.to_string();
    commands.entity(parent).with_children(|parent| {
        parent
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        width: Val::Auto,
                        height: Val::Auto,
                        left: Val::Px(card_width * x),
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
                        background_color: Color::WHITE.into(),
                        ..Default::default()
                    },
                    Name::new("Noise Image"),
                    NoiseImage(name.to_string()),
                ));
            });
    });

    let dependencies = stack.get_spec(&name).unwrap().dependencies();
    let dep_count = dependencies.len() as f32;

    for (i, dependency) in dependencies.into_iter().enumerate() {
        let x = if dep_count > 1.0 {
            x + i as f32 - (dep_count / 4.0)
        } else {
            x + i as f32
        };
        add_noise_ui_dependency_tree(parent, x, y + 1.0, dependency, stack, commands);
    }
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
    q: Query<(Entity, &NoiseSpec), With<NoiseSpec>>,
    settings: Res<NoiseStackRes>,
) {
    for (entity, NoiseSpec(name)) in &q {
        commands.entity(entity).despawn_descendants();

        let spec = settings.get_spec(name).unwrap();

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
    q: Query<(Entity, &NoiseImage), With<NoiseImage>>,
    mut images: ResMut<Assets<Image>>,
    settings: Res<NoiseStackRes>,
) {
    info!("Updating noise images!");

    for (entity, NoiseImage(name)) in &q {
        let started = std::time::Instant::now();

        let noise = settings.build(name);

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

                b.copy_from_slice(&[height, height, height, 255u8]);
            }
        }

        let handle = images.add(create_image(512, 512, buffer));

        commands.entity(entity).insert(UiImage::new(handle));
        let diff = std::time::Instant::now() - started;
        info!("{name} took {}ms", diff.as_millis());
    }

    info!("Updated!");
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

pub fn create_terrain_stack() -> NoiseStack {
    // TODO: Move this to a ron file
    let mut map = HashMap::new();

    let continent = NoiseFnSpec::Fbm {
        seed: 42,
        frequency: 1.0,
        octaves: 14,
        lacunarity: 2.2089,
        persistence: 0.5,
    };
    map.insert("continent".to_string(), continent);

    let curve = NoiseFnSpec::Curve {
        source: "continent".to_string(),
        control_points: vec![
            (-2.0000, -1.625),
            (-1.0000, -1.375),
            (0.0000, -0.375),
            (0.0625, 0.125),
            (0.1250, 0.250),
            (0.2500, 1.000),
            (0.5000, 0.250),
            (0.7500, 0.250),
            (1.0000, 0.500),
            (2.0000, 0.500),
        ],
    };
    map.insert("curve".to_string(), curve);

    let carver = NoiseFnSpec::Fbm {
        seed: 42,
        frequency: 4.3437,
        octaves: 11,
        lacunarity: 2.2089,
        persistence: 0.5,
    };
    map.insert("carver".to_string(), carver);

    let scaled_carver = NoiseFnSpec::ScaleBias {
        source: "carver".to_string(),
        scale: 0.375,
        bias: 0.625,
    };
    map.insert("scaled_carver".to_string(), scaled_carver);

    let carved_continent = NoiseFnSpec::Min {
        source_1: "scaled_carver".to_string(),
        source_2: "curve".to_string(),
    };
    map.insert("carved_continent".to_string(), carved_continent);

    let clamp = NoiseFnSpec::Clamp {
        source: "carved_continent".to_string(),
        bounds: (-1.0, 1.0),
    };
    map.insert("main".to_string(), clamp);

    NoiseStack::new(map)
}
