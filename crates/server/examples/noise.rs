use bevy::{
    ecs::system::EntityCommands,
    input::mouse::MouseMotion,
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
use bracket_noise::prelude::{FastNoise, FractalType};
use noise::NoiseFn;

fn main() {
    App::new()
        .init_resource::<Noises>()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            WorldInspectorPlugin::new(),
            ResourceInspectorPlugin::<Noises>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                bevy::window::close_on_esc,
                update_noise.run_if(resource_changed::<Noises>),
            ),
        )
        .run();
}

#[derive(Reflect, Default, PartialEq, Eq, Hash)]
enum NoiseType {
    #[default]
    Continentalness,
    Erosion,
}

#[derive(Reflect, InspectorOptions, Default)]
struct NoiseSettings {
    seed: u64,
    size: UVec2,
    frequency: f32,
    fractal_octaves: i32,
    fractal_gain: f32,
    fractal_lacunarity: f32,
    curve: Vec<(f32, u16)>,
    noise: Handle<Image>,
}

#[derive(Resource, Reflect, InspectorOptions, Deref, DerefMut)]
#[reflect(Resource, InspectorOptions)]
struct Noises(HashMap<NoiseType, NoiseSettings>);

impl Default for Noises {
    fn default() -> Self {
        let mut map = HashMap::new();
        map.insert(
            NoiseType::Continentalness,
            NoiseSettings {
                seed: 42,
                size: UVec2::new(512, 512),
                frequency: 1.0,
                fractal_octaves: 14,
                fractal_gain: 0.5,
                fractal_lacunarity: 2.2089,
                curve: vec![(-1.0, 50), (0.3, 100), (0.4, 150), (1.0, 150)],
                noise: Handle::default(),
            },
        );

        Self(map)
    }
}

#[derive(Component, Debug, Default)]
struct NoiseOceanLandImage;

#[derive(Component, Debug, Default)]
struct ContentNode;

fn test_noise() -> impl noise::NoiseFn<f64, 3> {
    use noise::*;

    let mut v: Vec<Box<dyn NoiseFn<f64, 3>>> = vec![];

    let a = Fbm::<Perlin>::new(42);
    v.push(Box::new(a.clone()));

    let b = Curve::new(a)
        .add_control_point(-2.0, -1.625)
        .add_control_point(-1.0, -1.625)
        .add_control_point(0.0, -1.625)
        .add_control_point(1.0, -1.625);

    v.push(Box::new(b.clone()));

    let c = Fbm::<Perlin>::new(42);
    v.push(Box::new(c.clone()));

    let d = Min::new(c, b);
    v.push(Box::new(d.clone()));

    d
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());

    let c = test_noise();
    let n = c.get([0.0, 0.0, 0.0]);

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
            let mut entity_cmds = parent.spawn((
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

            add_noise_ui_node(&mut entity_cmds);
        });
}

fn add_noise_ui_node(entity_cmds: &mut EntityCommands) {
    entity_cmds.with_children(|parent| {
        parent
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
                ContentNode,
            ))
            .with_children(|parent| {
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
                            width: Val::Px(300.0),
                            height: Val::Px(300.0),
                            ..Default::default()
                        },
                        background_color: Color::WHITE.into(),
                        ..Default::default()
                    },
                    Name::new("Noise Settings"),
                    NoiseOceanLandImage,
                ));
            });
    });
}

fn update_noise(
    mut commands: Commands,
    q: Query<Entity, With<NoiseOceanLandImage>>,
    mut images: ResMut<Assets<Image>>,
    settings: Res<Noises>,
) {
    let Some(settings) = settings.get(&NoiseType::Continentalness) else {
        return;
    };

    if settings.size.x == 0 || settings.size.y == 0 {
        return;
    }

    info!("Updating noise!");

    let Ok(entity) = q.get_single() else {
        return;
    };

    let mut noise = FastNoise::seeded(settings.seed);
    noise.set_noise_type(bracket_noise::prelude::NoiseType::SimplexFractal);
    noise.set_frequency(settings.frequency);
    noise.set_fractal_type(FractalType::FBM);
    noise.set_fractal_octaves(settings.fractal_octaves);
    noise.set_fractal_gain(settings.fractal_gain);
    noise.set_fractal_lacunarity(settings.fractal_lacunarity);

    // 512w, 512h, 1 byte per color, 4 color channel (RGBA)
    let mut buffer = vec![0; 4 * 512 * 512];

    let mut min = f32::MAX;
    let mut max = 0.0;

    for w in 0..512 {
        for h in 0..512 {
            let i = ((w * 512 + h) * 4) as usize;
            let b = &mut buffer[i..i + 4];

            let x = w as f32 / settings.size.x as f32;
            let y = h as f32 / settings.size.y as f32;
            let noise = noise.get_noise(x, y);

            if noise < min {
                min = noise;
            } else if noise > max {
                max = noise;
            }

            let height = (((noise + 1.0) / 2.0) * 255.0) as u8;

            b.copy_from_slice(&[height, height, height, height]);
        }
    }

    info!("Noise ({min}, {max})");

    let handle = images.add(create_image(512, 512, buffer));

    commands.entity(entity).insert(UiImage::new(handle));
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
