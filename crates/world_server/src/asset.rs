use bevy::{
    asset::{
        io::{
            AssetReader, AssetReaderError, AssetSource, AssetSourceBuilder, AssetSourceBuilders,
            AssetWriter, PathStream, Reader,
        },
        AssetLoader, LoadContext,
    },
    prelude::*,
    utils::BoxedFuture,
};
use projekto_core::{
    chunk::{Chunk, ChunkStorage},
    voxel,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) struct ChunkAssetPlugin;

impl Plugin for ChunkAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ChunkAsset>()
            .init_asset_loader::<ChunkAssetLoader>();
    }
}

pub fn setup_chunk_asset_loader(app: &mut App) {
    if app.is_plugin_added::<AssetPlugin>() {
        panic!("ChunkAssetPlugin must be added before AssetPlugin");
    }
    let mut sources_builder = app
        .world
        .get_resource_or_insert_with::<AssetSourceBuilders>(Default::default);

    let (sender, receiver) = async_channel::unbounded::<async_channel::Sender<()>>();

    let source = AssetSourceBuilder::default()
        .with_reader(move || Box::new(ChunkAssetReader::new(sender.clone())));
    sources_builder.insert("chunk", source);

    app.world.insert_resource(ChunkAssetGenReceiver(receiver));
}

#[derive(Resource)]
pub(crate) struct ChunkAssetGenReceiver(pub async_channel::Receiver<async_channel::Sender<()>>);

#[derive(Asset, Default, TypePath, Serialize, Deserialize)]
pub(crate) struct ChunkAsset {
    pub chunk: Chunk,
    pub kind: ChunkStorage<voxel::Kind>,
    pub light: ChunkStorage<voxel::Light>,
    pub occlusion: ChunkStorage<voxel::FacesOcclusion>,
    pub soft_light: ChunkStorage<voxel::FacesSoftLight>,
    pub vertex: Vec<voxel::Vertex>,
}

#[derive(Default)]
struct ChunkAssetLoader;

#[derive(Debug, Error)]
enum ChunkAssetLoaderError {
    #[error("Failed to generate chunk. Error with generation channel.")]
    GenChannel,
    #[error("Could not load chunk. Error: {0}")]
    Io(#[from] std::io::Error),
}

impl AssetLoader for ChunkAssetLoader {
    type Asset = ChunkAsset;

    type Settings = ();

    type Error = ChunkAssetLoaderError;

    fn load<'a>(
        &'a self,
        _reader: &'a mut Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        todo!()
    }

    fn extensions(&self) -> &[&str] {
        &["cnk"]
    }
}

struct ChunkAssetReader {
    sender: async_channel::Sender<async_channel::Sender<()>>,
    reader: Box<dyn AssetReader>,
    writer: Box<dyn AssetWriter>,
}

impl ChunkAssetReader {
    fn new(sender: async_channel::Sender<async_channel::Sender<()>>) -> Self {
        Self {
            sender,
            reader: AssetSource::get_default_reader("chunks".to_string())(),
            writer: AssetSource::get_default_writer("chunks".to_string())().unwrap(),
        }
    }

    async fn generate<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> Result<Box<Reader<'a>>, AssetReaderError> {
        let (s, r) = async_channel::unbounded();
        self.sender.try_send(s).unwrap();
        r.recv().await.unwrap();
        self.reader.read(path).await
    }
}

impl AssetReader for ChunkAssetReader {
    fn read<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> BoxedFuture<'a, Result<Box<Reader<'a>>, AssetReaderError>> {
        Box::pin(async {
            let result = self.reader.read(path).await;
            match result {
                Err(AssetReaderError::NotFound(_)) => self.generate(path).await,
                _ => result,
            }
        })
    }

    fn read_meta<'a>(
        &'a self,
        _path: &'a std::path::Path,
    ) -> BoxedFuture<'a, Result<Box<Reader<'a>>, AssetReaderError>> {
        todo!()
    }

    fn read_directory<'a>(
        &'a self,
        _path: &'a std::path::Path,
    ) -> BoxedFuture<'a, Result<Box<PathStream>, AssetReaderError>> {
        todo!("Implement this as regions or make each X coordinates a directory?");
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a std::path::Path,
    ) -> BoxedFuture<'a, Result<bool, AssetReaderError>> {
        todo!()
    }
}
