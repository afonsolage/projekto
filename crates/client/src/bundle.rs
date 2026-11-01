use bevy::prelude::*;
use projekto_core::coords::Chunk;

#[derive(Component, Default, Debug, Clone, Copy, Deref, DerefMut)]
pub struct ChunkLocal(pub Chunk);
