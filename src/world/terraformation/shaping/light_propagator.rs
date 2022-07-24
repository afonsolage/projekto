use std::collections::{LinkedList, VecDeque};

use bevy::prelude::*;

use crate::world::storage::{
    chunk::{self, Chunk},
    VoxWorld,
};

pub fn propagate(world: &mut VoxWorld, locals: &[IVec3]) {
    propagate_natural_light(world, locals);
}

fn propagate_natural_light(world: &mut VoxWorld, locals: &[IVec3]) {
    for &local in locals {
        let chunk = world.get_mut(local).unwrap();
        propagate_chunk_natural_light(chunk);
    }
}

fn propagate_chunk_natural_light(chunk: &mut Chunk) {
    let mut queue = (0..chunk::X_END)
        .zip(0..chunk::Z_END)
        .map(|(x, z)| IVec3::new(x, chunk::Y_END, z))
        .collect::<VecDeque<_>>();

    while let Some(local) = queue.pop_front() {
        //
    }
}
