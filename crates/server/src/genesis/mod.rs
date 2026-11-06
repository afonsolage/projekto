mod server;
mod terrain;

use projekto_core::{
    chunk::{self, ChunkStorage},
    coords::Chunk,
    voxel::{self},
};
pub use server::{ChunkCreation, GenesisError, GenesisServer, GenesisTask};
pub use terrain::{WorldGenConfig, WorldGenConfigError};

use crate::light;

pub(crate) fn init_light(
    _chunk: Chunk,
    chunk_kind: &ChunkStorage<voxel::Kind>,
    chunk_light: &mut ChunkStorage<voxel::Light>,
) {
    // Set sun light on top voxels
    chunk::top_voxels()
        .filter(|&voxel| !chunk_kind.get(voxel).is_opaque())
        .for_each(|voxel| {
            chunk_light.set_type(
                voxel,
                voxel::LightTy::Natural,
                voxel::Light::MAX_NATURAL_INTENSITY,
            );
        });

    chunk_light.pack();

    let _neighbor_propagation = light::propagate(
        chunk_kind,
        chunk_light,
        voxel::LightTy::Natural,
        chunk::top_voxels(),
    );

    // TODO: Emit light propagation events
}
