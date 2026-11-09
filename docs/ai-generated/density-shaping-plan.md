# Density terrain shaping implementation plan

## Context

PR #116 gave the generator biome-blended landform profiles, tectonic uplift, terraces and a soft top
clamp. Terrain there is still a heightfield: a column becomes solid below its shaped height, the
overhang field lifts that height by a few blocks and the cavern field carves voxels out afterwards.
Issue #111 asks for a per-voxel density decision instead, so one field can express shelves, cliffs,
overhangs and cave intersections.

Phase 1, shipped in PR #124, added the density pipeline: the shaped #109 column became the bias of a
zero crossing, a 3D field and an erosion-driven stretch moved that crossing, and the cave carve plus
the overhang lift stayed separate stages. This change is phase 2: it folds the cavern carve and the
overhang lift into the same signed density, removes the separate surface test and carve branch,
retunes the cave parameters and bumps the world version. Issues #112 (richer cave systems) then adds
tunnel families to the unified field.

Current pipeline, for reference:

- [`sample_column`](crates/server/src/genesis/terrain/mod.rs#L136) shapes one world column: warped
  landform fields, blended biome shape, one terrace, then
  [`soft_clamp_height`](crates/server/src/genesis/terrain/mod.rs#L605).
- [`samples`](crates/server/src/genesis/terrain/mod.rs#L208) caches an 18x18 column halo per chunk
  with bias, slope, biome, stretch and the 3D strengths.
- [`generate`](crates/server/src/genesis/terrain/mod.rs#L243) fills solid voxels through the single
  density decision, then applies the surface layers from the highest solid voxel.
- Caves and overhangs come from [`CaveConfig`](crates/server/src/genesis/terrain/config.rs) fields
  sampled on world-aligned lattices.

## Decision

### One density decides solidity

```text
density = (bias - y) * stretch
        + density3d * density_amplitude
        + overhang3d * (column.overhang_strength * column.stretch)
        - cave_carve

cave_carve = cave_amplitude * max(0, cavern3d * column.cave_density - cave_threshold)
solid      = density > 0
```

- `bias` is the #109 height: landform profiles, uplift, relief, detail, blended biome shape, one
  terrace and the soft clamp. Its upper bound is `max_height - density_margin`.
- `stretch` maps the warped erosion field (`-1..=1`) through a spline to a positive vertical gain.
- `density3d` is the 3D density field and `density_amplitude` its density units.
- `overhang3d` is the existing cave overhang field. `overhang_strength` keeps its per-biome block
  semantics and is multiplied by the column `stretch` so the lift is expressed in density units: a
  strength of `s` blocks moves the zero crossing by up to `s` blocks, exactly like the phase-1 lift.
- `cavern3d` is the existing cavern field. The cave term subtracts density wherever the scaled
  cavern value exceeds `cavern_threshold`; `cave_amplitude` converts the excess into density units.
- There is no separate `y >= surface` test and no post-density carve branch. The overhang is no
  longer applied as a per-voxel surface displacement, and caves are no longer a carve pass: both are
  terms of the same expression.

`amplitude = 0` and `carve_amplitude = 0` with caves disabled reproduce the plain height surface
exactly: `solid = y < bias`, because `stretch` is positive.

### Bias ceiling

`density_margin = ceil(density_amplitude / min_stretch) + ceil(caves.overhang_strength)`, zero for
the disabled terms. The overhang term moves the crossing up by at most the overhang strength and the
density field by at most `density_amplitude / stretch`, so the bias is clamped below `max_height` by
that margin and the generated surface stays inside the configured ceiling. The cave carve only
lowers the density, so it cannot raise the surface. Validation rejects
`max_height - density_margin <= min_height`.

### Evaluation bounds

The fill pass is split into three ranges per column:

- `y > bias + noise_half` is air, where `noise_half = density_amplitude / stretch + overhang_strength`.
  The base density can be at most zero there and the carve only subtracts, so no lattice is
  sampled.
- `y < bias - solid_depth` is solid, where
  `solid_depth = noise_half + carve_bound / stretch` and
  `carve_bound = cave_amplitude * max(0, cave_density - cave_threshold)` bounds the largest carve
  the column can receive (the cavern field is in `-1..=1`). The base density minus the largest
  possible carve is still positive, so no lattice is sampled.
- Between those bounds the decision needs samples. Deep voxels have a provably positive base
  density: they sample the cavern lattice first and are solid unless the carve is large enough to
  matter, in which case the density and overhang lattices are read afterwards. Near the surface the
  base density is read first, and caverns are only sampled when it is positive because the carve
  can never increase solidity.

With the shipped amplitude the provable-solid floor sits below the chunk for every column, so the
band spans the whole column and the dense early-out does not trigger; the upper air early-out and
the positive-base cave sampling are the hot-path savers, and the density and overhang lattices are
still only sampled near the surface and at voxels whose carve is positive. Smaller carve amplitudes
or larger cave thresholds bring the floor back into the column; the bound stays as the guard for
those configurations.

Lattice node values are clamped to the documented `-1..=1` noise range when the three lattices are
built, so the evaluation bounds do not depend on FastNoise's approximate simplex bound.

### Band-limited lattices

[`evaluation_band`](crates/server/src/genesis/terrain/mod.rs) takes the union of every column's
decision range (provable-solid floor to possible-solid ceiling), clamps it to the chunk and aligns
the lower node to each lattice's cell grid. All three lattices are built over that band and are
world-aligned, so neighbouring chunks interpolate identical values at shared coordinates. The old
`caves.band` setting is gone: the band is derived from the decision bounds instead of a hand-tuned
margin.

### Pipeline order

```text
continentalness, erosion, weirdness, detail   shape fields
temperature, humidity                         climate fields
weights, blended shape                        biome parameters
bias                                          #109 height, soft-clamped to
                                              min_height..max_height - density_margin
stretch                                       erosion -> vertical gain
band                                          union of the decision bounds
lattices                                      density, overhang and cavern over the band
solid                                         sign of the unified density
surface layers                                from the highest solid voxel down
```

The surface pass still starts at the highest solid voxel of the density scan and stops at the first
air voxel, so an overhang underside or a cave ceiling interrupts the material run exactly like a
carved cavern did before. Slopes are still measured from the bias columns, so a cliff created only
by the density displacement keeps its biome top kind on its vertical faces.

## Configuration

`worldgen.ron` gains `caves.carve_amplitude`, loses the hand-tuned `caves.band`, and retunes the
cavern threshold. The density block is unchanged from phase 1:

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
caves: (
    enabled: true,
    overhang: (frequency: 0.008, octaves: 2, gain: 0.5, lacunarity: 2.0),
    overhang_strength: 6.0,
    cavern: (frequency: 0.02, octaves: 3, gain: 0.5, lacunarity: 2.0),
    cavern_threshold: 0.30,
    carve_amplitude: 4096.0,
    cell_xz: 4,
    cell_y: 8,
),
```

| Field | Validation | Shipped | Effect |
|---|---|---|---|
| `density.amplitude` | finite, `0..=32` | `14.0` | density units of the 3D field term |
| `density.stretch` | sorted spline, values in `0.1..=4.0` | floor `0.6` | erosion to vertical gain |
| `caves.overhang_strength` | finite, `0..=64` | `6.0` | global cap for the per-biome overhang lift, in blocks |
| `caves.cavern_threshold` | finite, `0..=1` | `0.30` | cavern excess needed before carving starts |
| `caves.carve_amplitude` | finite, `0..=MAX_CARVE_AMPLITUDE` (`8192`) | `4096.0` | density units subtracted per unit of cavern excess |

The shipped `density_margin` is `ceil(14 / 0.6) + ceil(6) = 30` blocks, so the bias ceiling is
`222`. `max_height - density_margin > min_height` is validated, as are the lattice strides dividing
the chunk width and height.

### Tuning the carve amplitude

A constant carve term has to overcome the bias term to carve at depth: the phase-1 boundary was the
depth-independent test `cavern * cave_density > 0.32`, while the unified boundary is
`cavern * cave_density > cave_threshold + base / carve_amplitude`. Keeping the phase-1 cave extent
therefore needs a large amplitude, and the amplitude sets how sharply the cave boundary follows the
threshold. Measured over the phase-1 world, the amplitude needed to preserve each carved voxel has a
median of about 800 density units and a 90th percentile of about 5 200, so values in the low
thousands are required. The shipped `4096` keeps the phase-1 cave extent and depth distribution;
[`MAX_CARVE_AMPLITUDE`](crates/server/src/genesis/terrain/config.rs) is a named validation guard at
`8192` that leaves retuning headroom for #112 while keeping the term finite.

With `carve_amplitude = 4096` the provable-solid floor sits thousands of blocks below the bias, so
the deep early-out almost never triggers. That is the cost of keeping the phase-1 cave extent with a
constant-amplitude carve term; the measured bench is unchanged because the cavern lattice was
already sampled for every positive-base voxel in phase 1.

Measured cave statistics over 169 chunks, in the region `bias - 36` and below (outside the 3D noise
band, so air there is carved air in both phases):

| Statistic | Phase 1 | Phase 2 (shipped) |
|---|---|---|
| Carved fraction | 9.72% | 9.36% |
| Columns with cave air | 52.9% | 54.85% |
| Surface entrances | 0.10% | 0.19% |
| Carved voxels within 128 blocks of the bias | 85.2% | 87.0% |

The entrance rate is slightly higher than phase 1 and stays far below the 0.4% budget asserted by
the statistics test. The unified carve can also remove the thin overhang roof above a cavity, so a
few openings expose deeper shafts than the phase-1 surface test allowed; the carved fraction and
depth numbers above show that the cave volume itself stays close. Caves are still visible across the
vertical slice.

## Stage split

Phase 1 (PR #124): configuration, density bias, band-limited density and overhang lattices, early
outs, tests, bench gate and archive version 7.

Phase 2 (this change): folds the cavern and overhang fields into the single density, removes the
separate lift and carve, derives the evaluation band from the decision bounds, retunes the cave
parameters, bumps the archive version to 8 and updates the plan, architecture section and tests.

Phase 3 (issue #112, independent): adds tunnel and chamber cave families to the same unified field.
The carve term is the extension point; adding families means adding field terms, not a second carve
pass.

## Performance

The phase-1 baseline is a median of 0.50 ms/chunk (5 runs, 256 chunks, 12 threads). The phase-2
target is at most ~0.95 ms/chunk to leave room for the extra sampling of a unified decision.

The unified world measures, over five runs of `cargo bench -p projekto_server --bench genesis`
(256 chunks, 12 threads):

| Metric | Value |
|---|---|
| Median | 0.51 ms/chunk |
| Minimum | 0.51 ms/chunk |
| Maximum | 0.51 ms/chunk |

The cost is within noise of phase 1. Phase 1 already sampled the cavern lattice for every voxel
below the bias and the overhang lattice only inside the bias band; phase 2 keeps the cavern sample
for positive-base voxels and samples the density and overhang lattices only near the surface or at
voxels whose carve is positive. The full-column band lattices add a few thousand field samples per
chunk, which the early-outs pay back on the air above the ceiling.

The bench reads `PROJEKTO_GENESIS_BENCH_RUNS` (default 1) and prints the median, minimum and maximum
across runs after the existing output lines. It does not fail on a wall-clock number; CI does not run
benches, and the gate is the documented median.

## Tests

New and updated unit tests in `genesis::terrain`:

| Test | Assertion |
|---|---|
| `world_space_density_matches_generated_voxels` | an independent unified-density probe matches every generated voxel, with caves and overhang active, and the cave term carves voxels |
| `zero_amplitude_reproduces_the_height_surface` | zero density amplitude with caves disabled reproduces `y < bias` voxel for voxel |
| `density_term_alone_matches_its_own_formula` | with only the density field active the generated voxels follow its formula |
| `overhang_term_alone_matches_its_own_formula` | with only the overhang active the crossing is the bias shifted by the lift, and it lifts voxels above the bias |
| `cave_term_alone_matches_its_own_formula` | with only the cavern field active the voxels follow `base - carve`, and no solid voxel appears above the bias |
| `increasing_cave_density_never_increases_solidity` | raising the blended cave density only removes solidity |
| `increasing_carve_amplitude_never_increases_solidity` | raising the carve amplitude only removes solidity, and the higher value removes at least one voxel |
| `zero_carve_amplitude_removes_carving` | a zero carve amplitude, with the overhang term off, produces the caves-disabled world |
| `provable_decision_bounds_match_generated_voxels` | the provable-solid floor and possible-solid ceiling, re-derived from the configuration values, match the generated voxels |
| `shipped_cave_statistics_match_the_phase_one_extent` | carved fraction, cave column fraction, entrance budget and depth extent stay inside tight bounds around the phase-1 numbers |
| `world_space_top_solid_matches_generated_chunks` | an independent world-space scan finds the same top solid as each generated chunk |
| `band_lattices_are_continuous_across_chunk_borders` | all three lattices agree between adjacent chunks on their shared range |
| `density_lattice_is_continuous_across_chunk_borders` | density lattices agree on the shared range |
| `density_lattice_matches_the_field_at_nodes_and_midpoints` | nodes equal field samples and midpoints equal the trilinear corner mean |
| `band_limited_overhang_lattice_matches_the_full_lattice` | limiting the overhang lattice to the band never changes a sampled value |
| `evaluation_band_covers_every_reachable_decision` | the band covers every column's decision range and every lattice covers the band |
| `lod_cells_stay_conservative_with_density_overhangs` | a coarse LOD cell containing air below a column top is never rendered solid |
| `density_and_overhang_keep_saturated_peaks_below_the_ceiling` | saturated peaks stay under `max_height` with both density terms active |
| `shipped_density_values_are_pinned` | the shipped margin, ceiling, carve amplitude, threshold and all three lattice strides are pinned |
| config rejection tests | every new or retuned invalid value is rejected with the path in the message |

Tests updated for the removed stages: the cave and overhang tests that isolate one stage disable the
other terms, the global density margin tests include the overhang strength, and the lattice tests
use explicit cell strides instead of the cave-config constructor.

## Validation gates

| Check | Command | Pass condition |
|---|---|---|
| Format | `cargo fmt --all -- --check` | clean |
| Lints | `cargo clippy -p projekto_server --all-targets -- -D warnings` | clean |
| Tests | `cargo test -p projekto_server` | all tests pass |
| Benches build | `cargo check -p projekto_server --benches` | compiles |
| Bench | five runs of `cargo bench -p projekto_server --bench genesis` | median at most ~0.95 ms/chunk |
| Slice map | `cargo run -q -p projekto_server --example terrain_map -- 64` | landforms intact, caves visible, peaks under `max_height` |
| Review markers | `rg -n '//@AI'` | none left unresolved |

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| A constant carve amplitude cannot reproduce the depth-independent phase-1 carve exactly | The amplitude is tuned to keep measured carved fraction, cave column share and depth extent within a few points of phase 1; the statistics test pins those numbers |
| The cave boundary becomes too sharp or too soft as the amplitude changes | `carve_amplitude` is validated and pinned by test; the cavern threshold stays in `0..=1` and is retuned with the amplitude |
| Deep voxels no longer skip all three lattices because the shipped amplitude is large | The upper air early-out and the positive-base cave gating remain; the measured bench is unchanged because phase 1 already sampled the cavern lattice under every column |
| The overhang lift changes the surface when moved into the density | The `stretch` conversion preserves the block displacement and the overhang-only test compares the crossing against the lift formula; the shipped overhang pockets test stays |
| Peaks cross `max_height` | The bias margin now covers both the density field and the overhang lift, and the saturated-peaks test runs with both active |
| Seams between chunks | All three lattices are world-aligned and tested across chunk borders; the evaluation band is derived only from the chunk's own columns |
| The unified carve can expose deeper cave shafts at the surface because it can remove the overhang roof over a cavity | The entrance rate is measured and capped by the statistics test (0.19% shipped against a 0.4% budget), and the carved fraction and depth extent are pinned close to phase 1 |
| LOD fills overhang or cave pockets | The coarse mesher keeps a cell open when any full-resolution voxel in it is air below the column top; a test renders the density world through the coarse mesher |
| The unified decision hides a regression behind the probe function | The probe is an independent implementation of the same expression, the single-term tests pin each contribution, and monotonicity tests pin the carve direction |
| Old archives keep the old world | `VERSION_MAGIC` becomes 8; per-asset reads reject older magic and the server regenerates stale chunks |
| #112 needs a different cave pipeline | The carve is one density term; cave families are added as extra field terms on the same decision |

---

*AI-drafted plan; the author reviews and owns it.*
