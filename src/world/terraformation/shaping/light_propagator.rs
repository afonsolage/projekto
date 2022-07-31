use std::collections::VecDeque;

use bevy::{prelude::*, utils::HashMap};
use itertools::Itertools;

use crate::world::{
    storage::{
        chunk::{self, ChunkNeighborhood},
        voxel::{self, LightTy},
        VoxWorld,
    },
    terraformation::VoxelUpdateList,
};

/// Update light on the world based on the voxel update list.
/// This function removes light when an opaque voxel is placed and propagate light otherwise.
///
/// This function assumes all chunk kind neighborhood is updated.
///
///  **Returns** a list of updated chunks.
/// Some returned chunks may have been marked as updated due to it's neighbor being updated on the edge.
pub fn update_light(
    world: &mut VoxWorld,
    updated: &[(IVec3, VoxelUpdateList)],
    ty: LightTy,
) -> Vec<IVec3> {
    let mut propagator = Propagator::new(world, ty);
    propagator.update_light(updated);
    propagator.finish()
}

/// Propagates natural light on new generated chunks. This function is optimized to be run on chunks which has no previous light.
/// For chunks that already has light values, use [`update_light`] instead.
/// This function won't remove any light.
///
/// The way this function works is by first propagating all lights internally on all given chunks, without neighbor propagation.
/// After that, all light values are propagated from a chunk to another one.
pub fn propagate_natural_light_on_new_chunk(world: &mut VoxWorld, locals: &[IVec3]) {
    let mut propagator = Propagator::new(world, LightTy::Natural);
    propagator.propagate_natural_light_on_new_chunks(locals);
    let _ = propagator.finish();
}

/// Helper struct, used to simplify and optimize propagation process.
struct Propagator<'a> {
    world: &'a mut VoxWorld,
    ty: LightTy,
    propagate_queue: VecDeque<(IVec3, Vec<IVec3>)>,
    remove_queue: VecDeque<(IVec3, Vec<IVec3>)>,
    dirty_chunks: Vec<IVec3>,
}

impl<'a> Propagator<'a> {
    /// Creates a new [`Propagator`] for the given [`VoxWorld`]
    fn new(world: &'a mut VoxWorld, ty: LightTy) -> Self {
        Self {
            world,
            ty,
            propagate_queue: Default::default(),
            remove_queue: Default::default(),
            dirty_chunks: Default::default(),
        }
    }

    /// Consumes self and return a list of chunks which was affected directly or indirectly by light propagation.
    fn finish(mut self) -> Vec<IVec3> {
        let dirty_chunks = self
            .dirty_chunks
            .iter()
            .cloned()
            .unique()
            .filter(|&local| self.world.exists(local))
            .collect_vec();

        for &local in dirty_chunks.iter() {
            self.update_light_chunk_neighborhood(local);
        }

        dirty_chunks
    }

    /// Propagate light on newly generated chunks in an optimized way.
    fn propagate_natural_light_on_new_chunks(&mut self, locals: &[IVec3]) {
        trace!("Propagating natural light on new {} chunks", locals.len());

        debug_assert!(
            self.ty == LightTy::Natural,
            "This function should be used only natural light propagation"
        );

        self.propagate_natural_light_top_down(locals);
        self.propagate_natural_light_neighborhood(locals);
    }

    /// Propagate light on queued chunks and voxels.
    /// This function update light chunk neighborhood before working on a chunk, but not after, so it may end with outdated values.
    /// This function also update neighborhood light values based on propagation across neighbors.
    fn propagate_light(&mut self, skip_neighbors: bool) {
        perf_fn_scope!();

        while let Some((local, voxels)) = self.propagate_queue.pop_front() {
            if !self.world.exists(local) {
                continue;
            }

            self.dirty_chunks.push(local);

            // TODO: Check if it's possible to optimize this later on
            self.update_light_chunk_neighborhood(local);

            // Apply propagation on current chunk, if exists, and get a list of propagations to be applied on neighbors.
            let neighbor_propagation = self.propagate_light_on_chunk(local, voxels, skip_neighbors);

            if !skip_neighbors {
                self.set_light(neighbor_propagation);
            }
        }
    }

    /// Propagates natural light across neighborhood.
    /// This function must be called after natural internal propagation, since it will check edge chunks only.
    fn propagate_natural_light_neighborhood(&mut self, locals: &[IVec3]) {
        perf_fn_scope!();

        trace!(
            "Preparing to propagate natural light to neighbors of {} chunks",
            locals.len()
        );

        // Map all voxels on the edge of chunk and propagate it's light to the neighborhood.
        let chunks_boundary_voxels = locals
            .iter()
            .map(|&local| {
                (
                    local,
                    voxel::SIDES
                        .iter()
                        .flat_map(|&side| ChunkNeighborhood::<voxel::Light>::side_iterator(side))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<HashMap<_, _>>();

        trace!(
            "Propagating light on {} chunk neighbors",
            chunks_boundary_voxels.keys().len()
        );

        self.propagate_queue = chunks_boundary_voxels.into_iter().collect();
        self.propagate_light(false);
    }

    /// Propagate initial top-down natural light across all given chunks.
    /// This function propagate the natural light from the top-most voxels downwards.
    /// This function only propagate internally, does not spread the light to neighbors.
    fn propagate_natural_light_top_down(&mut self, locals: &[IVec3]) {
        perf_fn_scope!();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, chunk::Y_END, z)))
            .collect_vec();

        self.propagate_queue = locals
            .iter()
            .map(|&local| (local, top_voxels.clone()))
            .collect();

        self.propagate_light(true);
    }

    /// Update light values based on the given [`VoxelUpdateList`] for each chunk local.
    /// This function uses a [`flood_fill`](https://en.wikipedia.org/wiki/Flood_fill) with [`BFS`](https://en.wikipedia.org/wiki/Breadth-first_search)
    /// traversal, based on [`Benjamin`](https://github.com/afonsolage/projekto/issues/29) approach.
    fn update_light(&mut self, updated: &[(IVec3, VoxelUpdateList)]) {
        perf_fn_scope!();

        let (mut removal, mut emission, mut propagation) =
            (HashMap::new(), HashMap::new(), HashMap::new());

        // Split updated list in removal and propagation
        for (local, voxels_update) in updated {
            for (voxel, new_kind) in voxels_update {
                if new_kind.is_opaque() {
                    removal.entry(*local).or_insert(vec![]).push(*voxel);
                }
                // TODO: Add this kind check to KindDescs
                if self.ty == LightTy::Artificial && *new_kind == 4.into() {
                    emission
                        .entry(*local)
                        .or_insert(vec![])
                        // TODO: Get this from Kind emission value
                        .push((*voxel, 10u8));
                } else {
                    // Get the highest surrounding light source and propagate to current voxel
                    if let Some((propagation_source_local, propagation_source_voxel)) =
                        self.find_highest_surrounding_light(*local, *voxel)
                    {
                        propagation
                            .entry(propagation_source_local)
                            .or_insert(vec![])
                            .push(propagation_source_voxel);
                    }
                };
            }
        }

        self.propagate_queue = propagation.into_iter().collect();
        self.remove_queue = removal.into_iter().collect();

        self.set_light(emission.into_iter().collect());
        self.remove_light();
        self.propagate_light(false);
    }

    /// Propagates light using a [`flood_fill`](https://en.wikipedia.org/wiki/Flood_fill) with [`BFS`](https://en.wikipedia.org/wiki/Breadth-first_search).
    /// This function won't update any neighbor, instead it returns a map containing the value to set and propagate on neighbors.
    ///
    /// **Returns** a map with values to propagate on neighbors.
    fn propagate_light_on_chunk(
        &mut self,
        local: IVec3,
        voxels: Vec<IVec3>,
        skip_neighbors: bool,
    ) -> Vec<(IVec3, Vec<(IVec3, u8)>)> {
        perf_fn_scope!();

        let mut queue = voxels.iter().cloned().collect::<VecDeque<_>>();

        let mut neighbors = vec![vec![]; voxel::SIDE_COUNT];

        let chunk = self.world.get_mut(local).unwrap();

        while let Some(voxel) = queue.pop_front() {
            if chunk.kinds.get(voxel).is_opaque() {
                continue;
            }

            let current_intensity = chunk.lights.get(voxel).get(self.ty);

            for side in voxel::SIDES {
                let side_voxel = voxel + side.dir();

                // Skip if there is no side_voxel or if it's opaque
                if let Some(side_kind) = chunk.kinds.get_absolute(side_voxel) && !side_kind.is_opaque() {
                } else {
                    continue;
                }

                let propagated_intensity =
                    Self::calc_propagated_intensity(self.ty, side, current_intensity);

                if chunk::is_within_bounds(side_voxel) {
                    // Propagate inside the chunk
                    let side_intensity = chunk.lights.get(side_voxel).get(self.ty);

                    if propagated_intensity > side_intensity {
                        chunk
                            .lights
                            .set_type(side_voxel, self.ty, propagated_intensity);

                        // TODO: Find a better way to distinguish between dirty chunks and propagation chunks
                        // If current side_voxel is on the edge, flag all neighbors as dirty, so they can be updated.
                        if chunk::is_at_bounds(side_voxel) {
                            for dir in chunk::neighboring((0, 0, 0).into(), side_voxel) {
                                self.dirty_chunks.push(local + dir);
                            }
                        }

                        if propagated_intensity > 1 {
                            queue.push_back(side_voxel);
                        }
                    }
                } else if !skip_neighbors {
                    // Propagate outside the chunk

                    let (_, neighbor_voxel) = chunk::overlap_voxel(side_voxel);

                    let neighbor_intensity =
                        match chunk.lights.neighborhood.get(side, neighbor_voxel) {
                            Some(l) => l.get(self.ty),
                            None => continue,
                        };

                    if propagated_intensity > neighbor_intensity {
                        // Flag neighbor to propagate light on verified voxel
                        neighbors[side as usize].push((neighbor_voxel, propagated_intensity));
                    }
                }
            }
        }

        neighbors
            .into_iter()
            .enumerate()
            .map(|(i, v)| (local + voxel::SIDES[i].dir(), v))
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }

    /// Update light [`ChunkNeighborhood`] of the given chunk.
    /// This function should be called whenever neighbors chunks had their light values updated.
    fn update_light_chunk_neighborhood(&mut self, local: IVec3) {
        perf_fn_scope!();

        let mut neighborhood = ChunkNeighborhood::default();
        for side in voxel::SIDES {
            let dir = side.dir();
            let neighbor = local + dir;

            if let Some(neighbor_chunk) = self.world.get(neighbor) {
                neighborhood.set(side, &neighbor_chunk.lights);
            }
        }

        let chunk = self.world.get_mut(local).unwrap();
        chunk.lights.neighborhood = neighborhood;
    }

    /// Apply a given update list and queue updated voxels for propagation.
    /// This function also check for neighboring chunks when an edge voxels is updated and mark them as dirty.
    fn set_light(&mut self, locals: Vec<(IVec3, Vec<(IVec3, u8)>)>) {
        for (neighbor_local, neighbor_lights) in locals {
            if let Some(neighbor_chunk) = self.world.get_mut(neighbor_local) {
                self.dirty_chunks.push(neighbor_local);

                let mut propagation_list = vec![];

                for (neighbor_voxel, new_intensity) in neighbor_lights {
                    neighbor_chunk
                        .lights
                        .set_type(neighbor_voxel, self.ty, new_intensity);

                    self.dirty_chunks
                        .extend(chunk::neighboring(neighbor_local, neighbor_voxel));

                    if new_intensity > 1 {
                        propagation_list.push(neighbor_voxel);
                    }
                }

                if !propagation_list.is_empty() {
                    self.propagate_queue
                        .push_back((neighbor_local, propagation_list));
                }
            }
        }
    }

    /// Find the surrounding voxel with highest light intensity.
    /// **Returns** [`Option::Some`] with chunk local and voxel position.
    /// **Returns** [`Option::None`]  if there is no surrounding light value or if it's less or equals than 1.
    fn find_highest_surrounding_light(&self, local: IVec3, voxel: IVec3) -> Option<(IVec3, IVec3)> {
        let chunk = self.world.get(local)?;

        // Get side with highest intensity
        let (highest_side, intensity) = voxel::SIDES
            .iter()
            .filter_map(|&side| {
                chunk
                    .lights
                    .get_absolute(voxel + side.dir())
                    .map(|l| (side, l.get(self.ty)))
            })
            .max_by_key(|&(_, l)| l)?;

        if intensity <= 1 {
            None
        } else {
            let side_voxel = voxel + highest_side.dir();

            if chunk::is_within_bounds(side_voxel) {
                Some((local, side_voxel))
            } else {
                let (_, neighbor_voxel) = chunk::overlap_voxel(side_voxel);
                Some((local + highest_side.dir(), neighbor_voxel))
            }
        }
    }

    /// Removed light on queued chunks and voxels.
    /// This function update light chunk neighborhood before working on a chunk, but not after, so it may end with outdated values.
    /// This function also update neighborhood light values based on light removal across neighbors.
    /// This function may queue voxels for propagation so it should be called before [`propagate_light`]
    fn remove_light(&mut self) {
        perf_fn_scope!();

        while let Some((local, voxels)) = self.remove_queue.pop_front() {
            if self.world.exists(local) {
                // TODO: Check if it's possible to optimize this later on
                self.update_light_chunk_neighborhood(local);

                self.remove_light_on_chunk(local, &voxels);
            }
        }
    }

    /// Checks if this a natural light propagation
    fn is_natural_propagation(ty: LightTy, side: voxel::Side, intensity: u8) -> bool {
        ty == LightTy::Natural
            && side == voxel::Side::Down
            && intensity == voxel::Light::MAX_NATURAL_INTENSITY
    }

    /// Removes light of the given chunk on given voxels and queue chunks for propagation.
    fn remove_light_on_chunk(&mut self, local: IVec3, voxels: &[IVec3]) {
        perf_fn_scope!();

        let chunk = self.world.get_mut(local).unwrap();

        // Remove all natural light from given voxels and queue'em up with older intensity value
        let mut queue = voxels
            .iter()
            .map(|&voxel| {
                let intensity = chunk.lights.get(voxel).get(self.ty);
                chunk.lights.set_type(voxel, self.ty, 0);
                (voxel, intensity)
            })
            .collect::<VecDeque<_>>();

        let mut propagate_self = vec![];
        let mut propagate_neighbor = vec![vec![]; voxel::SIDE_COUNT];
        let mut remove_neighbor = vec![vec![]; voxel::SIDE_COUNT];

        while let Some((voxel, old_intensity)) = queue.pop_front() {
            for side in voxel::SIDES {
                let side_voxel = voxel + side.dir();

                if chunk::is_within_bounds(side_voxel) {
                    let side_intensity = chunk.lights.get(side_voxel).get(self.ty);

                    if (Self::is_natural_propagation(self.ty, side, side_intensity))
                        || (side_intensity != 0 && old_intensity > side_intensity)
                    {
                        chunk.lights.set_type(side_voxel, self.ty, 0);

                        self.dirty_chunks.extend(chunk::neighboring(local, voxel));

                        queue.push_back((side_voxel, side_intensity));
                    } else if side_intensity >= old_intensity {
                        propagate_self.push(side_voxel);
                    }
                } else {
                    if let Some(neighbor_light) = chunk.lights.get_absolute(side_voxel) {
                        let neighbor_intensity = neighbor_light.get(self.ty);

                        let (_, neighbor_voxel) = chunk::overlap_voxel(side_voxel);

                        if neighbor_intensity != 0 && old_intensity > neighbor_intensity {
                            remove_neighbor[side as usize].push(neighbor_voxel);
                        } else if neighbor_intensity >= old_intensity {
                            propagate_neighbor[side as usize].push(voxel);
                        }
                    }
                }
            }
        }

        self.propagate_queue.push_back((local, propagate_self));
        self.propagate_queue.extend(
            propagate_neighbor
                .into_iter()
                .enumerate()
                .map(|(i, voxels)| (voxel::SIDES[i].dir() + local, voxels))
                .filter(|(_, voxels)| !voxels.is_empty()),
        );
        self.remove_queue.extend(
            remove_neighbor
                .into_iter()
                .enumerate()
                .map(|(i, voxels)| (voxel::SIDES[i].dir() + local, voxels))
                .filter(|(_, voxels)| !voxels.is_empty()),
        );
    }

    /// **Returns** the propagated intensity based on side.
    fn calc_propagated_intensity(ty: LightTy, side: voxel::Side, intensity: u8) -> u8 {
        match side {
            voxel::Side::Down
                if ty == LightTy::Natural && intensity == voxel::Light::MAX_NATURAL_INTENSITY =>
            {
                intensity
            }
            _ if intensity > 0 => intensity - 1,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::world::storage::{chunk::Chunk, voxel::Light};

    use super::*;

    fn top_voxels() -> impl Iterator<Item = IVec3> {
        (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| (x, chunk::Y_END, z).into()))
    }

    fn set_natural_light_on_top_voxels(chunk: &mut Chunk) {
        let light = Light::natural(Light::MAX_NATURAL_INTENSITY);

        for local in top_voxels() {
            chunk.lights.set(local, light);
        }
    }

    fn fill_z_axis(z: i32, chunk: &mut Chunk) {
        for x in 0..=chunk::X_END {
            for y in 0..=chunk::Y_END {
                chunk.kinds.set((x, y, z).into(), 1.into());
            }
        }
    }

    #[test]
    fn update_artificial_light_simple() {
        let mut chunk = Chunk::default();
        fill_z_axis(1, &mut chunk);

        /*
                        +------------------------+
                     4  | 6  | 7  | 8  | 7  | 6  |
                        +------------------------+
                     3  | 7  | 8  | 9  | 8  | 7  |
                        +------------------------+
        Y            2  | 8  | 9  | 10 | 9  | 8  |
        |               +------------------------+
        |            1  | 7  | 8  | 9  | 8  | 7  |
        + ---- X        +------------------------+
                     0  | 6  | 7  | 8  | 7  | 6  |
                        +------------------------+

                     +    0    1    2    3    4
        */

        chunk.kinds.set((2, 2, 0).into(), 4.into());

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        super::update_light(
            &mut world,
            &[((0, 0, 0).into(), vec![((2, 2, 0).into(), 4.into())])],
            LightTy::Artificial,
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();

        assert_eq!(
            chunk.lights.get((2, 2, 0).into()).get(LightTy::Artificial),
            10,
            "Light value should be set on placed voxel"
        );
    }

    #[test]
    fn update_natural_light_simple() {
        let mut chunk = Chunk::default();
        set_natural_light_on_top_voxels(&mut chunk);
        fill_z_axis(1, &mut chunk);

        chunk.kinds.set((1, 0, 0).into(), 1.into());
        chunk.kinds.set((1, 1, 0).into(), 1.into());
        chunk.kinds.set((1, 2, 0).into(), 1.into());

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        super::propagate_natural_light_on_new_chunk(&mut world, &[(0, 0, 0).into()]);

        let chunk = world.get_mut((0, 0, 0).into()).unwrap();
        assert_eq!(chunk.lights.get((1, 3, 0).into()).get(LightTy::Natural), 15);

        chunk.kinds.set((1, 2, 0).into(), 0.into());
        drop(chunk);

        super::update_light(
            &mut world,
            &[((0, 0, 0).into(), vec![((1, 2, 0).into(), 0.into())])],
            LightTy::Natural,
        );

        let chunk = world.get_mut((0, 0, 0).into()).unwrap();
        assert_eq!(chunk.lights.get((1, 2, 0).into()).get(LightTy::Natural), 15);
    }

    #[test]
    fn propagate_chunk_natural_light_empty() {
        let mut chunk = Chunk::default();
        set_natural_light_on_top_voxels(&mut chunk);

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        let mut propagador = Propagator::new(&mut world, LightTy::Natural);
        propagador.propagate_light_on_chunk((0, 0, 0).into(), top_voxels().collect(), true);

        // Test the test function
        assert_eq!(
            top_voxels().count(),
            chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();

        for local in chunk::voxels() {
            assert_eq!(
                chunk.lights.get(local).get(voxel::LightTy::Natural),
                Light::MAX_NATURAL_INTENSITY
            );
        }
    }

    #[test]
    fn propagate_chunk_natural_light_neighborhood() {
        /*
                           Chunk               Neighbor                    Chunk               Neighbor
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     11 | -- | 15 |        | -- | -- | 15 |          11 | -- | 15 |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     10 | -- | 15 |        | -- | -- | 15 |          10 | -- | 15 |        | 14*| -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     9  | -- | -- |        | 0  | -- | 15 |          9  | -- | -- |        | 13 | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     8  | -- | 2  |        | 1  | -- | 15 |          8  | -- | 11 |        | 12 | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     7  | -- | 3  |        | -- | -- | 15 |          7  | -- | 10 |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     6  | -- | 4  |        | 5  | -- | 15 |    ->    6  | -- | 9  |        | 8  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     5  | -- | -- |        | 6  | -- | 15 |          5  | -- | -- |        | 7  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     4  | -- | 8  |        | 7  | -- | 15 |          4  | -- | 8  |        | 7  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     3  | -- | 9  |        | -- | -- | 15 |          3  | -- | 9  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
        Y            2  | -- | 10 |        | 11 | -- | 15 |          2  | -- | 10 |        | 11 | -- | 15 |
        |               +----+----+        +----+----+----+             +----+----+        +----+----+----+
        |            1  | -- | -- |        | 12 | -- | 15 |          1  | -- | -- |        | 12 | -- | 15 |
        + ---- X        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     0  | -- | 12 |        | 13 | 14 | 15 |          0  | -- | 12 |        | 13 | 14 | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+

                     +    14   15            0    1    2             +    14   15            0    1    2
        */

        let mut world = VoxWorld::default();

        let mut chunk = Chunk::default();
        chunk.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (11..=chunk::Y_END).rev() {
            chunk.kinds.set((15, y, 0).into(), 0.into());
        }

        let mut neighbor = Chunk::default();
        neighbor.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (0..=chunk::Y_END).rev() {
            neighbor.kinds.set((2, y, 0).into(), 0.into());
        }

        chunk.kinds.set((15, 11, 0).into(), 0.into());
        chunk.kinds.set((15, 10, 0).into(), 0.into());
        chunk.kinds.set((15, 8, 0).into(), 0.into());
        chunk.kinds.set((15, 7, 0).into(), 0.into());
        chunk.kinds.set((15, 6, 0).into(), 0.into());
        chunk.kinds.set((15, 4, 0).into(), 0.into());
        chunk.kinds.set((15, 3, 0).into(), 0.into());
        chunk.kinds.set((15, 2, 0).into(), 0.into());
        chunk.kinds.set((15, 0, 0).into(), 0.into());

        neighbor.kinds.set((0, 8, 0).into(), 0.into());
        neighbor.kinds.set((0, 9, 0).into(), 0.into());
        neighbor.kinds.set((0, 6, 0).into(), 0.into());
        neighbor.kinds.set((0, 5, 0).into(), 0.into());
        neighbor.kinds.set((0, 4, 0).into(), 0.into());
        neighbor.kinds.set((0, 2, 0).into(), 0.into());
        neighbor.kinds.set((0, 1, 0).into(), 0.into());
        neighbor.kinds.set((0, 0, 0).into(), 0.into());
        neighbor.kinds.set((1, 0, 0).into(), 0.into());
        neighbor.kinds.set((2, 0, 0).into(), 0.into());

        set_natural_light_on_top_voxels(&mut neighbor);
        set_natural_light_on_top_voxels(&mut chunk);

        world.add((0, 0, 0).into(), chunk);
        world.add((1, 0, 0).into(), neighbor);

        super::super::update_kind_neighborhoods(
            &mut world,
            vec![(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );

        super::propagate_natural_light_on_new_chunk(
            &mut world,
            &vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );

        assert_eq!(
            world
                .get((1, 0, 0).into())
                .unwrap()
                .lights
                .get((0, 0, 0).into())
                .get(LightTy::Natural),
            13,
            "Light propagation failed. This is handled in another test"
        );

        assert_eq!(
            world
                .get((0, 0, 0).into())
                .unwrap()
                .lights
                .get((15, 0, 0).into())
                .get(LightTy::Natural),
            12,
            "Light propagation failed. This is handled in another test"
        );

        let neighbor = world.get_mut((1, 0, 0).into()).unwrap();
        neighbor.kinds.set((0, 10, 0).into(), 0.into());
        drop(neighbor);

        super::super::update_kind_neighborhoods(
            &mut world,
            vec![(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );

        let updated = [((1, 0, 0).into(), vec![((0, 10, 0).into(), 0.into())])];

        //Act
        super::update_light(&mut world, &updated, LightTy::Natural);

        // Check neighbor
        let neighbor = world.get((1, 0, 0).into()).unwrap();
        let expected_neighbor = [
            ((0, 0, 0).into(), 13),
            ((0, 1, 0).into(), 12),
            ((0, 2, 0).into(), 11),
            ((0, 4, 0).into(), 7),
            ((0, 5, 0).into(), 7),
            ((0, 6, 0).into(), 8),
            ((0, 8, 0).into(), 12),
            ((0, 9, 0).into(), 13),
            ((0, 10, 0).into(), 14),
        ];

        for (voxel, intensity) in expected_neighbor {
            assert_eq!(
                neighbor.lights.get(voxel).get(LightTy::Natural),
                intensity,
                "Failed at {voxel}"
            );
        }

        // Check chunk
        let chunk = world.get((0, 0, 0).into()).unwrap();
        let expected_chunk = [
            ((15, 0, 0).into(), 12),
            ((15, 2, 0).into(), 10),
            ((15, 3, 0).into(), 9),
            ((15, 4, 0).into(), 8),
            ((15, 6, 0).into(), 9),
            ((15, 7, 0).into(), 10),
            ((15, 8, 0).into(), 11),
        ];

        for (voxel, intensity) in expected_chunk {
            assert_eq!(
                chunk.lights.get(voxel).get(LightTy::Natural),
                intensity,
                "Failed at {voxel}"
            );
        }
    }

    #[test]
    fn remove_chunk_natural_light_neighborhood() {
        /*
                           Chunk               Neighbor                    Chunk               Neighbor
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     11 | -- | 15 |        | -- | -- | 15 |          11 | -- | 15 |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     10 | -- | 15 |        | 14 | -- | 15 |          10 | -- | 15 |        | 14 | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     9  | -- | -- |        | 13 | -- | 15 |          9  | -- | -- |        | --*| -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     8  | -- | 11 |        | 12 | -- | 15 |          8  | -- | 0  |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     7  | -- | 10 |        | -- | -- | 15 |          7  | -- | 0  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     6  | -- | 9  |        | 8  | -- | 15 |    ->    6  | -- | 0  |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     5  | -- | -- |        | 7  | -- | 15 |          5  | -- | -- |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     4  | -- | 8  |        | 7  | -- | 15 |          4  | -- | 0  |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     3  | -- | 9  |        | -- | -- | 15 |          3  | -- | 0  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
        Y            2  | -- | 10 |        | 11 | -- | 15 |          2  | -- | 0  |        | --*| -- | 15 |
        |               +----+----+        +----+----+----+             +----+----+        +----+----+----+
        |            1  | -- | -- |        | 12 | -- | 15 |          1  | -- | -- |        | 12 | -- | 15 |
        + ---- X        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     0  | -- | 12 |        | 13 | 14 | 15 |          0  | -- | 12 |        | 13 | 14 | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+

                     +    14   15            0    1    2             +    14   15            0    1    2
        */

        let mut world = VoxWorld::default();

        let mut chunk = Chunk::default();
        chunk.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (11..=chunk::Y_END).rev() {
            chunk.kinds.set((15, y, 0).into(), 0.into());
        }

        let mut neighbor = Chunk::default();
        neighbor.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (0..=chunk::Y_END).rev() {
            neighbor.kinds.set((2, y, 0).into(), 0.into());
        }

        chunk.kinds.set((15, 11, 0).into(), 0.into());
        chunk.kinds.set((15, 10, 0).into(), 0.into());
        chunk.kinds.set((15, 8, 0).into(), 0.into());
        chunk.kinds.set((15, 7, 0).into(), 0.into());
        chunk.kinds.set((15, 6, 0).into(), 0.into());
        chunk.kinds.set((15, 4, 0).into(), 0.into());
        chunk.kinds.set((15, 3, 0).into(), 0.into());
        chunk.kinds.set((15, 2, 0).into(), 0.into());
        chunk.kinds.set((15, 0, 0).into(), 0.into());

        neighbor.kinds.set((0, 10, 0).into(), 0.into());
        neighbor.kinds.set((0, 9, 0).into(), 0.into());
        neighbor.kinds.set((0, 8, 0).into(), 0.into());
        neighbor.kinds.set((0, 6, 0).into(), 0.into());
        neighbor.kinds.set((0, 5, 0).into(), 0.into());
        neighbor.kinds.set((0, 4, 0).into(), 0.into());
        neighbor.kinds.set((0, 2, 0).into(), 0.into());
        neighbor.kinds.set((0, 1, 0).into(), 0.into());
        neighbor.kinds.set((0, 0, 0).into(), 0.into());
        neighbor.kinds.set((1, 0, 0).into(), 0.into());

        set_natural_light_on_top_voxels(&mut neighbor);
        set_natural_light_on_top_voxels(&mut chunk);

        world.add((0, 0, 0).into(), chunk);
        world.add((1, 0, 0).into(), neighbor);

        super::super::update_kind_neighborhoods(
            &mut world,
            vec![(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );
        super::propagate_natural_light_on_new_chunk(
            &mut world,
            &vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );

        assert_eq!(
            world
                .get((1, 0, 0).into())
                .unwrap()
                .lights
                .get((0, 0, 0).into())
                .get(LightTy::Natural),
            13,
            "Light propagation failed. This is handled in another test"
        );

        assert_eq!(
            world
                .get((0, 0, 0).into())
                .unwrap()
                .lights
                .get((15, 0, 0).into())
                .get(LightTy::Natural),
            12,
            "Light propagation failed. This is handled in another test"
        );

        let neighbor = world.get_mut((1, 0, 0).into()).unwrap();

        neighbor.kinds.set((0, 9, 0).into(), 1.into());
        neighbor.kinds.set((0, 2, 0).into(), 1.into());

        let voxels = [(
            (1, 0, 0).into(),
            vec![((0, 9, 0).into(), 1.into()), ((0, 2, 0).into(), 1.into())],
        )];

        drop(neighbor);

        //Act
        let mut propagator = Propagator::new(&mut world, LightTy::Natural);
        propagator.update_light(&voxels);

        // Check neighbor
        let neighbor = world.get((1, 0, 0).into()).unwrap();
        let expected_neighbor = [
            ((0, 0, 0).into(), 13),
            ((0, 1, 0).into(), 12),
            ((0, 4, 0).into(), 0),
            ((0, 5, 0).into(), 0),
            ((0, 6, 0).into(), 0),
            ((0, 8, 0).into(), 0),
        ];

        for (voxel, intensity) in expected_neighbor {
            assert_eq!(
                neighbor.lights.get(voxel).get(LightTy::Natural),
                intensity,
                "Failed at {voxel}"
            );
        }

        // Check chunk
        let chunk = world.get((0, 0, 0).into()).unwrap();
        let expected_chunk = [
            ((15, 0, 0).into(), 12),
            ((15, 2, 0).into(), 0),
            ((15, 3, 0).into(), 0),
            ((15, 4, 0).into(), 0),
            ((15, 6, 0).into(), 0),
            ((15, 7, 0).into(), 0),
            ((15, 8, 0).into(), 0),
        ];

        for (voxel, intensity) in expected_chunk {
            assert_eq!(
                chunk.lights.get(voxel).get(LightTy::Natural),
                intensity,
                "Failed at {voxel}"
            );
        }
    }

    #[test]
    fn remove_chunk_natural_light_simple() {
        /*
                        +------------------------+      +-----------------------------+
                     4  | 15 | 15 | 15 | 15 | 15 |      | 15 | 15 | 15 | 15 | 15 | 15 |
                        +------------------------+      +-----------------------------+
                     3  | 15 | 15 | 15 | 15 | 15 |      | -- | -- | -- | -- | 15 | 15 |
                        +------------------------+      +-----------------------------+
        Y            2  | 15 | 15 | 15 | 15 | 15 |  ->  | -- | 12 | 13 | 14 | 15 | 15 |
        |               +------------------------+      +-----------------------------+
        |            1  | 15 | 15 | 15 | 15 | 15 |      | -- | 11 | -- | -- | -- | 15 |
        + ---- X        +------------------------+      +-----------------------------+
                     0  | 15 | 15 | 15 | 15 | 15 |      | -- | 10 | 9  | 8  | 7  | -- |
                        +------------------------+      +-----------------------------+

                     +    0    1    2    3    4      +    0    1    2    3    4    5
        */

        let mut chunk = Chunk::default();

        // Fill all blocks on Z = 1 so we can ignore the third dimension when propagating the light
        fill_z_axis(1, &mut chunk);
        set_natural_light_on_top_voxels(&mut chunk);

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);
        super::propagate_natural_light_on_new_chunk(&mut world, &[(0, 0, 0).into()]);

        let chunk = world.get_mut((0, 0, 0).into()).unwrap();

        for x in 0..=chunk::X_END {
            for y in 0..=chunk::Y_END {
                assert_eq!(
                    chunk.lights.get((x, y, 0).into()).get(LightTy::Natural),
                    voxel::Light::MAX_NATURAL_INTENSITY,
                    "Failed at {}",
                    IVec3::new(x, y, 0),
                );
            }
        }

        chunk.kinds.set((0, 0, 0).into(), 1.into());
        chunk.kinds.set((0, 1, 0).into(), 1.into());
        chunk.kinds.set((0, 2, 0).into(), 1.into());
        chunk.kinds.set((0, 3, 0).into(), 1.into());
        chunk.kinds.set((1, 3, 0).into(), 1.into());
        chunk.kinds.set((2, 1, 0).into(), 1.into());
        chunk.kinds.set((2, 3, 0).into(), 1.into());
        chunk.kinds.set((3, 1, 0).into(), 1.into());
        chunk.kinds.set((3, 3, 0).into(), 1.into());
        chunk.kinds.set((4, 1, 0).into(), 1.into());
        chunk.kinds.set((5, 0, 0).into(), 1.into());

        let chunk_map = [(
            (0, 0, 0).into(),
            vec![
                ((0, 0, 0).into(), 1.into()),
                ((0, 1, 0).into(), 1.into()),
                ((0, 2, 0).into(), 1.into()),
                ((0, 3, 0).into(), 1.into()),
                ((1, 3, 0).into(), 1.into()),
                ((2, 1, 0).into(), 1.into()),
                ((2, 3, 0).into(), 1.into()),
                ((3, 1, 0).into(), 1.into()),
                ((3, 3, 0).into(), 1.into()),
                ((4, 1, 0).into(), 1.into()),
            ],
        )];

        super::update_light(&mut world, &chunk_map, LightTy::Natural);

        let expected = vec![
            ((0, 0, 0).into(), 0),
            ((0, 1, 0).into(), 0),
            ((0, 2, 0).into(), 0),
            ((0, 3, 0).into(), 0),
            ((0, 4, 0).into(), 15),
            ((1, 0, 0).into(), 10),
            ((1, 1, 0).into(), 11),
            ((1, 2, 0).into(), 12),
            ((1, 3, 0).into(), 0),
            ((1, 4, 0).into(), 15),
            ((2, 0, 0).into(), 9),
            ((2, 1, 0).into(), 0),
            ((2, 2, 0).into(), 13),
            ((2, 3, 0).into(), 0),
            ((2, 4, 0).into(), 15),
            ((3, 0, 0).into(), 8),
            ((3, 1, 0).into(), 0),
            ((3, 2, 0).into(), 14),
            ((3, 3, 0).into(), 0),
            ((3, 4, 0).into(), 15),
            ((4, 0, 0).into(), 7),
            ((4, 1, 0).into(), 0),
            ((4, 2, 0).into(), 15),
            ((4, 3, 0).into(), 15),
            ((4, 4, 0).into(), 15),
        ];

        let chunk = world.get((0, 0, 0).into()).unwrap();
        for (voxel, intensity) in expected {
            assert_eq!(
                chunk.lights.get(voxel).get(LightTy::Natural),
                intensity,
                "Failed at {voxel}"
            );
        }
    }

    #[test]
    fn propagate_chunk_natural_light_simple_blocked() {
        /*
                        +------------------------+
                     4  | 15 | 15 | 15 | 15 | 15 |
                        +------------------------+
                     3  | 15 | 15 | -- | 15 | 15 |
                        +------------------------+
        Y            2  | 15 | 15 | 14 | 15 | 15 |
        |               +------------------------+
        |            1  | 15 | 15 | 14 | 15 | 15 |
        + ---- X        +------------------------+
                     0  | 15 | 15 | 14 | 15 | 15 |
                        +------------------------+

                     +    0    1    2    3    4
        */

        let mut chunk = Chunk::default();

        set_natural_light_on_top_voxels(&mut chunk);

        chunk.kinds.set((2, 3, 0).into(), 1.into());

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        let mut propagator = Propagator::new(&mut world, LightTy::Natural);
        propagator.propagate_natural_light_on_new_chunks(&[(0, 0, 0).into()]);

        let chunk = world.get((0, 0, 0).into()).unwrap();

        assert_eq!(
            chunk.lights.get((2, 3, 0).into()).get_greater_intensity(),
            0,
            "There should be no light on solid blocks"
        );

        assert_eq!(
            chunk.lights.get((2, 2, 0).into()).get_greater_intensity(),
            Light::MAX_NATURAL_INTENSITY - 1,
        );

        assert_eq!(
            chunk.lights.get((2, 1, 0).into()).get_greater_intensity(),
            Light::MAX_NATURAL_INTENSITY - 1,
        );

        assert_eq!(
            chunk.lights.get((2, 0, 0).into()).get_greater_intensity(),
            Light::MAX_NATURAL_INTENSITY - 1,
        );
    }

    #[test]
    fn propagate_chunk_natural_light_complex_blocked() {
        /*
                        +-----------------------------+----+----+
                     7  | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |
                        +-----------------------------+----+----+
                     6  | 15 | 15 | -- | -- | -- | -- | -- | 15 |
                        +-----------------------------+----+----+
                     5  | 15 | -- | 10 | 9  | 8  | 7  | 6  | -- |
                        +-----------------------------+----+----+
                     4  | 15 | -- | 11 | -- | 7  | -- | 5  | -- |
                        +-----------------------------+----+----+
                     3  | 15 | -- | 12 | -- | 6  | -- | 4  | -- |
                        +-----------------------------+----+----+
        Y            2  | 15 | 14 | 13 | -- | 5  | -- | 3  | -- |
        |               +-----------------------------+----+----+
        |            1  | -- | -- | -- | -- | 4  | 3  | 2  | -- |
        + ---- X        +-----------------------------+----+----+
                     0  | -- | 0  | 1  | 2  | 3  | 2  | 1  | -- |
                        +-----------------------------+----+----+

                     +    0    1    2    3    4    5    6    7
        */

        let mut chunk = Chunk::default();

        // Fill all blocks on Z = 1 so we can ignore the third dimension when propagating the light
        fill_z_axis(1, &mut chunk);

        set_natural_light_on_top_voxels(&mut chunk);

        chunk.kinds.set((0, 0, 0).into(), 1.into());
        chunk.kinds.set((0, 1, 0).into(), 1.into());
        chunk.kinds.set((1, 1, 0).into(), 1.into());
        chunk.kinds.set((1, 3, 0).into(), 1.into());
        chunk.kinds.set((1, 4, 0).into(), 1.into());
        chunk.kinds.set((1, 5, 0).into(), 1.into());
        chunk.kinds.set((2, 1, 0).into(), 1.into());
        chunk.kinds.set((2, 6, 0).into(), 1.into());
        chunk.kinds.set((3, 1, 0).into(), 1.into());
        chunk.kinds.set((3, 2, 0).into(), 1.into());
        chunk.kinds.set((3, 3, 0).into(), 1.into());
        chunk.kinds.set((3, 4, 0).into(), 1.into());
        chunk.kinds.set((3, 6, 0).into(), 1.into());
        chunk.kinds.set((4, 6, 0).into(), 1.into());
        chunk.kinds.set((5, 2, 0).into(), 1.into());
        chunk.kinds.set((5, 3, 0).into(), 1.into());
        chunk.kinds.set((5, 4, 0).into(), 1.into());
        chunk.kinds.set((5, 6, 0).into(), 1.into());
        chunk.kinds.set((6, 6, 0).into(), 1.into());
        chunk.kinds.set((7, 0, 0).into(), 1.into());
        chunk.kinds.set((7, 1, 0).into(), 1.into());
        chunk.kinds.set((7, 2, 0).into(), 1.into());
        chunk.kinds.set((7, 3, 0).into(), 1.into());
        chunk.kinds.set((7, 4, 0).into(), 1.into());
        chunk.kinds.set((7, 5, 0).into(), 1.into());

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        let mut propagator = Propagator::new(&mut world, LightTy::Natural);
        propagator.propagate_natural_light_on_new_chunks(&[(0, 0, 0).into()]);

        let expected = [
            ((0, 0, 0).into(), 0),
            ((0, 1, 0).into(), 0),
            ((0, 2, 0).into(), 15),
            ((0, 3, 0).into(), 15),
            ((0, 4, 0).into(), 15),
            ((0, 5, 0).into(), 15),
            ((0, 6, 0).into(), 15),
            ((0, 7, 0).into(), 15),
            ((1, 0, 0).into(), 0),
            ((1, 1, 0).into(), 0),
            ((1, 2, 0).into(), 14),
            ((1, 3, 0).into(), 0),
            ((1, 4, 0).into(), 0),
            ((1, 5, 0).into(), 0),
            ((1, 6, 0).into(), 15),
            ((1, 7, 0).into(), 15),
            ((2, 0, 0).into(), 1),
            ((2, 1, 0).into(), 0),
            ((2, 2, 0).into(), 13),
            ((2, 3, 0).into(), 12),
            ((2, 4, 0).into(), 11),
            ((2, 5, 0).into(), 10),
            ((2, 6, 0).into(), 0),
            ((2, 7, 0).into(), 15),
            ((3, 0, 0).into(), 2),
            ((3, 1, 0).into(), 0),
            ((3, 2, 0).into(), 0),
            ((3, 3, 0).into(), 0),
            ((3, 4, 0).into(), 0),
            ((3, 5, 0).into(), 9),
            ((3, 6, 0).into(), 0),
            ((3, 7, 0).into(), 15),
            ((4, 0, 0).into(), 3),
            ((4, 1, 0).into(), 4),
            ((4, 2, 0).into(), 5),
            ((4, 3, 0).into(), 6),
            ((4, 4, 0).into(), 7),
            ((4, 5, 0).into(), 8),
            ((4, 6, 0).into(), 0),
            ((4, 7, 0).into(), 15),
            ((5, 0, 0).into(), 2),
            ((5, 1, 0).into(), 3),
            ((5, 2, 0).into(), 0),
            ((5, 3, 0).into(), 0),
            ((5, 4, 0).into(), 0),
            ((5, 5, 0).into(), 7),
            ((5, 6, 0).into(), 0),
            ((5, 7, 0).into(), 15),
            ((6, 0, 0).into(), 1),
            ((6, 1, 0).into(), 2),
            ((6, 2, 0).into(), 3),
            ((6, 3, 0).into(), 4),
            ((6, 4, 0).into(), 5),
            ((6, 5, 0).into(), 6),
            ((6, 6, 0).into(), 0),
            ((6, 7, 0).into(), 15),
            ((7, 0, 0).into(), 0),
            ((7, 1, 0).into(), 0),
            ((7, 2, 0).into(), 0),
            ((7, 3, 0).into(), 0),
            ((7, 4, 0).into(), 0),
            ((7, 5, 0).into(), 0),
            ((7, 6, 0).into(), 15),
            ((7, 7, 0).into(), 15),
        ];

        let chunk = world.get_mut((0, 0, 0).into()).unwrap();

        for (local, intensity) in expected {
            assert_eq!(
                chunk.lights.get(local).get_greater_intensity(),
                intensity,
                "Failed at local {:?}",
                local
            );
        }

        /*
                        +-----------------------------+----+----+
                     7  | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |
                        +-----------------------------+----+----+
                     6  | 15 | 15 | -- | -- | -- | -- | -- | 15 |
                        +-----------------------------+----+----+
                     5  | 15 | -- | 10 | 11 | 12 | 13 | 14 | 15 |
                        +-----------------------------+----+----+
                     4  | 15 | -- | 11 | -- | 11 | -- | 13 | -- |
                        +-----------------------------+----+----+
                     3  | 15 | -- | 12 | -- | 10 | -- | 12 | -- |
                        +-----------------------------+----+----+
        Y            2  | 15 | 14 | 13 | -- | 9  | -- | 11 | -- |
        |               +-----------------------------+----+----+
        |            1  | -- | -- | -- | -- | 8  | 9  | 10 | -- |
        + ---- X        +-----------------------------+----+----+
                     0  | -- | 4  | 5  | 6  | 7  | 8  | 9  | -- |
                        +-----------------------------+----+----+

                     +    0    1    2    3    4    5    6    7
        */

        // Allow light to enter on (7, 5)
        chunk.kinds.set((7, 5, 0).into(), 0.into());

        let mut propagator = Propagator::new(&mut world, LightTy::Natural);
        propagator.update_light(&[((0, 0, 0).into(), vec![((7, 5, 0).into(), 0.into())])]);

        let expected = [
            ((0, 0, 0).into(), 0),
            ((0, 1, 0).into(), 0),
            ((0, 2, 0).into(), 15),
            ((0, 3, 0).into(), 15),
            ((0, 4, 0).into(), 15),
            ((0, 5, 0).into(), 15),
            ((0, 6, 0).into(), 15),
            ((0, 7, 0).into(), 15),
            ((1, 0, 0).into(), 4),
            ((1, 1, 0).into(), 0),
            ((1, 2, 0).into(), 14),
            ((1, 3, 0).into(), 0),
            ((1, 4, 0).into(), 0),
            ((1, 5, 0).into(), 0),
            ((1, 6, 0).into(), 15),
            ((1, 7, 0).into(), 15),
            ((2, 0, 0).into(), 5),
            ((2, 1, 0).into(), 0),
            ((2, 2, 0).into(), 13),
            ((2, 3, 0).into(), 12),
            ((2, 4, 0).into(), 11),
            ((2, 5, 0).into(), 10),
            ((2, 6, 0).into(), 0),
            ((2, 7, 0).into(), 15),
            ((3, 0, 0).into(), 6),
            ((3, 1, 0).into(), 0),
            ((3, 2, 0).into(), 0),
            ((3, 3, 0).into(), 0),
            ((3, 4, 0).into(), 0),
            ((3, 5, 0).into(), 11),
            ((3, 6, 0).into(), 0),
            ((3, 7, 0).into(), 15),
            ((4, 0, 0).into(), 7),
            ((4, 1, 0).into(), 8),
            ((4, 2, 0).into(), 9),
            ((4, 3, 0).into(), 10),
            ((4, 4, 0).into(), 11),
            ((4, 5, 0).into(), 12),
            ((4, 6, 0).into(), 0),
            ((4, 7, 0).into(), 15),
            ((5, 0, 0).into(), 8),
            ((5, 1, 0).into(), 9),
            ((5, 2, 0).into(), 0),
            ((5, 3, 0).into(), 0),
            ((5, 4, 0).into(), 0),
            ((5, 5, 0).into(), 13),
            ((5, 6, 0).into(), 0),
            ((5, 7, 0).into(), 15),
            ((6, 0, 0).into(), 9),
            ((6, 1, 0).into(), 10),
            ((6, 2, 0).into(), 11),
            ((6, 3, 0).into(), 12),
            ((6, 4, 0).into(), 13),
            ((6, 5, 0).into(), 14),
            ((6, 6, 0).into(), 0),
            ((6, 7, 0).into(), 15),
            ((7, 0, 0).into(), 0),
            ((7, 1, 0).into(), 0),
            ((7, 2, 0).into(), 0),
            ((7, 3, 0).into(), 0),
            ((7, 4, 0).into(), 0),
            ((7, 5, 0).into(), 15),
            ((7, 6, 0).into(), 15),
            ((7, 7, 0).into(), 15),
        ];

        let chunk = world.get((0, 0, 0).into()).unwrap();

        for (local, intensity) in expected {
            assert_eq!(
                chunk.lights.get(local).get_greater_intensity(),
                intensity,
                "Failed at local {:?}",
                local
            );
        }
    }

    #[test]
    fn propagate_natural_light_new_chunk_neighborhood() {
        /*
                           Chunk             Neighbor
                        +----+----+        +----+----+
                     11 | -- | 15 |        | -- | -- |
                        +----+----+        +----+----+
                     10 | -- | 15 |        | 14 | -- |
                        +----+----+        +----+----+
                     9  | -- | -- |        | 13 | -- |
                        +----+----+        +----+----+
                     8  | -- | 11 |        | 12 | -- |
                        +----+----+        +----+----+
                     7  | -- | 10 |        | -- | -- |
                        +----+----+        +----+----+
                     6  | -- | 9  |        | 8  | -- |
                        +----+----+        +----+----+
                     5  | -- | -- |        | 7  | -- |
                        +----+----+        +----+----+
                     4  | -- | 5  |        | 6  | -- |
                        +----+----+        +----+----+
                     3  | -- | 4  |        | -- | -- |
                        +----+----+        +----+----+
        Y            2  | -- | 3  |        | 2  | -- |
        |               +----+----+        +----+----+
        |            1  | -- | -- |        | 1  | -- |
        + ---- X        +----+----+        +----+----+
                     0  | -- | 0  |        | 0  | -- |
                        +----+----+        +----+----+

                     +    14   15            0    1
        */

        let mut world = VoxWorld::default();

        let mut chunk = Chunk::default();
        chunk.kinds.set_all(1.into()); // Make solid

        let mut neighbor = Chunk::default();
        neighbor.kinds.set_all(1.into()); // Make solid

        // Make a path to light propagate through
        for y in (11..=chunk::Y_END).rev() {
            chunk.kinds.set((15, y, 0).into(), 0.into());
        }

        chunk.kinds.set((15, 11, 0).into(), 0.into());
        chunk.kinds.set((15, 10, 0).into(), 0.into());
        chunk.kinds.set((15, 8, 0).into(), 0.into());
        chunk.kinds.set((15, 7, 0).into(), 0.into());
        chunk.kinds.set((15, 6, 0).into(), 0.into());
        chunk.kinds.set((15, 4, 0).into(), 0.into());
        chunk.kinds.set((15, 3, 0).into(), 0.into());
        chunk.kinds.set((15, 2, 0).into(), 0.into());
        chunk.kinds.set((15, 0, 0).into(), 0.into());

        neighbor.kinds.set((0, 10, 0).into(), 0.into());
        neighbor.kinds.set((0, 9, 0).into(), 0.into());
        neighbor.kinds.set((0, 8, 0).into(), 0.into());
        neighbor.kinds.set((0, 6, 0).into(), 0.into());
        neighbor.kinds.set((0, 5, 0).into(), 0.into());
        neighbor.kinds.set((0, 4, 0).into(), 0.into());
        neighbor.kinds.set((0, 2, 0).into(), 0.into());
        neighbor.kinds.set((0, 1, 0).into(), 0.into());
        neighbor.kinds.set((0, 0, 0).into(), 0.into());

        // Set light only on Chunk, so it can propagate all the way down.
        set_natural_light_on_top_voxels(&mut chunk);

        world.add((0, 0, 0).into(), chunk);
        world.add((1, 0, 0).into(), neighbor);

        super::super::update_kind_neighborhoods(
            &mut world,
            [(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );

        super::propagate_natural_light_on_new_chunk(
            &mut world,
            &[(0, 0, 0).into(), (1, 0, 0).into()],
        );

        let chunk_expected = [
            ((15, 11, 0).into(), 15),
            ((15, 10, 0).into(), 15),
            ((15, 8, 0).into(), 11),
            ((15, 7, 0).into(), 10),
            ((15, 6, 0).into(), 9),
            ((15, 4, 0).into(), 5),
            ((15, 3, 0).into(), 4),
            ((15, 2, 0).into(), 3),
            ((15, 0, 0).into(), 0),
        ];

        let chunk = world.get((0, 0, 0).into()).unwrap();

        for (local, intensity) in chunk_expected {
            assert_eq!(
                chunk.lights.get(local).get(voxel::LightTy::Natural),
                intensity,
                "Failed at {:?}",
                local
            );
        }

        let neighbor_expected = [
            ((0, 10, 0).into(), 14),
            ((0, 9, 0).into(), 13),
            ((0, 8, 0).into(), 12),
            ((0, 6, 0).into(), 8),
            ((0, 5, 0).into(), 7),
            ((0, 4, 0).into(), 6),
            ((0, 2, 0).into(), 2),
            ((0, 1, 0).into(), 1),
            ((0, 0, 0).into(), 0),
        ];

        let neighbor = world.get((1, 0, 0).into()).unwrap();

        for (local, intensity) in neighbor_expected {
            assert_eq!(
                neighbor.lights.get(local).get(voxel::LightTy::Natural),
                intensity,
                "Failed at {:?}",
                local
            );
        }
    }
}
