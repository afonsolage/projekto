use std::sync::Arc;

use async_channel::Sender;
use async_lock::OnceCell;
use bevy::{
    asset::{
        io::{
            AssetReader, AssetReaderError, AssetSource, AssetSourceBuilder, AssetSourceBuilders,
            AssetWriter, PathStream, Reader, VecReader,
        },
        AssetLoader, AsyncReadExt, LoadContext,
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

use crate::{app::RunAsync, gen};

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

    let (sender, receiver) = async_channel::unbounded();

    app.world
        .get_resource_or_insert_with::<AssetSourceBuilders>(Default::default)
        .insert(
            "chunk",
            AssetSourceBuilder::default()
                .with_reader(move || Box::new(ChunkAssetReader::new(sender.clone()))),
        );

    trace!("Chunk asset source was added.");

    gen::create(receiver).run_async();
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkAssetGenRequest {
    pub chunk: Chunk,
    cell: Arc<OnceCell<Result<Vec<u8>, ()>>>,
}

impl ChunkAssetGenRequest {
    fn new(path: &std::path::Path) -> Self {
        Self {
            chunk: Chunk::from_path(path),
            cell: Arc::new(OnceCell::new()),
        }
    }

    async fn get_result(self) -> Result<Vec<u8>, ()> {
        let _ = self.cell.wait().await;
        Arc::into_inner(self.cell)
            .expect("To have only a single ref to cell.")
            .into_inner()
            .expect("To be initialized")
    }

    pub(crate) fn finish(self, result: Result<Vec<u8>, ()>) {
        self.cell
            .set_blocking(result)
            .expect("Cell to not be initialized yet.");
    }
}

#[derive(Asset, Default, Debug, TypePath, Serialize, Deserialize)]
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
    #[error("Failed to deserialize chunk. Error: {0}")]
    Deserialize(#[from] bincode::Error),
    #[error("Could not load chunk. Error: {0}")]
    Io(#[from] std::io::Error),
}

impl AssetLoader for ChunkAssetLoader {
    type Asset = ChunkAsset;

    type Settings = ();

    type Error = ChunkAssetLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        // TODO: Get the exact size from .meta file
        Box::pin(async move {
            trace!("[AssetLoader] loading asset");
            let mut bytes = vec![0; 1024];

            reader.read_to_end(&mut bytes).await?;
            let asset = bincode::deserialize::<ChunkAsset>(&bytes)?;

            Ok(asset)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["cnk"]
    }
}

struct ChunkAssetReader {
    sender: Sender<ChunkAssetGenRequest>,
    reader: Box<dyn AssetReader>,
    _writer: Box<dyn AssetWriter>,
}

impl ChunkAssetReader {
    fn new(sender: Sender<ChunkAssetGenRequest>) -> Self {
        let create_root = true;
        Self {
            sender,
            reader: AssetSource::get_default_reader("chunks".to_string())(),
            _writer: AssetSource::get_default_writer("chunks".to_string())(create_root).unwrap(),
        }
    }

    async fn generate<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> Result<Box<Reader<'a>>, AssetReaderError> {
        trace!("Chunk asset {path:?} not found local. Requesting to generate it.");

        let request = ChunkAssetGenRequest::new(path);
        self.sender.try_send(request.clone()).unwrap();

        if let Ok(bytes) = request.get_result().await {
            // if let Err(err) = self.writer.write_bytes(path, &bytes).await {
            //     error!("Failed to save chunk {path:?} to disk: {err}");
            // }
            Ok(Box::new(VecReader::new(bytes)))
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
        Box::pin(async move {
            trace!("Loading chunk at {path:?}");
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
