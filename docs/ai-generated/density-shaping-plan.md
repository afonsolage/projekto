# Density terrain shaping implementation plan

## Context

PR #116 gave the generator biome-blended landform profiles, tectonic uplift, terraces and a soft top
clamp. Terrain there is still a heightfield: a column becomes solid below its shaped height, the
overhang field lifts that height by a few blocks and the cavern field carves voxels out afterwards.
Issue #111 asks for a per-voxel density decision instead, so one field can express shelves, cliffs,
overhangs and cave intersections.

This change is phase 1. It adds the density pipeline, configuration, validation, lattice
optimizations, tests and the archive version bump while keeping the existing cave and overhang
stages. Phase 2 folds the cavern and overhang fields into the same density decision and removes the
separate overhang lift.

Current pipeline, for reference:

- [`sample_column`](crates/server/src/genesis/terrain/mod.rs#L147) shapes one world column: warped
  landform fields, blended biome shape, one terrace, then
  [`soft_clamp_height`](crates/server/src/genesis/terrain/mod.rs#L571).
- [`samples`](crates/server/src/genesis/terrain/mod.rs#L218) caches an 18x18 column halo per chunk
  with height, slope, biome and the 3D parameters.
- [`generate`](crates/server/src/genesis/terrain/mod.rs#L253) fills solid voxels through
  [`is_solid`](crates/server/src/genesis/terrain/mod.rs#L412), then applies the surface layers from
  the highest solid voxel.
- Caves and overhangs come from [`CaveConfig`](crates/server/src/genesis/terrain/config.rs#L189)
  fields sampled on a world-aligned [`Lattice`](crates/server/src/genesis/terrain/mod.rs#L602).

## Decision

### Density decision

The shaped column becomes the bias of a zero crossing:

```text
surface = bias + overhang_lift
density = (surface - y) * stretch + density3d * amplitude
solid   = density > 0 and not carved
```

- `bias` is the #109 height: landform profiles, uplift, relief, detail, blended biome shape, one
  terrace and the soft clamp. Its upper bound is now `max_height - density_margin` instead of
  `max_height`.
- `stretch` maps the warped erosion field (`-1..=1`) through a spline to a positive vertical gain.
  Low-erosion columns keep a crisper crossing while high-erosion columns give the 3D term more
  weight.
- `density3d` is one 3D noise field sampled in world coordinates.
- `density_margin = ceil(amplitude / min_stretch)`, or zero when `amplitude` is zero. A field value
  in `-1..=1` moves the crossing by at most `amplitude / stretch`, so clamping the bias below
  `max_height` by that margin keeps the upward density displacement inside the configured ceiling.
  The existing per-voxel overhang lift can still push a little past it, exactly as before this
  change.
- `amplitude = 0` disables the 3D term. With caves disabled the generated voxels are then identical
  to the old height surface: solid when `y < bias`.

### Early-outs

The fill pass runs for every voxel of the chunk, so the 3D lattices must not be sampled outside the
band where the decision can change. With `aperture = density_margin + ceil(caves.overhang_strength)`
(zero overhang strength when caves are disabled):

- `y > bias + aperture` is air. The surface is at most `bias + strength`, so the bias term is at most
  `(strength - aperture) * stretch <= -density_margin * stretch_min`, and the 3D term adds at most
  `amplitude`; the sum stays below zero.
- `y < bias - aperture` is solid for the symmetric reason above.
- Only the remaining band samples the overhang and density lattices.
- The cavern carve still runs for every solid candidate below the bias, so caves behave exactly as
  before.

### Band-limited lattice

[`density_bounds`](crates/server/src/genesis/terrain/mod.rs#L369) takes the minimum and maximum bias
over the chunk's columns, extends them by the aperture, clamps to `0..Chunk::Y_AXIS_SIZE` and lets
the lattice align the lower node down to its cell grid. The density lattice only covers that band,
which is a few dozen voxels tall instead of the whole column.

[`Lattice::with_cells`](crates/server/src/genesis/terrain/mod.rs#L619) accepts its own strides;
[`Lattice::new`](crates/server/src/genesis/terrain/mod.rs#L611) keeps the cave-config strides for the
existing cavern lattice. Nodes stay on one world grid, so neighbouring chunks interpolate identical
values at shared coordinates, and the density cell strides are validated to divide the chunk width
and height for the same reason.

The overhang lattice is band-limited with the cave strides because it is only sampled inside the
band. The cavern lattice keeps the full [`lattice_bounds`](crates/server/src/genesis/terrain/mod.rs#L393)
range because caves are carved under every column. [`ChunkLattices`](crates/server/src/genesis/terrain/mod.rs#L476)
groups the three lattices.

### Pipeline order

```text
continentalness, erosion, weirdness, detail   shape fields
temperature, humidity                         climate fields
weights, blended shape                        biome parameters
bias                                          #109 height, soft-clamped to
                                              min_height..max_height - density_margin
stretch                                       erosion -> vertical gain
band                                          bias range extended by the aperture
lattices                                      overhang and density over the band,
                                              cavern over the cave bounds
solid                                         sign of the density, then the cavern carve
surface layers                                from the highest solid voxel down
```

The surface pass still starts at the highest solid voxel of the density scan. It can now stop at an
air pocket under a density overhang exactly as it already stops at a carved cavern, so the existing
material sequence and steep-slope handling are unchanged. Slopes are still measured from the bias
columns, so a cliff created only by the density displacement keeps its biome top kind on its
vertical faces; deriving slope from the generated top solid is a follow-up.

## Configuration

`worldgen.ron` gains a `density` block:

```ron
density: (
    field: (frequency: 0.04, octaves: 3, gain: 0.5, lacunarity: 2.0),
    amplitude: 14.0,
    stretch: (
        points: [
            (-1.0, 1.35),
            (-0.3, 1.0),
            (0.3, 0.85),
            (1.0, 0.6),
        ],
    ),
    cell_xz: 4,
    cell_y: 4,
),
```

| Field | Validation | Shipped | Effect |
|---|---|---|---|
| `density.field` | [`validate_field`](crates/server/src/genesis/terrain/config.rs#L568) | frequency 0.04, 3 octaves, gain 0.5, lacunarity 2.0 | 3D term sampled in world space |
| `density.amplitude` | finite, `0..=32` | `14.0` | density units applied to the field term |
| `density.stretch` | sorted spline with every value finite in `0.1..=4.0` | points below | erosion to vertical gain |
| `density.cell_xz` | divides the chunk width | `4` | horizontal lattice stride |
| `density.cell_y` | divides the chunk height | `4` | vertical lattice stride |

The stretch spline ships `(-1.0, 1.35)`, `(-0.3, 1.0)`, `(0.3, 0.85)` and `(1.0, 0.6)`, so the
shipped `density_margin` is `ceil(14 / 0.6) = 24` blocks and the bias ceiling is `228`.
`max_height - density_margin > min_height` is validated, and the margin is zero when the amplitude is
zero.

Validation additions, each message naming the path (`density.stretch.points[2] ...`):

- `density.field` goes through the shared field validation.
- `density.stretch` is a sorted, finite spline.
- Every stretch value is finite and within `0.1..=4.0`.
- `density.amplitude` is finite and within `0..=32`.
- `density.cell_xz` divides 16 and `density.cell_y` divides 256.
- `max_height - density_margin` must stay above `min_height`.

The amplitude and cell values were tuned against the slice map rather than taken from the starting
values. At amplitude 8, 3 octaves, frequency 0.016 and cell stride 4/8 the density term had no
vertical gradient large enough to beat the bias gain, so every column stayed monotone and no overhang
appeared. Amplitude 14 with frequency 0.04 and 3 octaves, a lower stretch floor (0.6) and a 4-block
vertical lattice produces overhang pockets on about 2% of the columns while keeping a mean
neighbouring-column top difference of 0.68 blocks (the heightfield baseline is 0.33), so the
landforms stay readable and the small-scale structure is real.

## Stage split

One PR, phase 1:

- Add the configuration and validation.
- Add the density bias, band-limited lattices and the density decision with early-outs.
- Keep the overhang lift and cavern carve unchanged.
- Add the tests, bench gate, plan document, architecture update and archive version bump.

Phase 2, on top of this branch:

- Fold the cavern and overhang fields into the density expression so all solidity comes from one
  decision, then remove the overhang lift and its per-biome strength.

The tunnel families from #112 then layer onto the unified decision instead of reworking it.

## Performance

The baseline on `main` with 256 chunks and 12 threads is 0.68–0.83 ms/chunk with a median of
0.74 ms/chunk. The target is a median no more than 10% above that (~0.81 ms/chunk).

The density world measures, over five runs of `cargo bench -p projekto_server --bench genesis`
(256 chunks, 12 threads):

| Metric | Value |
|---|---|
| Median | 0.50 ms/chunk |
| Minimum | 0.50 ms/chunk |
| Maximum | 0.52 ms/chunk |

The change is faster than the baseline because the fill loop stops above the bias band and only
samples the overhang and density lattices inside it; the old loop sampled the overhang lattice for
every voxel of the column. The band-limited density lattice has about 5×5 horizontal nodes per
chunk and follows the chunk's bias range in y, so it adds several hundred field samples per chunk;
the density term itself reads the lattice only where the early-outs cannot decide.

The bench reads `PROJEKTO_GENESIS_BENCH_RUNS` (default 1) and prints the median, minimum and maximum
across runs after the existing output lines. It does not fail on a wall-clock number; CI does not run
benches, and the gate is the documented median.

## Tests

New unit tests in `genesis::terrain`:

| Test | Assertion |
|---|---|
| `shipped_density_margin_and_ceiling_are_pinned` | the shipped stretch floor 0.6 gives margin 24 and bias ceiling 228 |
| `zero_amplitude_reproduces_the_height_surface` | zero amplitude with caves disabled makes every voxel solid exactly when `y < bias` |
| `density_displaces_the_surface_inside_the_configured_ceiling` | the density moves the top solid on some columns and keeps the crossing inside `min_height - margin..=max_height` |
| `density_keeps_saturated_peaks_below_the_ceiling` | with the mountain profile saturated against the bias ceiling, the generated top solid stays at or below `max_height` |
| `density_field_creates_real_overhangs` | a strong synthetic density configuration produces a solid/air/solid column |
| `shipped_density_config_produces_real_overhangs` | the shipped values still produce overhang pockets over a 3×3 chunk area with caves disabled |
| `world_space_density_matches_generated_voxels` | an independent world-space density recomputation matches every generated voxel |
| `world_space_density_matches_generated_voxels_with_overhang_lift` | the same parity holds with the shipped overhang lift active, the density/overhang lattice strides are pinned and the band-limited overhang lattice equals the full-range reference inside the band |
| `world_space_top_solid_matches_generated_chunks` | an independent world-space scan finds the same top solid as each generated chunk |
| `band_limited_overhang_lattice_is_continuous_across_chunk_borders`, `density_lattice_is_continuous_across_chunk_borders` | both chunks' band lattices return the same value on the shared grid range |
| `band_lattices_cover_the_sampled_range` | the band lattices cover every voxel range the density decision can sample |
| `density_lattice_matches_the_field_at_nodes_and_midpoints` | nodes equal field samples and midpoints equal the trilinear corner mean |
| `lod_cells_stay_conservative_with_density_overhangs` | a coarse LOD cell containing air below a column top is never rendered solid, and at least one open cell mixes solid and air |
| config rejection tests | every new invalid density value is rejected with the path in the message, including an out-of-range global overhang strength |

Tests updated for the new clamp bound and for the density stage: the height-formula tests clamp to
`max_height - density_margin`, and the cave and overhang tests disable the density term to isolate
the stage they cover.

## Validation gates

| Check | Command | Pass condition |
|---|---|---|
| Format | `cargo fmt --all -- --check` | clean |
| Lints | `cargo clippy -p projekto_server --all-targets -- -D warnings` | clean |
| Tests | `cargo test -p projekto_server` | all tests pass |
| Benches build | `cargo check -p projekto_server --benches` | compiles |
| Bench | five runs of `cargo bench -p projekto_server --bench genesis` | median within 10% of the `main` baseline |
| Slice map | `cargo run -q -p projekto_server --example terrain_map -- 64` | landforms intact, shelves and cliffs visible, peaks under `max_height` |
| Review markers | `rg -n '//@AI'` | none left unresolved |

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| The density term turns the terrain into noise | The 3D field is one smooth field on a world-aligned lattice; the shipped values keep the neighbouring-column top difference near 0.68 blocks, and validation bounds the amplitude and cell strides |
| Peaks cross `max_height` | The bias is clamped below the ceiling by `density_margin`, and the displacement bound is tested with caves disabled |
| Genesis gets slower | Early-outs keep the lattices out of far voxels, the scan stops above the band, and the bench gate records the median |
| Seams between chunks | Lattice nodes are world-aligned and both lattices are tested across chunk borders; the world-space top-solid test covers adjacent chunks |
| The surface pass breaks on overhanging top voxels | The pass already starts at the highest solid voxel and stops at the first air voxel; the density test world exercises this path |
| Old archives keep the old world | `VERSION_MAGIC` becomes 7; per-asset reads reject older magic and the server regenerates stale chunks |
| LOD fills overhang pockets | The coarse mesher keeps a cell open when any full-resolution voxel in it is air below the column top; a test renders the density world through the coarse mesher and checks every such cell stays open |
| Phase 2 changes the decision again | The density expression, the band and the lattice refactor are the extension points; folding the cave and overhang fields in does not change the configuration shape |
| Density-only cliffs keep biome top materials on vertical faces | Slopes still come from the bias columns; phase 2 can derive steepness from the generated top-solid heights |

---

*AI-drafted plan; the author reviews and owns it.*
