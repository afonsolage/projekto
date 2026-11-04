//! Coarse 3D meshes for distant chunks.
//!
//! The coarse grid downsamples horizontally (x/z) but keeps the full vertical resolution. A
//! coarse cell is solid only when every full-resolution voxel in its block is non-air. Any air
//! pocket therefore stays open, and horizontal overhangs spanning a coarse cell remain visible.
//! Solid features narrower than a coarse cell can still simplify away at LOD.

use bevy::math::{IVec3, Vec2, Vec3};
use projekto_core::{
    chunk::{self, ChunkSide, ChunkStorage, GetChunkStorage},
    coords::{Chunk, ChunkVoxel},
    voxel::{self, Kind, Light},
};

use super::{VERTICES, VERTICES_INDICES};
use crate::light;

const LIGHT_FRACTION: f32 = (Light::MAX_NATURAL_INTENSITY as f32).recip();

#[derive(Clone, Copy)]
pub(super) struct LodScale {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) z: i32,
}

impl LodScale {
    pub(super) fn for_lod(lod: u8) -> Self {
        let horizontal = 1i32 << lod.min(4);
        Self {
            x: horizontal,
            y: 1,
            z: horizontal,
        }
    }
}

#[derive(Clone, Copy)]
struct CoarseCell {
    top_kind: Kind,
    side_kind: Kind,
    representative: ChunkVoxel,
}

/// Builds a coarse mesh for a chunk at `lod > 0`.
pub fn generate_coarse_vertices<'a>(
    chunk: Chunk,
    kind: &ChunkStorage<Kind>,
    neighborhood: &[Option<&ChunkStorage<Kind>>; chunk::SIDE_COUNT],
    get_kind: impl GetChunkStorage<'a, Kind>,
    get_light: impl GetChunkStorage<'a, Light>,
    lod: u8,
) -> Vec<voxel::Vertex> {
    let scale = LodScale::for_lod(lod);
    let nx = Chunk::X_AXIS_SIZE as i32 / scale.x;
    let ny = Chunk::Y_AXIS_SIZE as i32 / scale.y;
    let nz = Chunk::Z_AXIS_SIZE as i32 / scale.z;

    let mut top_solid = [[-1i16; Chunk::Z_AXIS_SIZE]; Chunk::X_AXIS_SIZE];
    for (x, column) in top_solid.iter_mut().enumerate() {
        for (z, height) in column.iter_mut().enumerate() {
            for y in (0..Chunk::Y_AXIS_SIZE).rev() {
                let voxel = ChunkVoxel::new(x as u8, y as u8, z as u8);
                if !kind.get(voxel).is_none() {
                    *height = y as i16;
                    break;
                }
            }
        }
    }

    let mut cells = vec![None; (nx * ny * nz) as usize];
    let mut counts = Vec::new();
    for cy in 0..ny {
        for cz in 0..nz {
            for cx in 0..nx {
                let cell =
                    sample_cell(kind, IVec3::new(cx, cy, cz), scale, &top_solid, &mut counts);
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

                let base = IVec3::new(cx * scale.x, cy * scale.y, cz * scale.z);

                for side in voxel::SIDES {
                    if neighbor_solid(
                        IVec3::new(cx, cy, cz),
                        side,
                        scale,
                        nx,
                        ny,
                        nz,
                        &cells,
                        neighborhood,
                    ) {
                        continue;
                    }

                    let face_kind = if side == voxel::Side::Up {
                        cell.top_kind
                    } else {
                        cell.side_kind
                    };
                    let face = voxel::Face {
                        vertices: [ChunkVoxel::new(0, 0, 0); 4],
                        side,
                        kind: face_kind,
                        light: [0.0; 4],
                    };
                    let face_desc = kinds.get_face_desc(&face);
                    let tile_coord_start = face_desc.offset.as_vec2() * tile_texture_size;

                    let mut positions = [Vec3::ZERO; 4];
                    for (i, position) in positions.iter_mut().enumerate() {
                        let corner = VERTICES[VERTICES_INDICES[side as usize][i]];
                        *position = Vec3::new(
                            corner[0] * scale.x as f32,
                            corner[1] * scale.y as f32,
                            corner[2] * scale.z as f32,
                        ) + base.as_vec3();
                    }

                    let x_delta = positions[0] - positions[1];
                    let y_delta = positions[0] - positions[3];
                    let x_tile =
                        (x_delta.x.abs() + x_delta.y.abs() + x_delta.z.abs()) * tile_texture_size;
                    let y_tile =
                        (y_delta.x.abs() + y_delta.y.abs() + y_delta.z.abs()) * tile_texture_size;
                    let tile_uv = [
                        Vec2::new(0.0, y_tile),
                        Vec2::new(x_tile, y_tile),
                        Vec2::new(x_tile, 0.0),
                        Vec2::new(0.0, 0.0),
                    ];

                    let normal = side.normal();
                    let neighbors = light::gather_neighborhood_light(
                        chunk,
                        cell.representative,
                        get_kind,
                        get_light,
                    );
                    let light_values = light::soft_vertex_light(&neighbors, side);
                    let ambient = neighbors.iter().flatten().copied().max().unwrap_or(0) as f32;
                    let floor = ambient * 0.5;

                    for i in 0..4 {
                        vertices.push(voxel::Vertex {
                            position: positions[i],
                            normal,
                            uv: tile_uv[i],
                            tile_coord_start,
                            light: Vec3::splat(light_values[i].max(floor) * LIGHT_FRACTION),
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
    cell: IVec3,
    scale: LodScale,
    top_solid: &[[i16; Chunk::Z_AXIS_SIZE]; Chunk::X_AXIS_SIZE],
    counts: &mut Vec<(Kind, u32)>,
) -> Option<CoarseCell> {
    let base = cell * IVec3::new(scale.x, scale.y, scale.z);
    let mut top: Option<(i32, Kind, ChunkVoxel)> = None;
    counts.clear();

    for dy in 0..scale.y {
        for dz in 0..scale.z {
            for dx in 0..scale.x {
                let voxel = ChunkVoxel::new(
                    (base.x + dx) as u8,
                    (base.y + dy) as u8,
                    (base.z + dz) as u8,
                );
                let kind = kind.get(voxel);
                let y = base.y + dy;

                if kind.is_none() {
                    // Air with solid above it is a cave or overhang: keep it open. Air that is
                    // open to the sky is part of a surface cell and gets filled below.
                    if top_solid[voxel.x as usize][voxel.z as usize] > y as i16 {
                        return None;
                    }
                    continue;
                }

                if top.is_none_or(|(top_y, _, _)| y > top_y) {
                    top = Some((y, kind, voxel));
                }

                if let Some((_, count)) = counts.iter_mut().find(|(entry, _)| *entry == kind) {
                    *count += 1;
                } else {
                    counts.push((kind, 1));
                }
            }
        }
    }

    let (_, top_kind, representative) = top?;
    let side_kind = counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(kind, _)| *kind)
        .unwrap_or(top_kind);

    Some(CoarseCell {
        top_kind,
        side_kind,
        representative,
    })
}

#[allow(clippy::too_many_arguments)]
fn neighbor_solid(
    cell: IVec3,
    side: voxel::Side,
    scale: LodScale,
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
        voxel::Side::Right => (0, neighbor.y * scale.y, neighbor.z * scale.z),
        voxel::Side::Left => (
            Chunk::X_AXIS_SIZE as i32 - scale.x,
            neighbor.y * scale.y,
            neighbor.z * scale.z,
        ),
        voxel::Side::Front => (neighbor.x * scale.x, neighbor.y * scale.y, 0),
        voxel::Side::Back => (
            neighbor.x * scale.x,
            neighbor.y * scale.y,
            Chunk::Z_AXIS_SIZE as i32 - scale.z,
        ),
        _ => return false,
    };

    block_solid(neighbor_kind, x0, y0, z0, scale)
}

pub(super) fn block_solid(
    kind: &ChunkStorage<Kind>,
    x0: i32,
    y0: i32,
    z0: i32,
    scale: LodScale,
) -> bool {
    for dy in 0..scale.y {
        for dz in 0..scale.z {
            for dx in 0..scale.x {
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

    fn scale() -> LodScale {
        LodScale::for_lod(1)
    }

    fn top_solid(kind: &ChunkStorage<Kind>) -> [[i16; Chunk::Z_AXIS_SIZE]; Chunk::X_AXIS_SIZE] {
        let mut top = [[-1i16; Chunk::Z_AXIS_SIZE]; Chunk::X_AXIS_SIZE];
        for (x, column) in top.iter_mut().enumerate() {
            for (z, height) in column.iter_mut().enumerate() {
                for y in (0..Chunk::Y_AXIS_SIZE).rev() {
                    if !kind
                        .get(ChunkVoxel::new(x as u8, y as u8, z as u8))
                        .is_none()
                    {
                        *height = y as i16;
                        break;
                    }
                }
            }
        }
        top
    }

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

        assert!(
            {
                let mut counts = Vec::new();
                sample_cell(
                    &kind,
                    IVec3::new(0, 0, 0),
                    scale(),
                    &top_solid(&kind),
                    &mut counts,
                )
                .is_none()
            },
            "any air in a coarse cell must keep a cave visible"
        );
    }

    #[test]
    fn overhang_stays_solid_above_air() {
        let mut kind = ChunkStorage::<Kind>::default();
        block(&mut kind, 2, 3);

        assert!({
            let mut counts = Vec::new();
            sample_cell(
                &kind,
                IVec3::new(0, 2, 0),
                scale(),
                &top_solid(&kind),
                &mut counts,
            )
            .is_some()
        });
        assert!({
            let mut counts = Vec::new();
            sample_cell(
                &kind,
                IVec3::new(0, 1, 0),
                scale(),
                &top_solid(&kind),
                &mut counts,
            )
            .is_none()
        });
    }

    #[test]
    fn thin_overhang_is_preserved() {
        let mut kind = ChunkStorage::<Kind>::default();
        for z in 0..2 {
            for x in 0..2 {
                kind.set(ChunkVoxel::new(x, 2, z), Kind::id(1));
            }
        }

        assert!({
            let mut counts = Vec::new();
            sample_cell(
                &kind,
                IVec3::new(0, 2, 0),
                scale(),
                &top_solid(&kind),
                &mut counts,
            )
            .is_some()
        });

        let light = ChunkStorage::<Light>::default();
        let vertices = generate_coarse_vertices(
            Chunk::new(0, 0),
            &kind,
            &[None; chunk::SIDE_COUNT],
            |_| Some(&kind),
            |_| Some(&light),
            1,
        );

        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.normal == voxel::Side::Down.normal()),
            "a one-voxel-thick overhang must still emit its underside"
        );
    }

    #[test]
    fn overhang_emits_downward_faces() {
        let mut kind = ChunkStorage::<Kind>::default();
        block(&mut kind, 2, 3);
        let light = ChunkStorage::<Light>::default();
        let neighborhood = [None; chunk::SIDE_COUNT];

        let vertices = generate_coarse_vertices(
            Chunk::new(0, 0),
            &kind,
            &neighborhood,
            |_| Some(&kind),
            |_| Some(&light),
            1,
        );
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

        let vertices = generate_coarse_vertices(
            Chunk::new(0, 0),
            &kind,
            &neighborhood,
            |_| Some(&kind),
            |_| Some(&light),
            1,
        );
        assert!(
            !vertices
                .iter()
                .any(|vertex| vertex.normal == voxel::Side::Right.normal()),
            "faces against a solid neighbor chunk must be culled"
        );
    }

    #[test]
    fn coarse_light_contains_ambient_occlusion() {
        let mut open_kind = ChunkStorage::<Kind>::default();
        block(&mut open_kind, 0, 0);
        let mut occluded_kind = open_kind.clone();
        occluded_kind.set(ChunkVoxel::new(1, 1, 0), Kind::id(1));

        let mut open_light = ChunkStorage::<Light>::default();
        let mut occluded_light = open_light.clone();
        for y in 1..=2 {
            for z in 0..2 {
                for x in 0..2 {
                    let voxel = ChunkVoxel::new(x, y, z);
                    if open_kind.get(voxel).is_none() {
                        open_light.set(voxel, Light::natural(15));
                    }
                    if occluded_kind.get(voxel).is_none() {
                        occluded_light.set(voxel, Light::natural(15));
                    }
                }
            }
        }

        let min_up = |kind: &ChunkStorage<Kind>, light: &ChunkStorage<Light>| {
            generate_coarse_vertices(
                Chunk::new(0, 0),
                kind,
                &[None; chunk::SIDE_COUNT],
                |_| Some(kind),
                |_| Some(light),
                1,
            )
            .iter()
            .filter(|vertex| vertex.normal == voxel::Side::Up.normal())
            .map(|vertex| vertex.light.x)
            .fold(f32::MAX, f32::min)
        };

        let open = min_up(&open_kind, &open_light);
        let occluded = min_up(&occluded_kind, &occluded_light);

        assert!(
            occluded < open,
            "an opaque neighbour must darken a coarse face corner ({occluded} < {open})"
        );
    }

    #[test]
    fn coarse_face_uses_adjacent_air_light() {
        let mut kind = ChunkStorage::<Kind>::default();
        for y in 0..=1 {
            for z in 0..2 {
                for x in 0..2 {
                    kind.set(ChunkVoxel::new(x, y, z), Kind::id(1));
                }
            }
        }

        let mut light = ChunkStorage::<Light>::default();
        for z in 0..2 {
            for x in 0..2 {
                light.set(ChunkVoxel::new(x, 2, z), Light::natural(15));
            }
        }

        let vertices = generate_coarse_vertices(
            Chunk::new(0, 0),
            &kind,
            &[None; chunk::SIDE_COUNT],
            |_| Some(&kind),
            |_| Some(&light),
            1,
        );

        assert!(
            vertices
                .iter()
                .filter(|vertex| vertex.normal == voxel::Side::Up.normal())
                .any(|vertex| vertex.light.x > 0.0),
            "coarse up faces must take light from adjacent air, not opaque voxels"
        );
    }
}
