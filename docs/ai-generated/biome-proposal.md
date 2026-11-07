# Biome generation

## Context

Issue #42 replaced the single noise field with a layered world generator: four 2D shape fields
combined by splines ([`TerrainGenerator::height`](crates/server/src/genesis/terrain/mod.rs#L90)),
3D cave and overhang fields, and one global surface rule set. The issue closed with biomes
explicitly deferred, so the pre-biome world had one surface treatment: Grass over Dirt over Rock,
with Rock on steep slopes.

This proposal adds climate-driven biomes on top of that pipeline. The starting set has five
biomes: Plains, Forest, Desert, Tundra and Roughlands. Terrain shape, light propagation and the
client protocol stay unchanged.

## Scope

In scope:

- two climate fields, sampled in world coordinates;
- a data-driven biome table, per-column biome selection and per-biome surface rules;
- three new kinds, Sand, Moss and Snow, using existing hand-made atlas tiles with per-kind color
  multipliers;
- a per-kind color multiplier applied to vertex light while meshing, which tints those tiles;
- validation, distribution and world-space surface tests, `terrain-map` colors and an archive
  version bump.

Out of scope:

- water, oceans, beaches, rivers and aquifers;
- biome-specific dunes and blended biome transitions; height shaping landed in #109;
- trees (#43), structures, ore veins and bedrock;
- hand-painted Sand, Moss and Snow tiles, biome-dependent grass or foliage tinting beyond the
  per-kind color multiplier, and any client message or mesh format change.

## Pipeline

```text
shape(x, z)    -> height, erosion
climate(x, z)  -> temperature, humidity
biome          -> nearest point over (temperature, humidity, erosion)
surface        -> top, subsurface and depth for that biome
```

Every field is sampled at world coordinates. A column produces the same shape, climate and
biome no matter which chunk contains it.

Height remains a pure function of continentalness, erosion, weirdness and detail. The biome
pass reads the shape but never writes it. This keeps the existing height tests and chunk-border
continuity intact, and it prevents biome selection and biome height from depending on each
other.

## Climate fields

Two new 2D fields use the existing [`FieldConfig`](crates/server/src/genesis/terrain/config.rs#L48)
and [`NoiseField`](crates/server/src/genesis/terrain/noise.rs#L8):

| Field | Frequency | Octaves | Gain | Lacunarity | Seed tag |
|---|---:|---:|---:|---:|---:|
| temperature | 0.0015 | 3 | 0.5 | 2.0 | 7 |
| humidity | 0.0020 | 3 | 0.5 | 2.0 | 8 |

Both use `FastNoise` FBM, like the shape fields. Seeds come from
[`derive_seed`](crates/server/src/genesis/terrain/noise.rs#L35), so they are independent from
continentalness, erosion, weirdness and detail and from each other.

The frequencies put climate regions in the hundreds of blocks, which is the scale biomes
should have. A 1M-sample probe on a 4-block grid, using these parameters and a fixed probe seed,
gives 10th and 90th percentiles of about `-0.36` and `+0.36` for each field, with a maximum near
`+0.83`. Biome points placed just outside those deciles do not depend on rare tail values.

## Biome table

Biome selection is a nearest-point lookup in `(temperature, humidity, erosion)` with squared
Euclidean distance and equal weights. The first entry wins an exact tie, and entry order in
`worldgen.ron` makes that deterministic. Duplicate parameter triples are rejected at load time,
because a duplicate can never win.

Erosion is part of the lookup because it already controls relief amplitude
([`relief_amplitude`](crates/server/src/genesis/terrain/mod.rs#L102)): low erosion produces the
largest height swings. Erosion is already sampled by the shape pass, so this adds no extra noise
evaluation.

Absolute height is not part of the lookup. Over a 33×33 chunk sample at seed 42, surface height
spans roughly 102 to 138 blocks between the 10th and 90th percentiles inside the configured
70..240 range. Using height as a biome axis would produce narrow elevation bands and tie biome
selection to the terrain it is supposed to decorate.

A 2 km probe at seed 42 confirms the difference is roughness, not altitude: Roughlands median
height is 119 blocks (world median 119) while its median slope is 1.08 (world median 0.94).
The biome therefore claims low-erosion, high-relief ground, and the name says so.

| Biome | Temperature | Humidity | Erosion | Surface (top / subsurface / depth) |
|---|---:|---:|---:|---|
| Plains | 0.0 | 0.0 | 0.2 | Grass / Dirt / 3 |
| Forest | 0.0 | 0.4 | 0.2 | Moss / Dirt / 3 |
| Desert | 0.4 | -0.4 | 0.2 | Sand / Sand / 3 |
| Tundra | -0.4 | 0.0 | 0.2 | Snow / Dirt / 2 |
| Roughlands | 0.0 | 0.0 | -0.4 | Rock / Rock / 3 |

Global surface settings:

- `max_slope = 1.8` — slopes above it expose Rock in every biome;
- `steep = Rock`, `deep = Rock` — cliffs and cave ceilings keep the existing treatment;
- cave ceilings below a solid surface keep the deep kind, because the surface pass only walks
  down from the highest solid voxel, as it does today.

Top kinds are unique across the five entries, so a duplicate top is a config error. Roughlands
uses Rock, the same kind as steep slopes, so the surface map is a visual aid rather than a biome
count; the distribution and per-biome surface tests below are authoritative.

The parameter values are starting points chosen just outside the measured deciles. The
distribution test below enforces that no biome becomes too rare or too dominant; Step 3 tunes
the points against real maps.

## New kinds

| Kind | ID | Atlas offset | Color | Use |
|---|---:|---|---|---|
| Sand | 5 | (2, 0) | (1.9, 1.7, 1.1, 1.0) | Desert top and subsurface |
| Moss | 6 | (1, 0) | (0.7, 1.0, 0.6, 1.0) | Forest top |
| Snow | 7 | (2, 0) | (2.0, 2.1, 2.3, 1.0) | Tundra top |

The three kinds reuse existing hand-made tiles instead of adding new artwork: Sand and Snow tint
the Rock tile at (2, 0), and Moss tints the grass top tile at (1, 0). Project policy forbids
AI-generated media, so the dedicated 32×32 hand-painted tiles are a follow-up rather than part of
this change. Until then, the color multipliers keep the five biome surfaces distinguishable.

[`KindSideTexture::color`](crates/core/src/voxel/kind.rs#L13) multiplies vertex light in both
meshing paths: [`generate_vertices`](crates/server/src/meshing/mod.rs#L52) for full meshes and
[`generate_coarse_vertices`](crates/server/src/meshing/lod.rs#L66) for LOD 1. The shader multiplies
the sampled tile by that light, so the tint needs no atlas, mesh format or protocol change. Values
above 1 brighten the tile, and the existing kinds keep (1.0, 1.0, 1.0, 1.0), so only the three new
kinds change. No atlas PNG or XCF file changed.

All three are full opaque blocks, so lighting, face culling and greedy meshing need no special
case. The existing 10×10 atlas keeps `atlas_size = 320` and `atlas_tile_size = 32`.

Kinds are appended after Lamp in [`kind.ron`](assets/voxels/kind.ron); IDs are never reordered,
because archives store `Kind(u16)` values.

[`check_descs`](crates/core/src/voxel/kind.rs#L137) gains an assertion that every texture offset
is inside the atlas on both axes. The existing
[`load_kind_descriptions`](crates/core/src/voxel/kind.rs#L284) test is updated to call
`check_descs` so the shipped asset exercises it, and a second test feeds a synthetic
out-of-bounds offset and expects the panic.

## Configuration

The top-level `surface` block moves under `biomes`, and the climate fields join `fields`:

```ron
fields: (
    continentalness: (frequency: 0.0025, octaves: 4, gain: 0.5, lacunarity: 2.0),
    erosion: (frequency: 0.004, octaves: 3, gain: 0.5, lacunarity: 2.0),
    weirdness: (frequency: 0.008, octaves: 3, gain: 0.5, lacunarity: 2.0),
    detail: (frequency: 0.06, octaves: 2, gain: 0.5, lacunarity: 2.0),
    temperature: (frequency: 0.0015, octaves: 3, gain: 0.5, lacunarity: 2.0),
    humidity: (frequency: 0.002, octaves: 3, gain: 0.5, lacunarity: 2.0),
),
biomes: (
    max_slope: 1.8,
    steep: "Rock",
    deep: "Rock",
    entries: [
        (
            name: "Plains",
            parameters: (temperature: 0.0, humidity: 0.0, erosion: 0.2),
            surface: (top: "Grass", subsurface: "Dirt", subsurface_depth: 3),
        ),
        // Forest, Desert, Tundra and Roughlands follow the same shape.
    ],
),
```

[`WorldGenConfig::validate`](crates/server/src/genesis/terrain/config.rs#L170) adds these checks:

- `biomes.entries` is not empty;
- each name is non-empty and unique;
- each parameter triple is finite and not duplicated;
- each top kind is unique across entries;
- `biomes.max_slope` is finite and non-negative, moving the check that lived in the removed
  top-level `surface` block;
- every top, subsurface, steep and deep name resolves through
  [`Kind::from_name`](crates/core/src/voxel/kind.rs#L268) and is not the empty kind;
- `subsurface_depth` stays within the existing 0..=16 bound;
- the temperature and humidity fields pass the existing field validation.

The code does not require exactly five entries. A shipped-asset test asserts that
[`worldgen.ron`](assets/worldgen.ron) contains the five names above, so adding a sixth biome is a
config change once its top kind exists in `kind.ron`.

[`FieldSet`](crates/server/src/genesis/terrain/config.rs#L37) now describes the fields as the
height and climate fields, instead of the height model only.

Custom `worldgen.ron` files must move their surface block into a biome entry. The format is
intentionally breaking; there is one shipped asset and no compatibility shim.

## Code layout

```text
crates/server/src/genesis/terrain/
  config.rs   FieldSet gains temperature and humidity; BiomesConfig and BiomeConfig
  biome.rs    BiomeTable: selection, entries and resolved surface kinds
  surface.rs  SurfaceRule: top, subsurface and depth for one biome
  mod.rs      column pass: shape, climate, selection, fill
```

`TerrainGenerator` gains `temperature`, `humidity` and a `BiomeTable`. `height` is refactored into
a column helper that returns height and the erosion it already sampled; `height` remains as a
crate-internal wrapper for tests and existing call sites. `generate` caches `heights`, `slopes`
and biome indices per column, then uses the selected biome for the surface run.

The `BiomeTable` resolves kind names to `Kind` once per generator, so generation never does
string lookups. It exposes:

- `select(temperature, humidity, erosion) -> usize`;
- `surface(biome, depth, slope) -> Kind`;
- `surface_depth(biome) -> i32`;
- `deep() -> Kind`.

`Lattice`, `is_solid` and `init_light` are untouched. Both meshing paths apply the kind tint to
vertex light, which changes vertex values only, not the mesh or protocol layout.

## Boundaries and determinism

Biome boundaries are hard material changes, not seams:

- height is continuous because it never reads biome data;
- both chunks sample the same world coordinates and run the same selection, so each generated
  column matches the generator's world-space height, slope, biome and surface;
- a surface change happens at one column, like the existing Rock-on-slope rule.

Blended biome transitions are a separate change. A blend needs either weights from the
selection or biome-shaped height, and height shaping is deliberately out of scope here. A hard
surface edge is the smallest change that cannot introduce a terrain seam.

Ties are resolved by table order. Selection uses `<`, never `partial_cmp` or sorts, so the result
does not depend on the order columns are visited.

The shape seeds keep their existing tags and values, so height is bit-identical to the current
generator. Only the new climate seeds change, and only the surface output changes.

## Assets, archive and client

- Bump [`VERSION_MAGIC`](crates/archive/src/lib.rs#L56) to version 5 in Step 2, with a comment
  that version 5 adds biomes and their surface kinds.
- Delete and regenerate `archive/region` in Step 2, before any `just cia` run, so the archived
  area and newly generated chunks use the same surface. Step 3 runs
  `just archive verify archive/region` and `just archive holes archive/region` on the regenerated
  archive.
- `CHUNK_MESH_VERSION` bumps to 9 because both meshers now write tinted vertex light; vertex
  layout, UV layout and tile sizing are unchanged.
- The server resolves kinds to atlas UVs and the tint before meshing. The client receives the same
  `ChunkVertex` layout and the unchanged atlas; the three kinds reuse existing tiles, so no client
  asset update is required.
- No message enum, handler or serialization format changes.

## Performance

The shape pass samples four fields about five times per column because of `slope`. Climate adds
two 2D samples per column, 512 per chunk, so the shape pass itself evaluates about ten percent
more samples. Total genesis time grows less than that because fill and packing are unchanged.
The only new per-chunk kind-name work is the `BiomeTable` construction resolving twelve kind
names per generator. Biome selection adds five three-component distance checks per column.

Measure with `cargo bench -p projekto_server --bench genesis` on 256 chunks, five runs, and
compare medians:

- target: no more than 10% more median milliseconds per chunk, with the before/after range
  reported;
- record radius-32 RSS through the server BRP `projekto/memory` method as information only;
  archive/cache state makes it noisy. New kinds can grow a surface section's palette from 4 to 8
  entries, moving bit-packed indices from 2 to 3 bits in boundary sections. Update #91 only if a
  clean same-archive comparison shows material growth;
- extend the bench to print `Kind::name()` alongside the count so Sand, Moss and Snow are
  identifiable;
- if the target is missed, the first lever is two octaves for the climate fields, the second is
  sampling climate on a 4-block lattice and interpolating. Both change sampled climate values and
  therefore selection near boundaries; the selection algorithm, table format and surface rules
  do not change.

`GenesisServer` still builds a `TerrainGenerator` per task from a cloned config. The biome table
adds five small entries; the bench includes that construction cost. A shared immutable generator
is a separate optimization if the entry count grows.

## Streaming and LOD

No chunk lifecycle, section-storage or LOD logic changes. The LOD mesher picks the highest solid
voxel kind per coarse cell, so biome tops remain visible at LOD 1 and above. New kinds can grow
surface-section palettes and bit-packed index widths in boundary sections. Run `just terrain-walk`
once on the final config; the longer `terrain-stress` run is not required because no streaming
path changed. Record the radius-32 RSS as information only; there is no hard memory gate.

## Staged plan

Each step is its own PR with its own acceptance run.

### Step 1 — kinds and tint (small)

- Add Sand, Moss and Snow to [`kind.ron`](assets/voxels/kind.ron), reusing the Rock and grass-top
  tiles with per-kind color multipliers. Hand-painted tiles are a follow-up.
- Apply [`KindSideTexture::color`](crates/core/src/voxel/kind.rs#L13) to vertex light in
  [`generate_vertices`](crates/server/src/meshing/mod.rs#L52) and
  [`generate_coarse_vertices`](crates/server/src/meshing/lod.rs#L66), and add a unit test that the
  tint reaches the vertices.
- Extend [`check_descs`](crates/core/src/voxel/kind.rs#L137) with the atlas-bounds assertion, call
  it from [`load_kind_descriptions`](crates/core/src/voxel/kind.rs#L284), and add the
  out-of-bounds test.
- Update [`kind_color`](crates/server/examples/terrain_map.rs#L183) to match by kind name instead
  of numeric ID, and add the three colors.

Acceptance: `cargo test -p projekto_core`, `cargo test -p projekto_server`, `just ci`, and a
loaded world that shows the three tinted kinds at the expected offsets without a changed atlas.
This step changes kind metadata, meshing tint and tooling, not generated terrain.

### Step 2 — climate, selection and surfaces (medium)

- Add the climate fields and `BiomesConfig` to `worldgen.ron`, `FieldSet` and validation.
- Implement `BiomeTable::select` and per-biome surface rules.
- Move `max_slope`, `steep` and `deep` under `biomes`; remove the top-level `surface`; move the
  `max_slope` check with it.
- Update the tests that assume one global surface: the unit tests in `surface.rs`, the
  `config.rs` tests that mutate the surface kinds,
  the surface test in `mod.rs`, and
  `generated_terrain_has_surface_layers_and_caves` in
  [`terrain_generation.rs`](crates/server/tests/terrain_generation.rs).
- Add selection and validation tests: every entry's exact point selects that entry; duplicate
  triples, duplicate names, duplicate top kinds and an invalid `biomes.max_slope` are rejected;
  selection is deterministic for extreme values.
- Add surface tests per biome: depth 0 is the top, depths 1..=depth are the subsurface, and the
  next depth is the deep kind.
- Add a generated-column test: for each biome, find a flat column, generate its chunk and assert
  the top, subsurface and deep sequence, retrying a column if a cave interrupts the run.
- Add a world-space surface test: for every column of `(0,0)`/`(1,0)`, `(-1,0)`/`(0,0)`,
  `(0,0)`/`(0,1)` and `(0,-1)`/`(0,0)`, recompute height, slope, biome and expected top kind from
  world coordinates and compare with the generated top kind. Keep the existing height continuity
  check at 15.99/16.01.
- Add the distribution test: sample a 2048×2048 block square every eight blocks for seeds 42, 43
  and 44, print per-seed shares and fail if any biome is below 5% or above 40% of that seed's
  65,536 columns.
- Extend the genesis bench to print kind names; record the baseline.
- Bump the archive version and regenerate `archive/region` before the visual check.

All generator and selection tests are unit tests inside `genesis::terrain`; the generator and
`BiomeTable` are crate-private, and the integration test in `tests/` only updates its surface
assumptions.

Acceptance: `cargo test -p projekto_server`, `just ci`, `just terrain-map 64` shows the new
surface kinds across a 2064×2064 block map, `just cia` at radius 24 shows the biome surfaces
(three of them tinted) without seams, and the distribution and per-biome tests pass.

### Step 3 — tuning and release checks (small)

- Tune the frequencies and points, evaluating each candidate over seeds 42, 43 and 44 by changing
  `seed` in `worldgen.ron` between runs; every biome stays inside the distribution range.
- Run `just archive verify archive/region` and `just archive holes archive/region` on the
  regenerated archive.
- Run the genesis bench five times and record the median before/after.
- Keep the World generation subsection in `ARCHITECTURE.md` aligned with the final config,
  including shape, climate, biome selection, surface rules and the tint fallback.
- Run `just terrain-walk` once on the final config.
- Run `just ci` on the final config.

Acceptance: all validation below passes, and the final `worldgen.ron` is the one used for the
archive and screenshot checks.

## Validation

| Check | Command | Pass condition |
|---|---|---|
| Unit and integration | `cargo test -p projekto_server` | all pass |
| Core kinds | `cargo test -p projekto_core` | `kind.ron` loads, `check_descs` runs and offsets are inside the atlas |
| CI | `just ci` | fmt, clippy, tests and udeps pass |
| Distribution | `cargo test -p projekto_server biome_distribution -- --nocapture` | each biome between 5% and 40% of 65,536 columns for every seed |
| World-space surface | `cargo test -p projekto_server world_space_surface` | every column of two adjacent chunks on each horizontal axis matches an independent world-space recompute; height is continuous at 15.99/16.01 |
| Bench | `cargo bench -p projekto_server --bench genesis` | median of five runs at most 10% over baseline |
| Surface map | `just terrain-map 64` | new surface kinds visible across 2064×2064; visual aid only |
| Visual | `just cia` | `debug_reports/ai/latest.png` shows the five biome surfaces (three tinted) and no seams |
| Streaming | `just terrain-walk` | completes on the final config |
| Memory | server BRP `projekto/memory` at radius 32 | record only; compare with identical archive state |
| Archive | `just archive verify archive/region`, `just archive holes archive/region` | both clean |
| Review markers | `rg -n '//@AI'` | none left unresolved |

The distribution test samples a 2048×2048 block square around the origin every eight blocks for
seeds 42, 43 and 44. That is 65,536 columns per seed. Selection alone is cheap, so the test does
not generate chunks. The per-biome surface test reuses the same scan to find one flat column per
biome and generates only the chunks containing them.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| No water means no beaches or oceans; all five starting biomes are inland | Keep the land set; water adds Beach and Ocean entries and lowers the floor later |
| One biome becomes too rare or dominates; an exact point is always inside its own Voronoi cell, so skew is the real failure mode | Points sit just outside the measured deciles; the distribution test fails outside 5-40% per seed |
| Hard biome edges look artificial | Accept for the starting set; blending is a separate change with its own seam tests |
| Forest is only a Moss floor until trees land | Moss makes it visible now; #43 adds tree density to the same entry |
| Hand-painted Sand, Moss and Snow tiles cannot be AI-generated | Reuse existing hand-made tiles with color multipliers; dedicated tiles are a follow-up that changes only `kind.ron` offsets and colors |
| Tinted placeholders read as recolors and ship as final | The proposal documents the placeholders and the hand-painted tiles remain a tracked follow-up |
| Per-kind tint changes how existing kinds render | Existing kinds keep a (1.0, 1.0, 1.0, 1.0) multiplier, so only the three new kinds are tinted |
| Old archives keep the old surface | The version bump and archive regeneration happen in Step 2, before the visual check |
| Climate sampling pushes genesis over budget | Five-run median bench gate plus the two contingencies above |
| The surface map cannot count Roughlands because Rock is also the steep kind | The distribution and per-biome tests are the five-biome gate; the map is a visual aid |

## Follow-ups

- Hand-painted Sand, Moss and Snow tiles to replace the tinted placeholders; only their
  `kind.ron` offsets and colors change.
- Water, oceans, beaches and rivers, with a lower height floor and a separate water mesh pass.
- Blended biome transitions and material blending; #109 added selection weights and biome height
  shaping.
- Trees and other biome features in #43, driven by per-biome density fields.
- Biome-dependent grass and foliage tinting, which first needs a per-biome channel in the mesh or
  protocol beyond the per-kind color multiplier.
- More biomes, including swamp, savanna and badlands, as additional table entries with their own
  top kinds.

---

*AI-drafted proposal; the author reviews and owns it.*
