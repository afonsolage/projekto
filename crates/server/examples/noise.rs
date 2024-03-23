use bevy::{
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
    },
    utils::HashMap,
};
use bevy_inspector_egui::{
    inspector_options::ReflectInspectorOptions,
    quick::{ResourceInspectorPlugin, WorldInspectorPlugin},
    InspectorOptions,
};
use noise::NoiseFn;
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
struct NoiseConfig(String);

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
                    ..Default::default()
                },
                background_color: Color::DARK_GRAY.into(),
                ..Default::default()
            },
            Name::new("Content"),
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
        ))
        .with_children(|parent| {
            parent.spawn((
                NodeBundle {
                    style: Style {
                        width: Val::Auto,
                        height: Val::Auto,
                        border: UiRect::all(Val::Px(2.0)),
                        margin: UiRect::all(Val::Px(5.0)),
                        ..default()
                    },
                    border_color: Color::GRAY.into(),
                    background_color: Color::BLACK.into(),
                    ..default()
                },
                Name::new("Border"),
            ));
        })
        .add_child(content);

    let stack = create_terrain_stack();
    let mut dependencies = stack.build_dep_tree("main");
    dependencies.reverse();

    for dep in dependencies {
        add_noise_ui_node(content, dep, &mut commands);
    }

    commands.insert_resource(NoiseStackRes(stack));
}

fn add_noise_ui_node(parent: Entity, name: impl ToString, commands: &mut Commands) {
    commands.entity(parent).with_children(|parent| {
        parent.spawn((
            NodeBundle {
                style: Style {
                    width: Val::Auto,
                    height: Val::Auto,
                    ..Default::default()
                },
                background_color: Color::DARK_GREEN.into(),
                ..Default::default()
            },
            Name::new("Noise Settings"),
            NoiseConfig(name.to_string()),
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

        for _ in 0..3 {
            parent.spawn((
                NodeBundle {
                    style: Style {
                        height: Val::Px(2.0),
                        width: Val::Auto,
                        ..Default::default()
                    },
                    background_color: Color::GRAY.into(),
                    ..Default::default()
                },
                Name::new("Noise Div"),
            ));
        }
    });
}

fn update_noise_images(
    mut commands: Commands,
    q: Query<(Entity, &NoiseImage), With<NoiseImage>>,
    mut images: ResMut<Assets<Image>>,
    settings: Res<NoiseStackRes>,
) {
    info!("Updating noise images!");

    for (entity, NoiseImage(name)) in &q {
        info!("Building noise {name}");
        let noise = settings.build(name);
        info!("Built!");

        // 512w, 512h, 1 byte per color, 4 color channel (RGBA)
        let mut buffer = vec![0; 4 * 512 * 512];

        info!("Rendering!");
        for w in 0..512 {
            for h in 0..512 {
                let i = ((w * 512 + h) * 4) as usize;
                let b = &mut buffer[i..i + 4];

                let noise = noise.get([w as f64, 0.0, h as f64]) as f32;
                let height = (((noise + 1.0) / 2.0) * 255.0) as u8;

                b.copy_from_slice(&[height, height, height, height]);
            }
        }
        info!("Rendered!");

        let handle = images.add(create_image(512, 512, buffer));

        commands.entity(entity).insert(UiImage::new(handle));
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
