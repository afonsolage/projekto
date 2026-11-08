//! Procedural terrain atlas: generation, world-seed handling and the chunk material.
//!
//! The client no longer ships or loads a terrain spritesheet. The atlas is synthesized from the
//! world seed announced by the server, so every connected client renders the same tiles without
//! transferring any image data. Palette conversion can differ by at most a few sRGB steps between
//! platforms, which is cosmetic. The seed arrives right after the connection is established; the
//! fallback seed only covers the few frames before that.

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use projekto_core::voxel::{self, KindSideTexture, KindSidesDesc};
use projekto_messages::ServerInfo;
use projekto_proto::RegisterMessageHandler;
use projekto_textures::{
    ATLAS_SIZE, ArtPreset, TextureStyle, generate_atlas, index_tile::TILE, materials,
};

use crate::ChunkMaterial;

/// Seed used on the first frames, before the server announces the world seed.
///
/// It matches the seed shipped in `assets/worldgen.ron`, so a client joining the default world
/// does not visibly switch textures once the handshake completes; other worlds regenerate once.
pub const FALLBACK_WORLD_SEED: u64 = 42;

/// World seed used to synthesize terrain textures.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSeed(pub u64);

impl Default for WorldSeed {
    fn default() -> Self {
        Self(FALLBACK_WORLD_SEED)
    }
}

/// Handle of the generated terrain atlas, shared with the debug materials.
#[derive(Resource, Debug, Clone)]
pub struct TerrainAtlas(pub Handle<Image>);

pub(crate) struct TexturesPlugin;

impl Plugin for TexturesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldSeed>()
            .add_message_handler(handle_server_info)
            .add_systems(PreStartup, setup_textures)
            .add_systems(Startup, setup_material)
            .add_systems(
                Update,
                regenerate_atlas.run_if(resource_changed::<WorldSeed>),
            );
    }
}

fn setup_textures(mut commands: Commands, mut images: ResMut<Assets<Image>>, seed: Res<WorldSeed>) {
    let kinds_path = format!("{}/voxels/kind.ron", env!("ASSETS_PATH"));
    let descs = voxel::KindsDescs::init(kinds_path);
    check_atlas_layout(descs);

    let atlas = images.add(atlas_image(seed.0));
    commands.insert_resource(TerrainAtlas(atlas));
}

fn setup_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    atlas: Res<TerrainAtlas>,
) {
    let material = materials.add(ChunkMaterial {
        texture: atlas.0.clone(),
        tile_texture_size: 1.0 / voxel::KindsDescs::get().count_tiles() as f32,
        show_back_faces: false,
    });

    commands.insert_resource(crate::ChunkMaterialHandle(material));
}

fn handle_server_info(In(info): In<ServerInfo>, mut commands: Commands) {
    commands.insert_resource(WorldSeed(info.seed));
}

/// Rewrites the atlas pixels in place when the world seed becomes known (or changes).
fn regenerate_atlas(
    seed: Res<WorldSeed>,
    atlas: Res<TerrainAtlas>,
    mut images: ResMut<Assets<Image>>,
) {
    if images.insert(&atlas.0, atlas_image(seed.0)).is_err() {
        warn!(
            "terrain atlas asset is missing; cannot apply world seed {}",
            seed.0
        );
        return;
    }

    info!("Generated terrain atlas for world seed {}", seed.0);
}

fn atlas_image(seed: u64) -> Image {
    let generated = generate_atlas(TextureStyle::Structural, ArtPreset::default(), seed);

    let mut image = Image::new(
        Extent3d {
            width: ATLAS_SIZE as u32,
            height: ATLAS_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        generated.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

/// Fails fast on a mismatch between `kind.ron` and the generator instead of sampling outside the
/// atlas or rendering an unintended tile.
fn check_atlas_layout(descs: &voxel::KindsDescs) {
    assert_eq!(
        descs.atlas_size as usize, ATLAS_SIZE,
        "kind.ron atlas_size must match projekto_textures::ATLAS_SIZE"
    );
    assert_eq!(
        descs.atlas_tile_size as usize, TILE,
        "kind.ron atlas_tile_size must match the generator tile size"
    );

    for desc in &descs.descriptions {
        match &desc.sides {
            KindSidesDesc::None => {}
            KindSidesDesc::All(texture) => check_offset(&desc.name, texture),
            KindSidesDesc::Unique {
                right,
                left,
                up,
                down,
                front,
                back,
            } => {
                for texture in [right, left, up, down, front, back] {
                    check_offset(&desc.name, texture);
                }
            }
        }
    }
}

fn check_offset(kind: &str, texture: &KindSideTexture) {
    let offset = (texture.offset.x as u32, texture.offset.y as u32);
    assert!(
        materials::MATERIALS
            .iter()
            .any(|material| material.offset == offset),
        "kind {kind:?} uses atlas offset {offset:?}, which the generator does not produce"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use projekto_messages::ServerMessage;
    use projekto_proto::{BoxedMessage, ClientId, RunMessageHandlers};
    use projekto_textures::index_tile::TILE;

    use crate::ChunkMaterial;

    fn shipped_descs() -> voxel::KindsDescs {
        let path = format!("{}/voxels/kind.ron", env!("ASSETS_PATH"));
        let file = std::fs::File::open(path).expect("kind.ron must exist");
        ron::de::from_reader(file).expect("kind.ron must parse")
    }

    #[test]
    fn shipped_kind_descriptors_match_the_generator_layout() {
        check_atlas_layout(&shipped_descs());
    }

    #[test]
    fn generated_atlas_has_the_configured_size() {
        let image = atlas_image(42);
        assert_eq!(image.width() as usize, ATLAS_SIZE);
        assert_eq!(image.height() as usize, ATLAS_SIZE);
        assert_eq!(
            image.data.as_ref().map(Vec::len),
            Some(ATLAS_SIZE * ATLAS_SIZE * 4)
        );
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
        assert!(matches!(image.sampler, ImageSampler::Descriptor(_)));
    }

    #[test]
    fn atlas_tile_size_matches_the_generator() {
        assert_eq!(shipped_descs().atlas_tile_size as usize, TILE);
    }

    #[test]
    fn server_info_regenerates_the_atlas_for_the_world_seed() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Assets<ChunkMaterial>>()
            .add_plugins(TexturesPlugin);
        app.update();

        let handle = app.world().resource::<TerrainAtlas>().0.clone();
        let before = atlas_bytes(&app, &handle);
        assert_eq!(app.world().resource::<WorldSeed>().0, FALLBACK_WORLD_SEED);

        // Dispatch through the same message plumbing the network client uses, so a broken
        // handler registration or change detection fails this test.
        let boxed: BoxedMessage<ServerMessage> = Box::new(ServerInfo { seed: 7 });
        app.world_mut()
            .run_handlers::<ServerInfo>(ClientId::default(), boxed);
        app.update();

        assert_eq!(app.world().resource::<WorldSeed>().0, 7);
        let after = atlas_bytes(&app, &handle);
        assert_ne!(
            before, after,
            "the atlas must be regenerated when the world seed arrives"
        );
    }

    fn atlas_bytes(app: &App, handle: &Handle<Image>) -> Vec<u8> {
        app.world()
            .resource::<Assets<Image>>()
            .get(handle)
            .expect("generated atlas exists")
            .data
            .clone()
            .expect("generated atlas keeps its CPU pixels")
    }
}
