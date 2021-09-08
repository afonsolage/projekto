# Projekto

Projekto is a voxel game made with [Bevy Engine](https://github.com/bevyengine/bevy).


# Overview

This is a general overview of the aspects of the game. Some of these aspects may not be implemented yet or may change on the future.

### World Architecture

- **World** -- Holds all logic and data related to the physical world
  - **Storage** - Methods and data related to storage
    - **Voxel** - Represents a single voxel. Usually it's opaque and has no data, only functions
    - **Chunk** - A 3d grid containing voxels backed by a single dimension array
    - **Landscape** - Contains all visible chunks
    - **World** - Logical world data storage which holds all voxel persistent data
  - **Query** -- Methods and utilities to query world state
    - **Raycast** -- Projects a ray and check for intersection
  - **Manipulation** - Commands to manipulate the world 
    - **Set Voxel** - Set a voxel value in a given point on the world
    - **Spawn Chunk**  - Spawns a chunk in a given position
    - **Despawn Chunk** - Despawn chunk in a given position
  <!--- **Propagation** - Any computation task that needs propagate some value over the world
    - **Light** - Propagates sun and artificial light over the world
    - **Water** - Propagates water over the world
    - **Fire** - Propagates fire over the world
    - **Physics** - Propagate physics behavior, like structures collapse, over the world -->
  - **Rendering** - Steps to render the final visible chunk
    - **Faces Occlusion** - Hides chunks and voxels that doesn't needs to be rendered
    - **Ambient Occlusion** - Computes the AO of each face
    - **Faces Merge** - Merge faces with the same properties to reduce the number of vertices
    - **Vertex Computation** - Computes the vertices for all visible and merges faces 
    - **Mesh Generation** - Generates the final mesh using the computed vertices

## Current Pipeline Stages

1. **World Manipulation**
    1. **Add Chunks** - Process *CmdChunkAdd*, add new chunk to world and raises an *EvtChunkAdded*
    2. **Remove Chunks** - Process *CmdChunkRemove*, remove chunk from world and raises an *EvtChunkRemoved*
    3. **Update Chunks** - Process *CmdChunkUpdate*, update chunk and raises and *EvtChunkUpdated*
2. **Entity Managing**
    1. **Despawn chunks** - Watches for *EvtChunkRemoved* event and completely despawn a chunk entity
    2. **Spawn chunks** - Watches for *EvtChunkAdded* event and spawn new chunk entities and raises a *ChunkDirty* to the pipeline
    3. **Update chunks** - Watches for *EvtChunkUpdated* event and raises a *ChunkDirty* event to the pipeline
3. **Rendering**
    1. **Faces Occlusion** - Process *EvtChunkDirty* events and updates the `ChunkFacesOcclusion` component
    2. **Vertex Computation** - Process *EvtChunkDirty* events and updates the `ChunkVertices`component
    3. **Mesh Generation** - Process *EvtChunkDirty* events and generates updates the `Handle<Mesh>` component
    4. **Clean up** - Process *EvtChunkDirty* events and remove `ChunkBuildingBundle` components


# License
[MIT](https://choosealicense.com/licenses/mit/)