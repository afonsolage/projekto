use bevy::prelude::*;
use projekto_core::{chunk::Chunk, voxel};

#[derive(Component, Default, Debug, Clone, Copy, Deref, DerefMut)]
pub struct ChunkLocal(pub Chunk);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
pub struct ChunkVertex(pub Vec<voxel::Vertex>);
