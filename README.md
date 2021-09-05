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

1. **Begin**
    1. **Despawn chunks** - Watches for *ChunkRemoved* event and completely despawn a chunk entity
    2. **Spawn chunks** - Watches for *ChunkAdded* event and spawn new chunk entities and raises a *ChunkDirty* to the pipeline
    3. **Update chunks** - Watches for *ChunkUpdated* event and raises a *ChunkDirty* event to the pipeline
2. **Rendering**
    1. **Faces Occlusion** - Process *ChunkDirty* events and updates the `FacesOcclusion` component
    2. **Vertex Computation** - Process *ChunkDirty* events and updates the `Vertices`component
    3. **Mesh Generation** - Process *ChunkDirty* events and generates updates the `Handle<Mesh>` component
    4. **Cleanup** - Process *ChunkDirty* events and remove all temporary components
3. **End**


# License
[MIT](https://choosealicense.com/licenses/mit/)