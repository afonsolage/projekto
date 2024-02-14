use std::sync::Arc;

use async_channel::{Receiver, Sender};
use async_lock::OnceCell;
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

    let (sender, receiver) = async_channel::unbounded();

    let source = AssetSourceBuilder::default()
        .with_reader(move || Box::new(ChunkAssetReader::new(sender.clone())));
    sources_builder.insert("chunk", source);

    app.world.insert_resource(ChunkAssetGenReceiver(receiver));
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkAssetGenRequest {
    pub chunk: Chunk,
    cell: Arc<OnceCell<Result<(), ()>>>,
}

impl ChunkAssetGenRequest {
    fn new(path: &std::path::Path) -> Self {
        Self {
            chunk: Chunk::from_path(path),
            cell: Arc::new(OnceCell::new()),
        }
    }

    async fn get_result(self) -> Result<(), ()> {
        Arc::into_inner(self.cell)
            .expect("To have only a single ref to cell.")
            .into_inner()
            .expect("To be initialized")
    }

    pub(crate) fn finish(self, result: Result<(), ()>) {
        self.cell
            .set_blocking(result)
            .expect("Cell to not be initialized yet.");
    }
}

#[derive(Resource)]
pub(crate) struct ChunkAssetGenReceiver(pub Receiver<ChunkAssetGenRequest>);

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
    sender: Sender<ChunkAssetGenRequest>,
    reader: Box<dyn AssetReader>,
    writer: Box<dyn AssetWriter>,
}

impl ChunkAssetReader {
    fn new(sender: Sender<ChunkAssetGenRequest>) -> Self {
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
        let request = ChunkAssetGenRequest::new(path);
        self.sender.try_send(request.clone()).unwrap();

        if request.get_result().await.is_ok() {
            self.reader.read(path).await
        } else {
            Err(AssetReaderError::NotFound(path.to_path_buf()))
        }
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
