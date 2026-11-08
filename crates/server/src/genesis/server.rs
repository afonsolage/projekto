#![allow(unused)]
use bevy::{
    ecs::resource::Resource,
    prelude::*,
    tasks::{Task, TaskPool, TaskPoolBuilder, block_on, poll_once},
};
use projekto_core::{
    chunk::ChunkStorage,
    coords::Chunk,
    voxel::{Kind, Light},
};
//?
use thiserror::Error;

use crate::genesis::terrain::{TerrainGenerator, WorldGenConfig};

#[derive(Debug, Error)]
pub enum GenesisError {}

pub struct ChunkCreation {
    pub kind: ChunkStorage<Kind>,
    pub light: ChunkStorage<Light>,
}

pub struct GenesisTask(Chunk, Task<Result<ChunkCreation, GenesisError>>);

impl GenesisTask {
    pub fn try_get_result(&mut self) -> Option<Result<ChunkCreation, GenesisError>> {
        block_on(poll_once(&mut self.1))
    }

    pub fn chunk(&self) -> Chunk {
        self.0
    }
}

#[derive(Resource)]
pub struct GenesisServer {
    task_pool: TaskPool,
    config: WorldGenConfig,
}

impl GenesisServer {
    /// Creates a server using `assets/worldgen.ron`.
    pub fn new(num_threads: usize) -> Self {
        Self::with_config(num_threads, WorldGenConfig::load_default())
    }

    /// Creates a server with an explicit configuration, mainly for tests and tools.
    pub fn with_config(num_threads: usize, config: WorldGenConfig) -> Self {
        let task_pool = TaskPoolBuilder::new()
            .num_threads(num_threads)
            .thread_name("Genesis Server".to_string())
            .build();

        Self { task_pool, config }
    }

    /// Returns the world seed every client must use to generate its terrain textures.
    pub fn seed(&self) -> u64 {
        self.config.seed
    }

    pub fn generate(&self, chunk: Chunk) -> GenesisTask {
        let config = self.config.clone();
        let task = self.task_pool.spawn(async move {
            let generator = TerrainGenerator::new(config);
            let mut kind = ChunkStorage::default();
            generator.generate(chunk, &mut kind);

            let mut light = ChunkStorage::default();
            super::init_light(chunk, &kind, &mut light);

            Ok(ChunkCreation { kind, light })
        });

        GenesisTask(chunk, task)
    }
}
