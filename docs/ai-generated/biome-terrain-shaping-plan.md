# Biome terrain shaping implementation plan

## Context

PR #108 added five surface-only biomes. At that point the (pre-#109) pipeline in
[`TerrainGenerator`](crates/server/src/genesis/terrain/mod.rs) still computed one global height from continentalness, erosion, weirdness and detail, and
[`BiomeTable::select`](crates/server/src/genesis/terrain/biome.rs) only chose which
materials [`SurfaceRule::kind`](crates/server/src/genesis/terrain/surface.rs#L29) applies. The
approved design in [`biome-proposal.md`](docs/ai-generated/biome-proposal.md) put height shaping
and blended transitions out of scope and listed them as follow-ups. Issue #109 closes the height
shape and 3D-characteristics part of that gap while keeping transitions as a separate feature.

The body below describes the original biome-shaping design. The landform addendum near the end
supersedes its shape and transition details; the historical statements are kept for context.

Current pipeline, for reference:

- The pre-#109 `column` helper sampled continentalness, erosion, weirdness and detail and
  returned the height plus the erosion it already read.
- [`height`](crates/server/src/genesis/terrain/mod.rs#L90) is `base + peaks_and_valleys *
  relief_amplitude(erosion) + detail`, clamped to `min_height..max_height`.
- [`slope`](crates/server/src/genesis/terrain/mod.rs#L111) evaluates `height` at the four
  neighbour columns and returns the largest axis difference over two.
- [`generate`](crates/server/src/genesis/terrain/mod.rs#L118) caches heights, slopes and biome
  indices, then fills solid voxels through [`is_solid`](crates/server/src/genesis/terrain/mod.rs#L211)
  and applies the biome surface layers.
- [`BiomesConfig`](crates/server/src/genesis/terrain/config.rs#L63) holds `max_slope`, `steep`,
  `deep` and the five entries. Global cave settings live in [`CaveConfig`](crates/server/src/genesis/terrain/config.rs#L106).
- Archive version 5 ([`VERSION_MAGIC`](crates/archive/src/lib.rs#L58)) and mesh version 9
  ([`CHUNK_MESH_VERSION`](crates/archive/src/asset.rs#L10)).

## Design

### Pipeline order

```text
continentalness, erosion, weirdness, detail   shape fields
temperature, humidity                         climate fields
weights                                       blend over (temperature, humidity, erosion)
blended shape                                 height offset, relief multiplier, terrace
                                              strength, cave density, overhang strength
height                                        blend, terrace once, clamp
slope                                         largest axis difference of cached heights
dominant biome                                argmax squared distance, used for surfaces
solid voxels                                  blended overhang lift and cavern carve
surface layers                                dominant biome surface rule
```

Selection reads only the noise fields. It never reads height, slope or any shape parameter, so
there is no feedback cycle and no ordering ambiguity. This removes the "shape-only elevation
estimate" branch mentioned in the issue; the estimate is only needed if a future change selects a
biome by elevation, which stays out of scope. A test asserts that mutating every shape parameter
leaves the weights and the dominant index unchanged.

### Blend weights

Selection keeps the nearest-point table over `(temperature, humidity, erosion)` and gains a weight
pass:

1. For every entry `i`, compute the squared distance `sq_i = dt*dt + dh*dh + de*de`.
2. Track the dominant entry with the same strict `<` comparison and table order as today, so the
   dominant index is bit-identical to the current [`BiomeTable::select`](crates/server/src/genesis/terrain/biome.rs#L43).
3. Let `d_i = sqrt(sq_i)` and `d_min = sqrt(sq_min)`.
4. Raw weight `w_i = t_i * t_i` with `t_i = max(0, 1 - (d_i - d_min) / transition)`.
5. Normalize by the sum of raw weights.

`transition` is a new `biomes.transition` field expressed in parameter-space distance. It is the
distance from the nearest entry at which another entry's weight reaches zero. The squared falloff
has zero derivative at each entry's own cut-off, so weights do not step at the edge of the band.
The nearest entry always has raw weight 1, so it dominates before and after normalization. All
loops visit entries in table order and f32 summation order is fixed.

Dominance is decided on squared distances, not on the square roots: two distinct squared distances
can round to the same `sqrt` value, which would otherwise turn a strict comparison into a tie and
move the dominant index away from what #108 selects. `select` remains as a thin wrapper that
returns the dominant index from the weight pass.

The minimum validated `transition` is `0.1`. The parameter-space gradient of the climate fields is
about `0.0015..0.01` per block, so the smallest accepted band covers at least ten blocks of blend
in world space and cannot collapse to a one-column hard edge between integer columns. A test sweeps
`transition` values and asserts that the boundary stays continuous in each case. The upper guard of
`1.0` is a tuning bound, not a correctness constraint: a very wide band averages more biomes into
the blended shape, but dominance and the surface rule stay on the nearest entry.

Weight computation is one square root per entry per column. A guard covers non-finite sums: if the
sum is zero or not finite, the dominant entry receives weight 1 and the other weights are zero, so
extreme coordinates degrade to today's nearest-point behavior instead of producing `NaN`.

### Shape parameters

Each [`BiomeConfig`](crates/server/src/genesis/terrain/config.rs#L76) gains a `shape` block with
five fields. The terrace step and the transition width are shared settings in `biomes`, not
per-entry fields.

| Field | Range | Neutral | Effect | Blend |
|---|---|---|---|---|
| `height_offset` | `-32..=32` blocks | `0.0` | added to the column height before clamping | linear |
| `relief_multiplier` | `0..=2` | `1.0` | multiplies the peaks-and-valleys amplitude | linear |
| `terrace_strength` | `0..=1` | `0.0` | blends between raw and smoothed staircase height | linear |
| `cave_density` | `0..=2` | `1.0` | scales the cavern field before the threshold | linear |
| `overhang_strength` | `0..=caves.overhang_strength` blocks | global value | vertical overhang displacement | linear |

The blend is written relative to the neutral values, which makes the neutral configuration an
exact no-op instead of one that depends on the normalized weights summing to exactly one in f32:

```text
offset     = sum w_i * height_offset_i
multiplier = 1 + sum w_i * (relief_multiplier_i - 1)
strength   = sum w_i * terrace_strength_i
density    = 1 + sum w_i * (cave_density_i - 1)
overhang   = global + sum w_i * (overhang_strength_i - global)
```

Height formula:

```text
raw      = base_height(c) + peaks_and_valleys * relief_amplitude(e) * multiplier
           + detail + offset
terraced = raw + strength * (terrace(raw, terrace_step) - raw)
height   = terraced.clamp(min_height, max_height)
```

`terrace(h, step)` is `step * (floor(h / step) + smoothstep(frac))` with
`frac = h / step - floor(h / step)` and `smoothstep(u) = u * u * (3 - 2u)`. The staircase is
continuous because `smoothstep(0) = 0` and `smoothstep(1) = 1`, so the value is exactly
`(n + 1) * step` on both sides of a cell edge. For a constant step its derivative with respect to
`h` is in `0..=1.5`, and the staircase deviates from the raw height by at most about
`0.1 * step`. With a blended strength and a shared step the spatial gradient is bounded by

```text
|grad height| <= (1 + 0.5 * strength) * |grad raw| + 0.1 * step * |grad strength|
```

`terrace_strength = 0` is exactly the identity because the formula adds `0 * (terraced - raw)`.
Terracing is applied once, after the blend, never per biome and then blended: a per-biome
staircase would reintroduce the discontinuity the blend exists to remove.

Offsetting the base elevation is additive, and a per-biome height spline would need the same
blend and add config duplication for the same two degrees of freedom, so the multiplier plus offset
model is the smallest one that satisfies "base elevation" and "relief amplitude". The shared
terrace step keeps the staircase scale consistent across the world and removes the extra
`grad step` term that per-biome steps would add.

The clamp stays as the final safety net. Validation bounds each field; it does not try to prove a
cross-field envelope. With the shipped values the nominal top envelope,
`base_height` maximum 190 + amplitude maximum 30 times the largest multiplier 1.4 + detail 0.6 +
offset 6 + terrace overshoot 0.8, is about 239.4, under `max_height` 240. `peaks_and_valleys` is
in `-1..=1`, so today's floor only misses `min_height` by the detail term, about 0.6 blocks at the
lowest base. A relief multiplier of 1.4 pushes those valleys up to about 12.6 blocks below the
floor, where the clamp flattens them. The biome report below prints pre-clamp saturation in both
directions so tuning can see it. If top saturation becomes visible, lower the Roughlands
multiplier or offset rather than raising `max_height` toward the chunk height.

### Cave density and overhang strength

Both are blended per column and cached before the fill pass. They keep using the interpolated
[`Lattice`](crates/server/src/genesis/terrain/mod.rs#L259), not direct 3D field samples, so the
coarse-grid sampling cost and the existing border continuity are unchanged:

- Cavern: a voxel below `height` is carved when `density > 0` and
  `cavern.sample(x, y, z) * density > cavern_threshold`. `density = 0` disables cavern carving for
  any valid threshold. `cavern_threshold` is validated to `0..=1`, so a zero density already cannot
  carve; the explicit `density > 0` guard keeps the carve condition monotonic if that range is ever
  widened.
- Overhang: `surface = height + overhang.sample(x, y, z) * overhang_strength`. The strength is a
  non-negative blended value, so a strength of zero leaves the surface exactly at `height`.
- The `world_y < height` cave condition, the lattice construction, `lattice_bounds` and the
  `world_y >= surface` solidity test are unchanged.

The global `caves.overhang_strength` remains the upper bound for the biome values and sizes the
lattice margin, so validation capping each biome at that value keeps the existing coverage proof
and test valid. `is_solid` receives one `ColumnSample` reference (height, overhang strength, cave
density) instead of two extra scalars, which keeps the argument count under the clippy threshold.

### Biome boundaries

- Height is continuous. The noise fields, distances, weights, blended parameters and the terrace
  blend are continuous in world coordinates. Height is C0, not C1: switching which entry is the
  nearest creates a kink in the derivative of the minimum distance, and the clamp creates a kink at
  saturation. Kinks are bounded slopes, not jumps, and the issue asks for no cliff.
- Cave and overhang character is continuous per column: the cavern threshold and the overhang
  displacement vary smoothly, so carved volumes and lifted surfaces move continuously.
- Surface materials switch in one column at the Voronoi boundary, where the dominant index
  changes. Kinds are discrete and cannot be interpolated; this is the same hard material edge
  PR #108 accepted. The issue's continuity criterion is about height, and it stays separate from
  material kinds.
- The surface pass walks down from the highest solid voxel, so it follows the generated height.
  That voxel can be below `height` when a cavern carved the column's surface, exactly as today.

## Configuration

`worldgen.ron` gains `biomes.transition`, `biomes.terrace_step` and a `shape` block per entry:

```ron
biomes: (
    max_slope: 1.8,
    steep: "Rock",
    deep: "Rock",
    // Parameter-space distance at which a neighbouring biome's weight reaches zero.
    transition: 0.15,
    // Vertical spacing of the smoothed staircase; terrace_strength 0 disables it.
    terrace_step: 8.0,
    entries: [
        (
            name: "Plains",
            parameters: (temperature: 0.0, humidity: 0.0, erosion: 0.2),
            shape: (
                height_offset: -4.0,
                relief_multiplier: 0.8,
                terrace_strength: 0.0,
                cave_density: 0.8,
                overhang_strength: 2.0,
            ),
            surface: (top: "Grass", subsurface: "Dirt", subsurface_depth: 3),
        ),
        // Forest, Desert, Tundra and Roughlands follow the same shape.
    ],
),
```

Neutral values are `height_offset: 0.0`, `relief_multiplier: 1.0`, `terrace_strength: 0.0`,
`cave_density: 1.0` and `overhang_strength` equal to `caves.overhang_strength`. Starting values,
tuned in the final step against the report, surface map and screenshots:

| Biome | offset | relief | terrace strength | cave density | overhang |
|---|---:|---:|---:|---:|---:|
| Plains | -4.0 | 0.8 | 0.0 | 0.8 | 2.0 |
| Forest | 0.0 | 0.9 | 0.0 | 1.0 | 3.0 |
| Desert | 2.0 | 1.0 | 0.6 | 0.5 | 1.0 |
| Tundra | 4.0 | 1.1 | 0.2 | 0.6 | 2.0 |
| Roughlands | 6.0 | 1.4 | 0.3 | 1.5 | 6.0 |

The cavern carve threshold is 0.32 over a field that stays roughly inside `-1..=1`. A density
below about 0.32 cannot carve at all, and densities between 0.32 and 1 need increasingly rare high
samples. The starting values deliberately keep Desert and Tundra sparse; the report prints the
measured cave fraction per biome so tuning can raise a density if that biome is meant to show
caves, and the screenshots are taken from the measured fractions rather than assumptions.

Validation additions, each message naming the biome and the field
(`biomes.entries[2] (Desert).shape.terrace_strength ...`):

- `biomes.transition` is finite and within `0.1..=1.0`.
- `biomes.terrace_step` is finite and within `1..=32` blocks.
- `height_offset` is finite and within `-32..=32`.
- `relief_multiplier` is finite and within `0..=2`.
- `terrace_strength` is finite and within `0..=1`.
- `cave_density` is finite and within `0..=2`.
- `overhang_strength` is finite and within `0..=caves.overhang_strength`.

`caves.overhang_strength` is validated before `validate_biomes` and regardless of
`caves.enabled`, so the per-biome cap always compares against a validated bound and a non-finite
global value produces one clear error instead of a misleading biome error. The maximum
`transition` is a tuning guard, not a correctness constraint.

The existing checks (names, duplicate parameters, duplicate top kinds, kind resolution,
subsurface depth) are unchanged. The format is intentionally breaking again, like the surface
move in #108: there is one shipped asset and no compatibility shim.

## Code changes

| File | Change |
|---|---|
| [`terrain/config.rs`](crates/server/src/genesis/terrain/config.rs) | `BiomeShapeConfig`, `BiomeConfig.shape`, `BiomesConfig.transition` and `terrace_step`, validation with the biome name in every path, rejection tests |
| [`terrain/biome.rs`](crates/server/src/genesis/terrain/biome.rs) | `BiomeTable` keeps resolved shape values; `weights_into(t, h, e, out) -> usize` fills normalized weights from a caller-owned scratch buffer and returns the squared-distance dominant index; `select` keeps the same squared-distance loop for tests while `weights_into` is the production path; `blended_shape(&weights)` returns the five summed parameters |
| [`terrain/mod.rs`](crates/server/src/genesis/terrain/mod.rs) | `sample_column(x, z, weights) -> ColumnSample`; `samples(chunk) -> ChunkSamples` builds the 18x18 halo, the per-column samples and the 16x16 slopes; `generate` consumes `samples`; `is_solid` takes `&ColumnSample`; `height` and `slope` remain crate-private probes; `lattice_bounds` unchanged except for its doc comment |
| [`terrain/surface.rs`](crates/server/src/genesis/terrain/surface.rs) | unchanged; it keeps receiving depth and slope |
| [`assets/worldgen.ron`](assets/worldgen.ron) | `transition`, `terrace_step`, `shape` blocks, tuned values |
| [`archive/src/lib.rs`](crates/archive/src/lib.rs) | `VERSION_MAGIC` trailing byte becomes `0x06`, comment: version 6 changes the generated world through biome shaping and blended height |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | world generation subsection describes weights, shape parameters, the terrace blend and the archive regeneration step |
| [`docs/ai-generated/biome-proposal.md`](docs/ai-generated/biome-proposal.md) | follow-ups point height shaping at #109 and keep blended transitions out of scope |
| [`terrain_generation.rs`](crates/server/tests/terrain_generation.rs) | keep the kind and cave assertions; surface expectations still come from the shipped config |
| [`terrain/mod.rs`](crates/server/src/genesis/terrain/mod.rs) tests | new and updated unit tests for weights, blending, terrace, cave density, overhang and the world-space probe, plus `biome_terrain_report`, which prints per-biome height, slope, cave and saturation statistics and representative world columns |

No ECS, system ordering, protocol, mesh layout, client asset or example changes.
`CHUNK_MESH_VERSION` stays 9: the mesh format is untouched, and a world-generation change is gated
by `VERSION_MAGIC` because it invalidates the stored kinds, light and vertices together.

### Column cache and halo

Today `generate` reads 22 2D field values per column: four in `column`, four `height` calls with
four samples each through `slope`, and two climate samples. That is 5,632 field evaluations per
chunk. Weights need temperature and humidity on every height evaluation, so recomputing neighbours
through `height` would raise it to 30 per column, 7,680 per chunk.

`samples(chunk)` builds one `ColumnSample` for each column of an 18x18 halo (16 plus one column of
margin on each axis), which is `324 * 10 = 3,240` field evaluations per chunk, about 42% below
the old 5,632 (ten fields per halo column: three shape, detail, two climate, landform, upheaval and
two turbulence directions). The halo also gives one authoritative value per world column for height, slope, dominant
biome, overhang strength and cave density. The 16x16 interior is copied into the fill pass and the
18x18 grid is dropped with the task. A `ColumnSample` is two f32, one `usize` and two more f32, so
the grid is under 10 KiB including the 16x16 slope array.

`samples` is crate-private so the tests can compare its cached slopes with the `slope` probe for
every column; without that accessor the cache is not testable. `height` allocates a small weight
scratch per call, which is acceptable because production generation does not call it; `generate`
allocates one scratch vector per chunk and reuses it for every halo column, so there is no
per-column allocation and no fixed biome cap.

## Staged commits

One PR, four commits. Each commit builds and passes `just ci`. Commits 1 to 3 keep the shipped
world bit-identical to #108, and the archive version bump lands with the first commit that changes
the generated world.

### Commit 1: configuration and validation

- Add the types, `worldgen.ron` fields with neutral shapes and the shared `transition: 0.15` and
  `terrace_step: 8.0`.
- Add the rejection tests, including assertions that the error text contains the biome name and
  the field, and the validation-order test for a non-finite global overhang strength with caves
  disabled.
- No generation change: the neutral shape values describe the current world exactly.

Acceptance: `cargo test -p projekto_server`, `just ci`.

### Commit 2: blended selection and height shape

- Implement `weights_into`, `select` through it, `blended_shape`, the delta blend, the height
  formula and the terrace helper.
- Add `sample_column`, `samples` and the halo; `generate` consumes the cached samples; `height`
  and `slope` read the same column path.
- Update the height, surface, chunk-border and cache-consistency tests to the new column path,
  and add an independent-formula test for a synthetic config with terrace strength zero: the test
  recomputes the raw height from the noise fields instead of calling `height`.
- Ship neutral shapes so the generated world stays identical; add synthetic-config tests that set
  one biome's offset, multiplier or terrace and assert the dominated columns move.
- Add the boundary tests with a synthetic two-entry config: bisect the exact point where the two
  distances are equal, assert the weights are about 0.5/0.5 there, and assert the height at
  `point - 0.001` and `point + 0.001` differs by less than 0.05 blocks. A scan at 0.01-block steps
  asserts the same absence of first-order jumps. An unblended implementation with offsets of
  `+20` and `-20` fails this by a wide margin.
- Add the `transition` sweep test and the neutral-shape no-op test.
- Save the parent commit's `terrain-map` output under `debug_reports/baseline/` and compare the
  new output with `cmp`; identical bytes prove the refactor is a no-op with neutral shapes.

Acceptance: `cargo test -p projekto_server`, `just ci`, identical `terrain-map` baseline.

### Commit 3: cave density and overhang strength

- Extend `ColumnSample` and pass the per-column cave density and overhang strength into
  `is_solid`; stop reading the global values there.
- Update `overhang_strength_moves_the_surface_up` to raise the per-entry
  `shape.overhang_strength` values as well as the global ceiling; raising only the global value no
  longer moves the surface because `is_solid` reads the per-column strength.
- Add tests: an all-zero cave density config carves no cavern voxels in any generated chunk, and
  columns whose cached blended density is exactly zero stay solid, including with a negative
  cavern threshold; density two carves more than density zero over the same columns; with every
  overhang strength zero, every cave density zero and the carve threshold at 1.0, the top voxel is
  `ceil(height) - 1` on every column; a lifted surface appears when the strength is positive.
- Keep neutral values so the shipped world is unchanged.

Acceptance: `cargo test -p projekto_server`, `just ci`, identical `terrain-map` baseline.

### Commit 4: tuning, archive bump and validation

- Apply the tuned shipped values.
- Bump `VERSION_MAGIC` to 6 and add its comment.
- Delete and regenerate `archive/region`, then run the archive checks. The server already warns
  and regenerates an asset stored with the wrong magic, so deletion is a clean-run step rather
  than a correctness requirement; the version bump is what invalidates the old data.
- Run the biome report, the surface map, the visual pass, the genesis bench and the full gate.
- Update `ARCHITECTURE.md` and the proposal follow-ups, and document the regeneration step:
  a world-generation change requires the version bump, and deleting the local `archive/region`
  gives a clean visual run; per-asset reads reject older magic.
- Make `each_biome_generates_its_surface_sequence` deterministic: run it with caves disabled and
  all overhang strengths zero so a surface run is never interrupted, scan the 2048x2048-block area
  at the 8-block step for a flat column per biome, and assert the full top/subsurface/deep
  sequence for every biome; a biome with no flat column at all fails with its name.
- Address every `//@AI` marker in touched files and state in the PR description that the
  implementation is AI-assisted, per `AGENTS.md`.

Acceptance: the full validation table below.

## Tests

Unit tests in `genesis::terrain` (the generator and biome table stay crate-private):

| Test | Assertion |
|---|---|
| `weights_are_a_normalized_partition` | weights sum to one, nearest entry dominates, values stay finite with large coordinates |
| `weights_blend_smoothly_across_a_boundary` | crossing a pairwise Voronoi boundary by `1e-3` changes weights smoothly, and the weights are about even at the equality point |
| `transition_sweep_keeps_the_boundary_continuous` | synthetic configs with `0.1`, `0.15` and `0.5` all keep the blended boundary jump below the bound |
| `shape_parameters_do_not_affect_selection` | weights and dominant index are bit-identical after every shape field is changed |
| `tie_resolves_to_the_first_entry` | equal distances select the earlier entry and split weights evenly |
| `neutral_shapes_do_not_change_the_height` | with all shapes neutral, the blended height equals the terrain formula at sampled columns |
| `blended_height_matches_an_independent_formula` | a synthetic terrace-free config's height equals a test-side recomputation from the raw noise fields and weights |
| `offset_and_relief_blend_at_an_equidistant_point` | a synthetic two-entry config yields the mean offset and multiplier at the midpoint |
| `terrace_blend_matches_sample_column` | strengths 0, 0.5 and 1 match an independently computed terrace through `sample_column` |
| `terrace_is_continuous_at_step_edges` | the staircase matches on both sides of a cell edge and its deviation stays within `0.1 * step` |
| `height_has_no_jump_across_a_synthetic_biome_boundary` | the bisected synthetic boundary has no first-order height jump |
| `height_has_no_jump_across_a_shipped_biome_boundary` | same check with the tuned shipped config |
| `cached_slope_matches_the_probe` | `samples` slopes equal `TerrainGenerator::slope` for every column of generated chunks |
| `cave_density_zero_removes_caverns` | with every overhang strength zero, an all-zero density config carves nothing anywhere and columns whose cached blended density is exactly zero stay solid, for positive and negative thresholds |
| `cave_density_increases_carved_volume` | with every overhang strength zero, carved count with density two exceeds density zero over the same columns |
| `overhang_strength_zero_keeps_the_surface_at_height` | with caves enabled, every overhang strength zero, every cave density zero and the carve threshold at 1.0, the top voxel is `ceil(height) - 1` on every column |
| `overhang_strength_moves_the_surface_up` | with cave density zero, a positive per-entry overhang strength lifts at least one sampled column |
| `height_stays_within_configured_bounds_and_varies` | existing test, now over the blended height |
| `height_is_continuous_across_a_chunk_border` | existing test at 15.99/16.01 |
| `world_space_surface_matches_generated_chunks` | recomputes height, slope and surface biome from world coordinates through the probe functions (validating the halo mapping), compares every generated top kind with the surface rule, and checks top voxels against the generated height where no 3D effect applies |
| `caves_disabled_top_matches_height` | with caves disabled, the top solid voxel of every generated column is `ceil(height) - 1` |
| `biome_distribution_stays_within_bounds` | unchanged; selection is unchanged by shape parameters |
| `biome_terrain_report` | prints per-biome column counts, height percentiles, mean slope, carved fraction, clamp saturation and representative columns; not a pass/fail gate |
| `each_biome_generates_its_surface_sequence` | with caves disabled and overhang zero, every biome's shipped surface sequence is found from a flat scanned column and asserted; deterministic, no retry loop |
| `same_seed_generates_the_same_chunk` | determinism is unchanged |
| `shipped_config_has_five_named_biomes` | unchanged, with the new shape blocks present |

Integration test [`terrain_generation.rs`](crates/server/tests/terrain_generation.rs) keeps the
kind and cave assertions. The per-biome shape and 3D claims are covered by unit tests with
synthetic configs, which are deterministic and do not depend on tuned distribution percentages.

## Visual validation

`cargo test -p projekto_server biome_terrain_report -- --nocapture` prints, per biome, the column
share, height percentiles, mean slope, carved fraction, pre-clamp saturation and representative
world columns: the flattest column, the steepest column and a column where a cave interrupts the
surface run. Those columns are the camera targets below, so the visual pass uses measured terrain
rather than guessed coordinates.

`just terrain-map 64` produces the height, surface and vertical-slice maps. The height map shows
the five base elevations and relief amplitudes; the surface map shows the biome tops (Roughlands
shares Rock with steep slopes, as documented in #108, so it is not a biome counter); the slice
shows the cave and overhang character.

`just cia` renders the offscreen AI client and writes `debug_reports/ai/latest.png`. The AI client
exposes BRP on port 15722; position the camera per screenshot and request an immediate capture:

```sh
curl -s -X POST http://127.0.0.1:15722 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"projekto/set_camera","params":{"position":[x,y,z],"target":[x,y,z]}}'
curl -s -X POST http://127.0.0.1:15722 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"projekto/ai_snapshot"}'
```

Capture at least:

- one overview per biome at radius 24, camera above the biome centre, to show base elevation and
  relief;
- one low camera at a biome boundary, to show that height blends and only the material changes;
- one camera at the steepest measured column of Desert and Roughlands, to show terraces and
  cliffs;
- one camera inside a cavern in the two or three biomes with the highest measured carved fraction.
  The report's table covers all five biomes, so a biome with sparse caves is documented by its
  number instead of a forced screenshot.

Then inspect the PNGs, record the camera positions next to each screenshot in the PR description,
and delete the locally regenerated `archive/region` if it is not wanted in the working tree (it is
gitignored either way).

## Validation gates

| Check | Command | Pass condition |
|---|---|---|
| Core | `cargo test -p projekto_core` | kinds and kind metadata tests pass |
| Server | `cargo test -p projekto_server` | generator, selection, shape, cave and integration tests pass |
| CI | `just ci` | fmt, clippy, tests and udeps pass |
| Neutral no-op | `cmp` of `just terrain-map 64` against the saved parent-commit maps | byte-identical height, surface and slice maps after commit 2 |
| Boundary | `cargo test -p projekto_server height_has_no_jump` | no first-order height jump across synthetic and shipped boundaries |
| Probe | `cargo test -p projekto_server world_space_surface_matches_generated_chunks` | every cached column and generated top kind matches the world-space recompute |
| Bench | `cargo bench -p projekto_server --bench genesis` (five runs) | median ms/chunk within 10% of `main` at 507145f; report the before/after range |
| Surface map | `just terrain-map 64` | five distinguishable biome height profiles; terraces and cave slice visible |
| Visual | `just cia` | `debug_reports/ai/latest.png` shows each biome's shape, terraces and cliffs without a height seam at a boundary |
| Caves | report numbers plus cave captures | each biome's carved fraction is printed; caves are visible in the highest-fraction biomes |
| Archive | `rm -rf archive/region`, then a server run; `just archive verify archive/region`, `just archive holes archive/region` | regenerated archive verifies clean |
| Review markers | `rg -n '//@AI'` | no marker left unresolved |

The bench baseline is measured on commit 1's parent with the same `PROJEKTO_GENESIS_BENCH_CHUNKS`
and thread count. The halo cache is expected to keep the median at or below today's, because it
replaces 22 field evaluations per column with ten per halo column, still fewer in total. `just terrain-walk` is a smoke
check on the final config; no streaming or LOD path changes, so it is not a gate.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Terraces push slopes above `max_slope`, exposing Rock over Desert and Tundra tops | Tune `terrace_strength` against the surface map; `max_slope` stays global until per-biome slope thresholds earn a separate issue |
| Offsets and multipliers push peaks into the top clamp | The shipped nominal top envelope stays under `max_height`; the report prints pre-clamp saturation and tuning lowers the values if it grows |
| The floor clamp already flattens deep valleys; a relief multiplier above one deepens them | Report bottom saturation per biome and keep the Roughlands multiplier at or below 1.4; the clamp behavior predates this change |
| A biome wants a stronger overhang than the shared ceiling | The cap keeps the lattice margin proof; raising the global value also raises sampling cost, which the bench covers |
| Weighted selection adds cost to the hot genesis path | The 18x18 halo replaces four `height` recomputations per column; the bench gate compares medians and the fallback is a squared-distance kernel |
| A very small `transition` reintroduces a one-column edge | Validation rejects `transition < 0.1`; the sweep test covers the lower bound |
| Material edges remain hard at biome boundaries | Accepted and documented: kinds cannot be interpolated; the issue's continuity requirement is height, and #108 already accepted one-column material edges |
| A missed test update lets the cache or formula drift from generation | Cache-consistency, independent-formula and generated-top-voxel tests pin the halo mapping, the height formula and the voxel fill |
| Old local archives keep the old world | Version 6 magic rejects them per asset; the server logs and regenerates, and the regeneration step is documented and run before the visual pass |
| f32 order or iteration order changes results | Table order is fixed, dominance uses the current squared-distance comparison, no sorting and no map iteration; determinism tests stay |
| Tuning overfits the distribution test or makes a biome too rare | The distribution test is unchanged because selection is unchanged; per-biome numbers are printed for the tuning decision |

## Decisions for the author

- Parameter caps: `height_offset` in `-32..=32`, `relief_multiplier` in `0..=2` and
  `transition` in `0.1..=1.0` are validation constants, not terrain constants.
- `terrace_step` is global rather than per biome. A per-biome step would add a `grad step` term to
  the height gradient and reintroduce a discontinuity if the dominant step were switched at a
  boundary; the per-biome terrace character comes from `terrace_strength` and relief.
- Materials stay hard-edged at biome boundaries; smoothing them would need per-kind blending or a
  different surface representation and is out of scope alongside blended transitions.

## Addendum: Vintage Story-inspired terrain redesign

After the first visual pass the shape fields still read as uniform noise hills. A review of
Vintage Story's world generation showed that the scale and "living" feel come from large domain
warp, tectonic uplift and per-landform vertical profiles, not from larger noise amplitude alone.
This addendum records the redesign layered on top of the biome-shaping plan above.

Research sources: the Vintage Story wiki pages for
[Terrain Generation](https://wiki.vintagestory.at/Terrain_Generation),
[WorldGen Configuration](https://wiki.vintagestory.at/Modding:WorldGen_Configuration) and
[WorldGen Concept](https://wiki.vintagestory.at/Modding:WorldGen_Concept), plus the public
TerraPrety copy of the vanilla generator for warp/upheaval constants.

Design changes:

- **Domain warp.** Two low-frequency turbulence fields displace the shape sampling position by up
  to `warp_strength` blocks. Continentalness, erosion, weirdness, the landform mask and upheaval
  are all read at the warped position, so ridges and profile boundaries stop lining up with the
  noise lattice.
- **Landform profiles.** `landform_plains` and `landform_mountains` map continentalness to height.
  A very low-frequency `landform` field is mapped through a smoothstep to a blend weight. The
  profiles differ in both elevation and vertical scale, which is what makes mountain regions read
  as mountains instead of taller hills.
- **Tectonic uplift.** A low-frequency `upheaval` field maps through `upheaval_lift` to positive and
  negative blocks. The lift is scaled by the landform weight (`0.4 + 0.6 * weight`) so lowlands are
  not lifted uniformly and long slopes lead into mountain fronts.
- **Ridged relief.** The existing peaks-and-valleys term is scaled by the landform weight and stays
  smaller than the profile change, so it adds ridges without turning the whole world into noise.
- **Soft top clamp.** The last 16 blocks below `max_height` map through a rational asymptote. Rare
  peaks are compressed instead of becoming perfectly flat plateaus, and the shipped report counts
  columns above that knee. The landform weight also scales uplift from 40% to 100% and relief from
  60% to 140%.
- **Biome scale and material transitions.** Climate frequencies were lowered so biome cores span
  hundreds of blocks, and `transition` was widened to 0.4. Inside the transition band,
  `BiomeTable::surface_biome` picks the surface biome from the blend weights using a deterministic,
  uniform cell hash (`biomes.material_blend_cell`, default 12 blocks). Uniform jitter makes each
  material's realized share follow its weight, and cell-sized patches interleave instead of
  switching in one column.
- **Terrace and biome blending** stay as planned: one shared terrace after blending, biome shape
  fields blended by weights.

Vintage Story itself fills voxels from a 3D noise compared against a Y-dependent threshold curve,
which is what permits cliffs, benches and overhangs inside a column. Projekto still generates from
a column height, so this redesign reproduces the large-scale feel (warp, uplift, profiles) but not
true non-heightfield overhangs; that would be a separate, larger change.

New fields and settings: `fields.landform`, `fields.upheaval`, `fields.turbulence_x`,
`fields.turbulence_z`, `landform_plains`, `landform_mountains`, `upheaval_lift`, `warp_strength`
and `biomes.material_blend_cell`.
Tests cover the landform weight curve, warp/profile/uplift wiring, soft-clamp monotonicity and the
existing height, boundary, halo, cave and biome checks.

---

*AI-drafted plan; the author reviews and owns it.*
