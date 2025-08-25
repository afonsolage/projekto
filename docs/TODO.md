# Coordinate System Refactoring Plan

This document outlines the steps to refactor the coordinate system in the project to improve type safety and code clarity.

## Step 1: Create the `coords` Module and Add Dependencies

1.  Create a new file at `crates/core/src/coords.rs`.
2.  In `crates/core/Cargo.toml`, add `derive_more` as a dependency to help reduce boilerplate when creating newtypes.
3.  In the new `coords.rs` file, define newtype wrappers for each coordinate type:
    *   `WorldPos(Vec3)`
    *   `RegionPos(IVec2)`
    *   `ChunkPos(IVec2)`
    *   `VoxelPos(IVec3)`
    *   `LocalChunkPos((u8, u8))`
4.  Expose the new module by adding `pub mod coords;` to `crates/core/src/lib.rs`.

## Step 2: Centralize Conversion Logic

1.  In `coords.rs`, implement the `From` and `Into` traits for conversions between the new coordinate types.
2.  Move the existing conversion logic from `projekto_core::chunk`, `projekto_core::voxel`, and `projekto_archive::server` into the `impl` blocks for the newtypes in `coords.rs`.
3.  Ensure all coordinate conversion logic is centralized in this new module.

## Step 3: Incrementally Refactor `projekto_core`

1.  Start by updating the `projekto_core` crate to use the new coordinate types.
2.  Modify the `Chunk` and `Voxel` structs to use `ChunkPos` and `VoxelPos` respectively.
3.  Update all function signatures within `projekto_core` to use the new, specific coordinate types instead of the generic `IVec2`, `IVec3`, and `Vec3`.
4.  The compiler will raise errors due to type mismatches. Work through these errors to ensure the new types are used correctly throughout the crate.

## Step 4: Refactor Dependent Crates

1.  Once `projekto_core` compiles successfully, proceed to refactor the other crates in the workspace that depend on it (`projekto_server`, `projekto_client`, `projekto_archive`, etc.).
2.  Update the code in these crates to use the new coordinate types from `projekto_core`. This will involve changing struct definitions, function signatures, and variable types.
3.  Again, use the compiler errors as a guide to ensure all parts of the code are updated correctly.

## Step 5: Review and Finalize

1.  After all crates are refactored and the entire project compiles and runs as expected, perform a final review.
2.  Remove any old, now-unused coordinate conversion functions that were not cleaned up during the refactoring.
3.  Ensure the new coordinate types are used consistently across the entire codebase.
