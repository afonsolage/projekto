#![allow(unused)]
use bevy::{
    ecs::resource::Resource,
    prelude::*,
    tasks::{Task, TaskPool, TaskPoolBuilder, block_on, poll_once},
};
use projekto_core::{
    chunk::{Chunk, ChunkStorage},
    voxel::{Kind, Light},
};
//?
use thiserror::Error;

use crate::genesis::noise::Noise;

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
}

impl GenesisServer {
    pub fn new(num_threads: usize) -> Self {
        let task_pool = TaskPoolBuilder::new()
            .num_threads(num_threads)
            .thread_name("Genesis Server".to_string())
            .build();

        Self { task_pool }
    }

    pub fn generate(&self, chunk: Chunk) -> GenesisTask {
        let task = self.task_pool.spawn(async move {
            let mut noise = Noise::new();
            let mut kind = ChunkStorage::default();
            super::generate_chunk(&noise, chunk, &mut kind);

            let mut light = ChunkStorage::default();
            super::init_light(chunk, &kind, &mut light);

            Ok(ChunkCreation { kind, light })
        });

        GenesisTask(chunk, task)
    }
}
