# Projekto

Projekto is a voxel game made with [Bevy Engine](https://github.com/bevyengine/bevy).


# Overview

This is a general overview of the aspects of the game. Some of these aspects may not be implemented yet or may change on the future.

The project is currently in heavy redesign. The old project which has an async focus can be found [here](https://github.com/afonsolage/projekto/tree/async_io).
The new design will try to is best to use Bevy ECS and separate client and world voxel.

## Terminlogy ###
- **Voxel** - A volume pixel, represents a point or a cube in a 3D Space. Usually it is used as an index/lookup in an [Array of Struct](https://en.wikipedia.org/wiki/AoS_and_SoA#Array_of_structures)
- **Chunk** - A chunk of voxels, which has a size of 16x256x16. By itself contains no data, but is used as an index/lookup in an [Array of Struct](https://en.wikipedia.org/wiki/AoS_and_SoA#Array_of_structures)
- **Kind** - Indicates what kind of voxel it is (Air, Grass, Dirt, etc)
- **Side** - Points to a direction, doesn't contain any data. Voxel as 6 sides (Up, Right, Down, Left, Front, Back), but Chunk only has 4 sides (Right, Left, Front, Back) since there is never a Chunk above or bellow.
- **Face** - Refers to a face of a cubic voxel. Each voxel has 6 faces, one for each side


TODO: The README will be updated later on.


# License
[MIT](https://choosealicense.com/licenses/mit/)
