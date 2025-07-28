# Projekto

Projekto is a voxel game made with [Bevy Engine](https://github.com/bevyengine/bevy).


# Overview

This is a general overview of the aspects of the game. Some of these aspects may not be implemented yet or may change on the future.

The project is currently in heavy redesign. The old project which has an async focus can be found [here](https://github.com/afonsolage/projekto/tree/async_io).
The new design will try to is best to use Bevy ECS and separate client and world voxel.

## Terminology ###
- **Voxel** - A volume pixel, represents a point or a cube in a 3D Space. Usually it is used as an index/lookup in an [Array of Struct](https://en.wikipedia.org/wiki/AoS_and_SoA#Array_of_structures)
- **Chunk** - A chunk of voxels, which has a size of 16x256x16. By itself contains no data, but is used as an index/lookup in an [Array of Struct](https://en.wikipedia.org/wiki/AoS_and_SoA#Array_of_structures)
- **Kind** - Indicates what kind of voxel it is (Air, Grass, Dirt, etc)
- **Side** - Points to a direction, doesn't contain any data. Voxel as 6 sides (Up, Right, Down, Left, Front, Back), but Chunk only has 4 sides (Right, Left, Front, Back) since there is never a Chunk above or bellow.
- **Face** - Refers to a face of a cubic voxel. Each voxel has 6 faces, one for each side

## Crates ##
- **Core** - Contains all the core components used by server and client. It contains mostly voxel related structs, functions and some utilities;
- **Proto** - Is the base protocol crate, contains the underlying traits and macros in order to make the networking communication between client and server;
- **Messages** - Contains both the client and server messages. Client messages are sent to server and Server messages are sent from server to client;
- **Server** - The server binary which runs the world simulation and process most of the voxel world;
- **Client** - The game client which handles input and render the game itself;

If you need a detailed view, check [ARCHITECTURE](ARCHITECTURE.md)

# AI Usage

The code on this project is mainly written by hand (or keyboard) without the direct use of AI, although AI can be used to review architecture concepts, 
knowledge checking or to generate documentation (like ARCHITECTURE.md). Whenever I'm using AI for generate something I'll state about it.

I don't know if not using AI for coding is a good or a bad thing, but since this is a hobby project and I like programming, doing so would kill my enjoyment,
so while I'm totally against the use of IA for programming, I'll avoid using AI for generating code on this project.

# License
[MIT](https://choosealicense.com/licenses/mit/)
