//! Coordinates are a bit complex since they need to be relative to something.
//!
//! This mod attempts to organize it to be more comprehensive, while using Rust zero-cost
//! abstraction.
//!
//! The general rule is:
//! - If the coordinates are in `Vec3`/`Vec2`, this means it is a general world point in the world;
//! - If the coordinates are in `IVec3`/`IVec2`, this means it points to a `Voxel` in the world;
//! - If the coordinates are in `u8`/`u16`, this means it points to a voxel relative to it's parent
//!   container. This can be a `ChunkVoxel`, which is a voxel inside a `Chunk`, or a `RegionChunk`,
//!   which is a chunk inside a `Region`.
//!
//! The naming pattern is ``ParentChild``, so it is easier to understand what are the constrains of the
//! current coordinates. If some math has to be done, it needs to be converted to either
//! `IVec3`/`IVec2` or `Vec3`/`Vec2`, since the resulting coordinates can be out of bounds.
//!
//! Please note that only trivial conversions use the traits `From`/`TryFrom`. Conversions which
//! needs actual math or some form of logic, are done in explicitly calls, in the form of
//! ``from_XXXX``
//!
//! The conversion methods `From`/`TryFrom` and new methods `new`/`new_from` ensure the correctness
//! of parent bounds and will panic in debug mode, but not in release mode.
mod chunk;
mod region;
mod voxel;

pub use chunk::*;
pub use region::*;
pub use voxel::*;
