//! Coarse 3D meshes for distant chunks.
//!
//! The coarse grid is a conservative 3D downsample: a coarse cell is solid only when every
//! full-resolution voxel in its block is non-air. Any air pocket therefore stays open, which
//! keeps caves and overhangs visible instead of collapsing them into a heightmap.

use bevy::math::{IVec3, Vec2, Vec3};
use projekto_core::{
    chunk::{self, ChunkSide, ChunkStorage},
    coords::{Chunk, ChunkVoxel},
    voxel::{self, Kind, Light},
};

use super::{VERTICES, VERTICES_INDICES};

const LIGHT_FRACTION: f32 = (Light::MAX_NATURAL_INTENSITY as f32).recip();

#[derive(Clone, Copy)]
struct CoarseCell {
    kind: Kind,
    light: u8,
}

/// Builds a coarse mesh for a chunk at `lod > 0`.
pub fn generate_coarse_vertices(
    kind: &ChunkStorage<Kind>,
    light: &ChunkStorage<Light>,
    neighborhood: &[Option<&ChunkStorage<Kind>>; chunk::SIDE_COUNT],
    lod: u8,
) -> Vec<voxel::Vertex> {
    let factor = 1i32 << lod.min(4);
    let nx = Chunk::X_AXIS_SIZE as i32 / factor;
    let ny = Chunk::Y_AXIS_SIZE as i32 / factor;
    let nz = Chunk::Z_AXIS_SIZE as i32 / factor;

    let mut cells = vec![None; (nx * ny * nz) as usize];
    for cy in 0..ny {
        for cz in 0..nz {
            for cx in 0..nx {
                let cell = sample_cell(kind, light, IVec3::new(cx, cy, cz), factor);
                cells[cell_index(cx, cy, cz, nx, nz)] = cell;
            }
        }
    }

    let kinds = voxel::KindsDescs::get();
    let tile_texture_size = (kinds.count_tiles() as f32).recip();
    let mut vertices = Vec::new();

    for cy in 0..ny {
        for cz in 0..nz {
            for cx in 0..nx {
                let Some(cell) = cells[cell_index(cx, cy, cz, nx, nz)] else {
                    continue;
                };

                let base = IVec3::new(cx, cy, cz) * factor;
                let face_light = Vec3::splat(cell.light as f32 * LIGHT_FRACTION);

                for side in voxel::SIDES {
                    if neighbor_solid(
                        IVec3::new(cx, cy, cz),
                        side,
                        factor,
                        nx,
                        ny,
                        nz,
                        &cells,
                        neighborhood,
                    ) {
                        continue;
                    }

                    let face = voxel::Face {
                        vertices: [ChunkVoxel::new(0, 0, 0); 4],
                        side,
                        kind: cell.kind,
                        light: [0.0; 4],
                    };
                    let face_desc = kinds.get_face_desc(&face);
                    let tile_coord_start = face_desc.offset.as_vec2() * tile_texture_size;
                    let tile = factor as f32 * tile_texture_size;
                    let tile_uv = [
                        Vec2::new(0.0, tile),
                        Vec2::new(tile, tile),
                        Vec2::new(tile, 0.0),
                        Vec2::new(0.0, 0.0),
                    ];
                    let normal = side.normal();
                    let base = base.as_vec3();

                    for i in 0..4 {
                        let corner = VERTICES[VERTICES_INDICES[side as usize][i]];
                        vertices.push(voxel::Vertex {
                            position: Vec3::new(corner[0], corner[1], corner[2]) * factor as f32
                                + base,
                            normal,
                            uv: tile_uv[i],
                            tile_coord_start,
                            light: face_light,
                        });
                    }
                }
            }
        }
    }

    vertices
}

fn cell_index(x: i32, y: i32, z: i32, nx: i32, nz: i32) -> usize {
    ((y * nz + z) * nx + x) as usize
}

fn sample_cell(
    kind: &ChunkStorage<Kind>,
    light: &ChunkStorage<Light>,
    cell: IVec3,
    factor: i32,
) -> Option<CoarseCell> {
    let base = cell * factor;
    let mut top: Option<(i32, Kind, u8)> = None;

    for dy in 0..factor {
        for dz in 0..factor {
            for dx in 0..factor {
                let voxel = ChunkVoxel::new(
                    (base.x + dx) as u8,
                    (base.y + dy) as u8,
                    (base.z + dz) as u8,
                );
                let kind = kind.get(voxel);

                if kind.is_none() {
                    return None;
                }

                let y = base.y + dy;
                if top.is_none_or(|(top_y, _, _)| y > top_y) {
                    top = Some((y, kind, light.get(voxel).get_greater_intensity()));
                }
            }
        }
    }

    top.map(|(_, kind, light)| CoarseCell { kind, light })
}

#[allow(clippy::too_many_arguments)]
fn neighbor_solid(
    cell: IVec3,
    side: voxel::Side,
    factor: i32,
    nx: i32,
    ny: i32,
    nz: i32,
    cells: &[Option<CoarseCell>],
    neighborhood: &[Option<&ChunkStorage<Kind>>; chunk::SIDE_COUNT],
) -> bool {
    let mut neighbor = cell;

    match side {
        voxel::Side::Right => neighbor.x += 1,
        voxel::Side::Left => neighbor.x -= 1,
        voxel::Side::Up => neighbor.y += 1,
        voxel::Side::Down => neighbor.y -= 1,
        voxel::Side::Front => neighbor.z += 1,
        voxel::Side::Back => neighbor.z -= 1,
    }

    if neighbor.y < 0 || neighbor.y >= ny {
        return false;
    }

    if neighbor.x >= 0 && neighbor.x < nx && neighbor.z >= 0 && neighbor.z < nz {
        return cells[cell_index(neighbor.x, neighbor.y, neighbor.z, nx, nz)].is_some();
    }

    let Some(chunk_side) = ChunkSide::from_voxel_side(side) else {
        return false;
    };
    let Some(neighbor_kind) = neighborhood[chunk_side as usize] else {
        return false;
    };

    let (x0, y0, z0) = match side {
        voxel::Side::Right => (0, neighbor.y * factor, neighbor.z * factor),
        voxel::Side::Left => (
            Chunk::X_AXIS_SIZE as i32 - factor,
            neighbor.y * factor,
            neighbor.z * factor,
        ),
        voxel::Side::Front => (neighbor.x * factor, neighbor.y * factor, 0),
        voxel::Side::Back => (
            neighbor.x * factor,
            neighbor.y * factor,
            Chunk::Z_AXIS_SIZE as i32 - factor,
        ),
        _ => return false,
    };

    block_solid(neighbor_kind, x0, y0, z0, factor)
}

fn block_solid(kind: &ChunkStorage<Kind>, x0: i32, y0: i32, z0: i32, factor: i32) -> bool {
    for dy in 0..factor {
        for dz in 0..factor {
            for dx in 0..factor {
                let voxel = ChunkVoxel::new((x0 + dx) as u8, (y0 + dy) as u8, (z0 + dz) as u8);

                if kind.get(voxel).is_none() {
                    return false;
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(kind: &mut ChunkStorage<Kind>, y0: u8, y1: u8) {
        for y in y0..=y1 {
            for z in 0..2 {
                for x in 0..2 {
                    kind.set(ChunkVoxel::new(x, y, z), Kind::id(1));
                }
            }
        }
    }

    #[test]
    fn air_pocket_stays_open() {
        let mut kind = ChunkStorage::<Kind>::default();
        block(&mut kind, 0, 1);
        kind.set(ChunkVoxel::new(0, 0, 0), Kind::none());
        let light = ChunkStorage::<Light>::default();

        assert!(
            sample_cell(&kind, &light, IVec3::new(0, 0, 0), 2).is_none(),
            "any air in a coarse cell must keep a cave visible"
        );
    }

    #[test]
    fn overhang_stays_solid_above_air() {
        let mut kind = ChunkStorage::<Kind>::default();
        block(&mut kind, 2, 3);
        let light = ChunkStorage::<Light>::default();

        assert!(sample_cell(&kind, &light, IVec3::new(0, 1, 0), 2).is_some());
        assert!(sample_cell(&kind, &light, IVec3::new(0, 0, 0), 2).is_none());
    }

    #[test]
    fn overhang_emits_downward_faces() {
        let mut kind = ChunkStorage::<Kind>::default();
        block(&mut kind, 2, 3);
        let light = ChunkStorage::<Light>::default();
        let neighborhood = [None; chunk::SIDE_COUNT];

        let vertices = generate_coarse_vertices(&kind, &light, &neighborhood, 1);
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.normal == voxel::Side::Down.normal()),
            "the underside of an overhang must be meshed"
        );
    }

    #[test]
    fn solid_neighbor_culls_boundary_faces() {
        let mut kind = ChunkStorage::<Kind>::default();
        kind.fill(Kind::id(1));
        let mut neighbor = ChunkStorage::<Kind>::default();
        neighbor.fill(Kind::id(1));
        let light = ChunkStorage::<Light>::default();
        let neighborhood = [Some(&neighbor); chunk::SIDE_COUNT];

        let vertices = generate_coarse_vertices(&kind, &light, &neighborhood, 1);
        assert!(
            !vertices
                .iter()
                .any(|vertex| vertex.normal == voxel::Side::Right.normal()),
            "faces against a solid neighbor chunk must be culled"
        );
    }
}
