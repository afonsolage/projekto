use bevy::prelude::*;

use crate::world::{chunk, math, voxel};

#[derive(Default, Debug, Clone, Copy)]
pub struct RaycastHit {
    pub local: IVec3,
    pub position: Vec3,
    pub normal: IVec3,
}

pub fn intersect(origin: Vec3, dir: Vec3, range: f32) -> Vec<(RaycastHit, Vec<RaycastHit>)> {
    let mut result = vec![];

    let (hit_locals, hit_positions, hit_normals) = raycast(origin, dir, range, RaycastType::Chunk);

    debug_assert_eq!(hit_locals.len(), hit_positions.len());
    debug_assert_eq!(hit_locals.len(), hit_normals.len());

    for (idx, local) in hit_locals.iter().enumerate() {
        let hit_position = hit_positions[idx];

        let chunk_hit = RaycastHit {
            local: *local,
            position: hit_position,
            normal: hit_normals[idx],
        };

        let (voxel_hit_locals, voxel_hit_positions, voxel_hit_normals) =
            raycast(hit_position, dir, 0.0, RaycastType::Voxel);

        debug_assert_eq!(voxel_hit_locals.len(), voxel_hit_positions.len());
        debug_assert_eq!(voxel_hit_locals.len(), voxel_hit_normals.len());

        let mut voxels_hit = vec![];

        for (v_idx, v_local) in voxel_hit_locals.iter().enumerate() {
            voxels_hit.push(RaycastHit {
                local: *v_local,
                position: voxel_hit_positions[v_idx],
                normal: voxel_hit_normals[v_idx],
            })
        }

        result.push((chunk_hit, voxels_hit));
    }

    result
}

enum RaycastType {
    Chunk,
    Voxel,
}

fn raycast(
    origin: Vec3,
    dir: Vec3,
    range: f32,
    tp: RaycastType,
) -> (Vec<IVec3>, Vec<Vec3>, Vec<IVec3>) {
    let to_local = match tp {
        RaycastType::Chunk => chunk::to_local,
        RaycastType::Voxel => voxel::to_local,
    };

    let to_world = match tp {
        RaycastType::Chunk => chunk::to_world,
        RaycastType::Voxel => voxel::to_world,
    };

    let mut visited_chunks = vec![];
    let mut visited_positions = vec![];
    let mut visited_normals = vec![];

    let mut current_pos = origin;
    let mut current_chunk = to_local(&origin);
    let mut last_chunk = current_chunk;

    let grid_dir = math::to_grid_dir(&dir);
    let tile_offset = IVec3::new(
        if dir.x >= 0.0 { 1 } else { 0 },
        if dir.y >= 0.0 { 1 } else { 0 },
        if dir.z >= 0.0 { 1 } else { 0 },
    );

    while match tp {
        RaycastType::Chunk => current_pos.distance(origin) < range,
        RaycastType::Voxel => chunk::is_whitin_bounds(&current_chunk),
    } {
        visited_chunks.push(current_chunk);
        visited_positions.push(current_pos);
        visited_normals.push(last_chunk - current_chunk);

        last_chunk = current_chunk;

        let next_chunk = current_chunk + tile_offset;
        let delta = (to_world(&next_chunk) - current_pos) / dir;
        let distance = if delta.x < delta.y && delta.x < delta.z {
            current_chunk.x += grid_dir.x;
            delta.x
        } else if delta.y < delta.x && delta.y < delta.z {
            current_chunk.y += grid_dir.y;
            delta.y
        } else {
            current_chunk.z += grid_dir.z;
            delta.z
        };

        current_pos += distance * dir * 1.01;
    }

    (visited_chunks, visited_positions, visited_normals)
}
