use std::path::PathBuf;

use bevy::{math::IVec2, platform::collections::HashMap, prelude::*, tasks::AsyncComputeTaskPool};
use projekto_core::chunk::Chunk;

use crate::{ChunkAsset, archive::Archive, set::ChunkLoad};

#[derive(Resource, Deref, DerefMut)]
pub struct ArchiveServer(HashMap<Chunk, Archive<ChunkAsset>>);

impl ArchiveServer {}

fn load_archive(
    mut commands: Commands,
    mut reader: EventReader<ChunkLoad>,
    archive_map: ResMut<ArchiveServer>,
    task_pool: AsyncComputeTaskPool,
) {
    for &ChunkLoad(chunk) in reader.read() {
        if archive_map.contains_key(&chunk) {
            warn!("Trying to load a chunk that already exists: {chunk:?}");
            continue;
        }
    }
}
