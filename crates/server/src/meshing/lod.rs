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

/// Converts a voxel light intensity in `0..=15` to the normalized shader range.
const LIGHT_FRACTION: f32 = (Light::MAX_NATURAL_INTENSITY as f32).recip();

/// Downsample factors for a coarse chunk, measured in full-resolution voxels per coarse cell.
///
/// Horizontal axes are downsampled while `y` stays at one voxel, so horizontal overhangs keep
/// their full vertical detail.
#[derive(Clone, Copy)]
pub(super) struct LodScale {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) z: i32,
}

impl LodScale {
    /// Returns the per-axis scale for `lod`.
    ///
    /// LOD 1 uses 2×2 horizontal cells, LOD 2 uses 4×4, and so on. The horizontal factor is
    /// capped at 16 because a chunk is only 16 voxels wide.
    pub(super) fn for_lod(lod: u8) -> Self {
        let horizontal = 1i32 << lod.min(4);
        Self {
            x: horizontal,
            y: 1,
            z: horizontal,
        }
    }
}

/// Immutable chunk/grid context shared by the coarse face culling and lighting helpers.
struct CoarseView<'a> {
    scale: LodScale,
    nx: i32,
    ny: i32,
    nz: i32,
    neighborhood: &'a [Option<&'a ChunkStorage<Kind>>; chunk::SIDE_COUNT],
    neighborhood_light: &'a [Option<&'a ChunkStorage<Light>>; chunk::SIDE_COUNT],
}

#[derive(Clone, Copy)]
struct CoarseCell {
    top_kind: Kind,
    side_kind: Kind,
    representative: ChunkVoxel,
}

/// Builds a coarse mesh for a chunk at `lod > 0`.
#[allow(clippy::too_many_arguments)]
pub fn generate_coarse_vertices<'a>(
    chunk: Chunk,
    kind: &ChunkStorage<Kind>,
    light: &ChunkStorage<Light>,
    neighborhood: &[Option<&ChunkStorage<Kind>>; chunk::SIDE_COUNT],
    neighborhood_light: &[Option<&ChunkStorage<Light>>; chunk::SIDE_COUNT],
    get_kind: impl GetChunkStorage<'a, Kind>,
    get_light: impl GetChunkStorage<'a, Light>,
    lod: u8,
) -> Vec<voxel::Vertex> {
    let scale = LodScale::for_lod(lod);
    let nx = Chunk::X_AXIS_SIZE as i32 / scale.x;
    let ny = Chunk::Y_AXIS_SIZE as i32 / scale.y;
    let nz = Chunk::Z_AXIS_SIZE as i32 / scale.z;
    let view = CoarseView {
        scale,
        nx,
        ny,
        nz,
        neighborhood,
        neighborhood_light,
    };

    // Highest non-air Y per full-resolution column, used to tell surface air from caves.
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

    let cell_len = (nx * ny * nz) as usize;
    let mut cells = vec![None; cell_len];
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
                    if neighbor_solid(&view, &cells, IVec3::new(cx, cy, cz), side) {
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
                    let tint = Vec3::new(face_desc.color.0, face_desc.color.1, face_desc.color.2);

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
                    let ambient =
                        face_ambient_light(kind, light, &view, IVec3::new(cx, cy, cz), side) as f32;
                    let floor = ambient * 0.5;

                    for i in 0..4 {
                        vertices.push(voxel::Vertex {
                            position: positions[i],
                            normal,
                            uv: tile_uv[i],
                            tile_coord_start,
                            light: Vec3::splat(light_values[i].max(floor) * LIGHT_FRACTION) * tint,
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

/// Samples the full-resolution block behind one coarse cell.
///
/// Returns `None` for air cells. Air with solid above it belongs to a cave or overhang and stays
/// open; a mixed cell open to the sky is treated as solid so the coarse surface has no pits.
/// The returned cell carries the top/side render kinds and the voxel used for light sampling.
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

/// Returns the brightest air light in the block in front of a coarse face.
///
/// This is used as an ambient floor so a face cannot go completely black when the representative
/// voxel's soft-light sample is fully occluded.
fn face_ambient_light(
    kind: &ChunkStorage<Kind>,
    light: &ChunkStorage<Light>,
    view: &CoarseView,
    cell: IVec3,
    side: voxel::Side,
) -> u8 {
    let mut neighbor = cell;

    match side {
        voxel::Side::Right => neighbor.x += 1,
        voxel::Side::Left => neighbor.x -= 1,
        voxel::Side::Up => neighbor.y += 1,
        voxel::Side::Down => neighbor.y -= 1,
        voxel::Side::Front => neighbor.z += 1,
        voxel::Side::Back => neighbor.z -= 1,
    }

    if neighbor.y < 0 || neighbor.y >= view.ny {
        return Light::MAX_NATURAL_INTENSITY;
    }

    if neighbor.x >= 0 && neighbor.x < view.nx && neighbor.z >= 0 && neighbor.z < view.nz {
        return block_air_light(
            kind,
            light,
            neighbor.x * view.scale.x,
            neighbor.y * view.scale.y,
            neighbor.z * view.scale.z,
            view.scale,
        );
    }

    let Some(chunk_side) = ChunkSide::from_voxel_side(side) else {
        return Light::MAX_NATURAL_INTENSITY;
    };
    let (Some(neighbor_kind), Some(neighbor_light)) = (
        view.neighborhood[chunk_side as usize],
        view.neighborhood_light[chunk_side as usize],
    ) else {
        return Light::MAX_NATURAL_INTENSITY;
    };

    let (x0, y0, z0) = match side {
        voxel::Side::Right => (0, neighbor.y * view.scale.y, neighbor.z * view.scale.z),
        voxel::Side::Left => (
            Chunk::X_AXIS_SIZE as i32 - view.scale.x,
            neighbor.y * view.scale.y,
            neighbor.z * view.scale.z,
        ),
        voxel::Side::Front => (neighbor.x * view.scale.x, neighbor.y * view.scale.y, 0),
        voxel::Side::Back => (
            neighbor.x * view.scale.x,
            neighbor.y * view.scale.y,
            Chunk::Z_AXIS_SIZE as i32 - view.scale.z,
        ),
        _ => unreachable!(),
    };

    block_air_light(neighbor_kind, neighbor_light, x0, y0, z0, view.scale)
}

fn block_air_light(
    kind: &ChunkStorage<Kind>,
    light: &ChunkStorage<Light>,
    x0: i32,
    y0: i32,
    z0: i32,
    scale: LodScale,
) -> u8 {
    let mut max = 0;

    for dy in 0..scale.y {
        for dz in 0..scale.z {
            for dx in 0..scale.x {
                let voxel = ChunkVoxel::new((x0 + dx) as u8, (y0 + dy) as u8, (z0 + dz) as u8);

                if kind.get(voxel).is_none() {
                    max = max.max(light.get(voxel).get_greater_intensity());
                }
            }
        }
    }

    max
}

/// Returns whether the coarse cell adjacent to `cell` on `side` is solid.
fn neighbor_solid(
    view: &CoarseView,
    cells: &[Option<CoarseCell>],
    cell: IVec3,
    side: voxel::Side,
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

    if neighbor.y < 0 || neighbor.y >= view.ny {
        return false;
    }

    if neighbor.x >= 0 && neighbor.x < view.nx && neighbor.z >= 0 && neighbor.z < view.nz {
        return cells[cell_index(neighbor.x, neighbor.y, neighbor.z, view.nx, view.nz)].is_some();
    }

    let Some(chunk_side) = ChunkSide::from_voxel_side(side) else {
        return false;
    };
    let Some(neighbor_kind) = view.neighborhood[chunk_side as usize] else {
        // Missing neighbors are the outer landscape frontier. Treat them as solid so the
        // coarse mesh does not generate a full-height wall at the edge.
        return true;
    };

    let (x0, y0, z0) = match side {
        voxel::Side::Right => (0, neighbor.y * view.scale.y, neighbor.z * view.scale.z),
        voxel::Side::Left => (
            Chunk::X_AXIS_SIZE as i32 - view.scale.x,
            neighbor.y * view.scale.y,
            neighbor.z * view.scale.z,
        ),
        voxel::Side::Front => (neighbor.x * view.scale.x, neighbor.y * view.scale.y, 0),
        voxel::Side::Back => (
            neighbor.x * view.scale.x,
            neighbor.y * view.scale.y,
            Chunk::Z_AXIS_SIZE as i32 - view.scale.z,
        ),
        _ => return false,
    };

    block_solid(neighbor_kind, x0, y0, z0, view.scale)
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
    fn coarse_vertices_apply_kind_tint() {
        let sand = Kind::from_name("Sand").expect("Sand kind is configured");
        let mut kind = ChunkStorage::<Kind>::default();
        kind.fill(sand);
        let mut light = ChunkStorage::<Light>::default();
        light.fill(Light::natural(15));

        let vertices = generate_coarse_vertices(
            Chunk::new(0, 0),
            &kind,
            &light,
            &[None; chunk::SIDE_COUNT],
            &[None; chunk::SIDE_COUNT],
            |_| Some(&kind),
            |_| Some(&light),
            1,
        );
        let expected = Vec3::new(1.9, 1.7, 1.1);

        assert!(!vertices.is_empty());
        assert!(
            vertices
                .iter()
                .any(|vertex| (vertex.light - expected).length() < 1e-4),
            "coarse mesh did not apply the kind tint"
        );
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
            &light,
            &[None; chunk::SIDE_COUNT],
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
            &light,
            &neighborhood,
            &[None; chunk::SIDE_COUNT],
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
            &light,
            &neighborhood,
            &[None; chunk::SIDE_COUNT],
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
            &light,
            &[None; chunk::SIDE_COUNT],
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
