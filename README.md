# Projekto

Projekto is a voxel game made with [Bevy Engine](https://github.com/bevyengine/bevy).


# Overview

This is a general overview of the aspects of the game. Some of these aspects may not be implemented yet or may change on the future.

## Terminlogy ###
- **Voxel** - A volume pixel, represents a point or a cube in a 3D Space. 
- **Local** - Local position of voxel or chunk. If used in a voxel context it means the local position of the voxel inside the Chunk. If used in a chunk context, it means the local position of the chunk inside the VoxWorld.
- **Kind** - Indicates what kind of voxel it is (Air, Grass, Dirt, etc)
- **Side** - Points to a direction, doesn't contain any data.
- **Face** - Refers to a face of a cubic voxel. Each voxel has 6 faces, one for each side

## World Data Structure
Those are the main data structures used by world. There are others data structures but they are not bedrock like these:

- **[VoxWorld](https://github.com/afonsolage/projekto/blob/main/src/world/storage/voxworld.rs)** - Holds all chunks inside a map indexed by chunk local position.
- **[Chunk](https://github.com/afonsolage/projekto/blob/main/src/world/storage/chunk.rs)** - Group voxels together to batch the data transformation. Due to cache friendliness, each voxel data is stored in it's own storage, since usually transformations uses individually those storages.
- **[ChunkStorage](https://github.com/afonsolage/projekto/blob/main/src/world/storage/chunk.rs)** - A general purpose data structure which stores data in a one dimensional array but can be accessed using voxel locals.
- **[ChunkNeighborhood](https://github.com/afonsolage/projekto/blob/main/src/world/storage/chunk.rs)** - Used by *ChunkStorage* to cache neighborhood data, to avoid querying for neighbor chunks.
- **[ChunkStorageType](https://github.com/afonsolage/projekto/blob/main/src/world/storage/chunk.rs)** - A trait that must be implemented by any value stored inside *ChunkStorage*.
- **[ChunkKind](https://github.com/afonsolage/projekto/blob/main/src/world/storage/chunk.rs)** - A *ChunkStorage* which contains voxel *Kind*.
- **[ChunkLight](https://github.com/afonsolage/projekto/blob/main/src/world/storage/chunk.rs)** - A *ChunkStorage* which contains voxel *Light*.

## Terraformation

Terraformation is performed in a separated thread in order to avoid FPS dropping.

1. *[Genesis](https://github.com/afonsolage/projekto/blob/main/src/world/terraformation/genesis.rs)*
 - Manages chunk loading/saving/generation and reprocessing.
 - Run in batches, only one at a time, using the following logic flow:
    1. `optimize_commands` removing duplicated commands or invalidated commands, like load and unload the same chunk.
    2. `unload_chunks` from world.
    3. `load_chunks` from persistent cache.
       1. If cache doesn't exists `generate_chunk`
    4. `update_chunks` by placing or removing voxels.
    5. `recompute_chunks` internal state, like light, occlusion, vertices and so on.
       1. `build_kind_neighborhood` of all chunks in a single pass, since this is required by other steps.
       2. `propagate_light` on all chunks using a [BFS flood-fill algorithm](https://en.wikipedia.org/wiki/Flood_fill).
       3. `build_light_neighborhood` after all light was propagated.
       4. compute `faces_occlusion` of chunks.
       5. `merge_faces` of chunks which aren't `is_fully_occluded`.
       6. `generate_vertices` using the merged faces.
       7. `save_chunk` on persistent cache if the chunk was updated.

2. *[Landscaping](https://github.com/afonsolage/projekto/blob/main/src/world/terraformation/landscaping.rs)* - Manages which chunks should be loaded/unloaded into/from the world.
3. *[Terraforming](https://github.com/afonsolage/projekto/blob/main/src/world/terraformation/terraforming.rs)* - Handles queries and voxel update commands.

## Demo

[projekto_220730.webm](https://user-images.githubusercontent.com/1176452/181909251-cec6fe30-8a55-4107-8884-57cdc341919d.webm)


# License
[MIT](https://choosealicense.com/licenses/mit/)
