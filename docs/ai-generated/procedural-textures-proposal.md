# Procedural terrain textures

## Context

The client renders terrain from a 320×320 atlas of 32×32 tiles, and
[`kind.ron`](assets/voxels/kind.ron) maps every `Kind`/`Side` pair to a tile offset and an RGB tint
multiplier. The mesher bakes the tile origin into each vertex
([`generate_vertices`](crates/server/src/meshing/mod.rs#L52)), the fragment shader repeats the tile
across the quad ([`voxel.wesl`](assets/shaders/voxel.wesl#L59)), and the tint is applied to vertex
light on the server ([`generate_vertices`](crates/server/src/meshing/mod.rs#L72)).

This change retires the fixed spritesheet. The atlas is synthesized at client startup from the
world seed by [`projekto_textures`](crates/textures/Cargo.toml), and there is no bitmap or
spritesheet input: every tile, including grass, dirt, rock, sand, snow, moss and lamp, is generated
from code and authored numbers. Rendering performance, UV layout, vertex format and the protocol
do not change, because the output is still one atlas with the same tile offsets. The biome decides
colors and material motifs through the `Kind` bindings, and two different seeds produce visibly
different but coherent tile sets.

The five biomes in [`worldgen.ron`](assets/worldgen.ron) get their look from distinct `Kind`s:
Plains uses Grass over Dirt, Forest uses Moss over Dirt, Desert uses Sand, Tundra uses Snow and
Roughlands uses Rock. `Grass` binds a generated grass top, a composite grass-over-dirt side and
dirt below; `Moss` and `Snow` follow the same pattern with moss and snow bands.

## Constraints

- Deterministic and platform independent: the same seed must produce the same atlas everywhere.
- Cheap: the whole atlas is 102,400 pixels, so generation must cost a few milliseconds at client
  startup and never run per frame.
- Pixel art: 32×32 tiles, a handful of shades per material, no mipmaps, nearest sampling.
- Palette cohesion: one seed's tiles must read as one game, not as unrelated materials.
- Atlas layout is shared with the server. `tile_coord_start` is baked into saved chunk meshes, so
  changing a tile offset requires a `CHUNK_MESH_VERSION` bump
  ([`CHUNK_MESH_VERSION`](crates/archive/src/asset.rs#L10)).
- No image input and no image-generating models. The repository forbids AI-generated media
  ([AGENTS.md](AGENTS.md)); runtime algorithms with hand-authored parameters satisfy the policy and
  keep the texture data out of the repository.
- Multiplayer clients should agree on the look, so the seed is the world seed sent by the server,
  not a per-client visual seed.
- Art direction must be tunable without rewriting the generators, because the look is a team
  decision.

## Approach

Everything is synthesized from four pieces:

1. **Periodic fields**: value noise and Worley cells that wrap at the tile border, so greedy-meshed
   quads repeat without seams.
2. **Material motifs**: per-material feature layers — tufts for grass, pebbles and specks for dirt,
   boulders, broken cracks and strata for rock, ripples and grain for sand, hollows and sparkles for
   snow, cushions and lichen for moss, an emissive lattice for lamp.
3. **Layer stacks and procedural masks** for complex blocks: a side tile can be "dirt with a grass
   band on top" by compositing two simple materials through a band mask, with no authored alpha
   image.
4. **An authored palette per material in Oklch**, jittered by the world seed and quantized by an art
   preset.

The synthesis style decides how much structure runs: `noise` keeps only the minimum each material
needs to read, while `structural` adds the full motif set and is the style this proposal ships.
Both styles are fully procedural.

Rejected for now:

- **Example-based synthesis** (palette transfer, texture bombing, quilting, graph-cut): it needs an
  authored base tile to sample from, and there is no atlas to use as a source. It becomes a
  legitimate extension only if an artist later provides base tiles on purpose.
- **Neural methods** (neural cellular automata, GANs, diffusion): not deterministic across devices,
  far above the cost budget, and the output would be AI-generated media.
- **Offline procedural bakes** (Substance, Material Maker): no per-seed runtime variation.
- **Wang tiles and texture arrays**: worthwhile later for macro variation, but they need extra
  per-face data or a shader change, so they belong after the generation pipeline exists.

## Prototype

The generator lives in [`projekto_textures`](crates/textures/Cargo.toml) and is wired into the
client through [`TexturesPlugin`](crates/client/src/textures.rs#L40). It produces the exact 320×320
RGBA atlas the client expects, with the tile offsets declared in `kind.ron`. The example binary is
kept as an authoring tool for previewing styles, presets and seeds offline.

```
cargo run -p projekto_textures --example gen_atlas -- \
    --style structural --art warm --seed 42 \
    --out target/atlas.png --dump-tiles target/tiles
```

`--style` accepts `noise` and `structural`, and `--art` selects one of the art directions described
below. There is no `--base` or texture input of any kind: the only inputs are the style, the art
preset and the world seed. The library itself has no dependencies beyond `std`; the `image` crate is
only used by the example to save PNGs.

- [`generate_atlas`](crates/textures/src/lib.rs#L83) builds one tile per material and blits it into
  the atlas at the material's offset.
- [`PeriodicNoise`](crates/textures/src/noise.rs#L15) and
  [`PeriodicWorley`](crates/textures/src/noise.rs#L94) wrap at the tile border, which is what keeps
  repeated tiles seamless.
- [`IndexTile`](crates/textures/src/index_tile.rs#L17) keeps content as palette indices, and
  [`Ramp`](crates/textures/src/color.rs#L136) maps them to sRGB bytes. Colors are authored in
  Oklch: shadows shift cool, highlights shift warm, and chroma drops at both extremes.
- [`MATERIALS`](crates/textures/src/materials.rs#L201) holds the authored material recipes: atlas
  offset, base Oklch color, palette spread, seed tag, biome climate and, for complex blocks, a layer
  stack. [`LayerMask`](crates/textures/src/layers.rs#L14) evaluates the procedural band and patch
  masks.

The material set has 12 entries: the eight original materials plus `snow_side`, `moss_side`,
`mossy_rock` and `snowy_grass_side`, which occupy free atlas cells until `kind.ron` binds them.

Generation cost measured on a debug build is 4–8 ms for all twelve tiles, so it fits comfortably in
`Startup`. Output is `Rgba8UnormSrgb`. The generated image should carry an explicit
`ImageSampler::nearest()` instead of relying on `ImagePlugin::default_nearest()`, so the look does
not change if the plugin default is ever changed.

Two integration details surfaced while building it:

- The shader clamps tile-local UVs to the inner 88% of the tile
  ([`voxel.wesl`](assets/shaders/voxel.wesl#L59)). The outer ~2 px are stretched, so motifs live in
  rows and columns 2..30. Procedural tiles wrap at the border and do not need extra padding, but
  the shader inset must not be removed without adding a gutter or switching to a texture array.
- Tiles are periodic at 32 pixels, which is what a tile repeated across a greedy quad needs. Any
  future per-voxel variation has to come from extra atlas variants or a shader hash; it cannot be
  baked into different pixels of one tile.

## Art directions

The synthesis style decides how tiles are built; the art preset decides how they look. A preset
controls the number of shades a tile resolves to, palette contrast and chroma, color temperature,
motif density and relief strength. The palette is still authored per material in Oklch and jittered
by the seed, so a world varies inside the chosen direction instead of redesigning it.

| Preset | Shades | Character |
| --- | ---: | --- |
| `soft` | 12 | Closest to the shipped painterly feel: soft ramps, light motifs |
| `pixel` | 4 | Crisp pixel art: hard quantization and stronger relief |
| `muted` | 7 | Desaturated and slightly cool, natural and calm |
| `warm` | 8 | Warm hue shift and brighter highlights, golden-hour terrain |
| `vivid` | 6 | High chroma and high contrast, arcade look |
| `dramatic` | 5 | Darker shadows, higher contrast and a cooler cast |

Presets live in [`ArtPreset`](crates/textures/src/art.rs#L30) and are pure data, so a new look is a
new parameter set, not new generator code. Every tile's palette is built by
[`Ramp::new`](crates/textures/src/color.rs#L158), which quantizes the six generator levels onto the
preset's shade count.

All six presets were rendered in-game with the same camera pose, world and seed, together with the
shipped atlas as a visual baseline. Screenshots are in
`debug_reports/texture-research/ingame/proc_art_*.png`, with comparison sheets at
`debug_reports/texture-research/proc_art_tiles.png` (tiles), `proc_ingame_art.png` (terrain),
`proc_ingame_crops.png` (close-up crops) and `complex_blocks.png` (top/side/bottom faces of the
composite surface blocks).

## Complex blocks

A block can have a different material per face. `kind.ron` already models this with
`KindSidesDesc::Unique`, and the mesher bakes one tile origin per vertex, so face-specific
materials cost nothing at render time. A grass block is:

| Face | Material |
| --- | --- |
| up | `grass_top` |
| sides | `grass_side` |
| down | `dirt` |

The bottom face is not a special case; it is just another binding. A snow-covered grass block uses
`snow` on top, `snowy_grass_side` on the sides and `dirt` below.

The mixed side is where the generator needs a strategy. Instead of authoring an alpha mask or a
separate sheet, a material is a **layer stack**: the first layer is the full base material and each
following layer reuses another simple material's pattern and palette through a procedural mask.
`grass_side` is literally:

```ron
material "grass_side": layers: [
    (source: "dirt",      mask: Full),
    (source: "grass_top", mask: (top_band: (height: 8.0, variation: 4.5, drips: 20, lip: true))),
],
```

Because the overlay references `grass_top`, the band uses the same grass pattern and the same
seeded palette as the top face, so the block reads as one material. The mask is evaluated from the
seed: noisy band height, optional one-pixel overhangs, a dark lip at the seam and a bright top row.
No alpha image or extra tile data is stored.

The same layer system covers every "X over Y" surface:

| Block | Layers |
| --- | --- |
| grass block side | dirt + grass top band |
| tundra surface side | dirt + snow top band |
| forest surface side | dirt + moss top band |
| overgrown rock | rock + moss patches |
| snowy grass side | dirt + grass band + snow band |

Masks are procedural and periodic:

- `Full` — the whole tile; used by the base layer.
- `TopBand` — top band with noisy height, drips, lip and top highlight; used by side materials.
- `Patches` — noise-thresholded organic patches; used by moss on rock.

Adding a new block is a data change: bind its faces in `kind.ron` and add a material with a layer
stack. The generator does not gain a branch per block.

Two details keep composites coherent:

- The base layer's palette is the material palette; overlays take their palette from the referenced
  simple material, so the grass band matches the grass top exactly for a given world seed.
- A layer source must be a simple material, so a stack cannot recurse. A three-layer stack is
  allowed (dirt below grass below snow); cycles are rejected by a test.

Cases that do not fit this model, and the strategy for each:

- **All faces unique, not mixed** (logs, carved blocks): bind six materials with `Unique`. Blocks
  with an axis orientation need one Kind variant per axis, because voxels carry no orientation;
  that is a Kind/data-model change, not a texture problem.
- **Transparent or cutout blocks** (leaves, plants, glass): the chunk material is opaque and the
  mesher emits full cubes. This needs a second chunk material with alpha test/blend and, for
  non-cube shapes, a meshing pass for cross quads. Generated tiles can already carry alpha; the
  generator is not the blocker.
- **Connected textures** (frames or edges reacting to neighbours): the mesher does not know the
  neighbourhood at tile-selection time, so this needs per-face variant data or a shader technique
  and belongs to the later tile-variant phase.
- **Animated textures** (water, lava): shader and time based, not per-seed tile generation.

Composite examples rendered with the `soft` preset:

| Block | Top | Side | Bottom |
| --- | --- | --- | --- |
| grass | grass | dirt + grass band | dirt |
| tundra surface | snow | dirt + snow band | dirt |
| forest surface | moss | dirt + moss band | dirt |
| overgrown rock | rock + moss patches | same | rock |
| snowy grass | snow | dirt + grass band + snow band | dirt |

## Results

Artifacts from the prototype run are in `debug_reports/texture-research/` (gitignored): the per-art
atlases and per-tile dumps for seed 42, seed variation dumps, comparison sheets, biome material
mockups and the in-game screenshots listed above.

Observed in-game for the fully procedural `structural` style:

- The terrain reads as a coherent block game without any source artwork. Grass shows a stippled
  body with light tuft marks, dirt has pebbles and dark specks, rock shows domed boulders, broken
  cracks and faint strata, sand has fine warped ripples, snow has soft hollows and sparkles, and
  moss has dark cushions with lichen dots.
- Against the shipped atlas the procedural tiles are less noisy because they quantize to a small
  palette. The `soft` preset is the closest to the current painterly grain; `pixel` is the
  strongest stylistic break.
- The art presets separate cleanly at terrain scale: `muted` desaturates the greens and cools the
  rock, `warm` shifts dirt and sand toward ochre, `vivid` saturates, `dramatic` darkens shadows and
  raises contrast.
- Seed variation is limited to palette jitter (a few degrees of hue, a few percent of lightness),
  motif placement and noise phases. That matches the requirement that every seed produces a
  slightly different tile set while the palette family keeps the tiles cohesive.
- Composite sides read as one block: the grass band on a side uses the same grass pattern and
  palette as the top face, the seam gets a dark lip and occasional overhangs, and the dirt below is
  the same dirt material as the bottom face. Tundra and forest surfaces use the same mechanism with
  snow and moss bands, and a three-layer stack shows a snow cap over a grass band over dirt.
- All composite tiles were validated in the same in-game scene with the `soft` preset; the side
  faces of grass blocks are generated, not sampled from the shipped PNG.

## Recommendation

Adopt the fully procedural `structural` style with one of the art presets:

1. No bitmap, spritesheet or base texture is shipped or read for terrain rendering. The fixed PNG
   is retired once this lands.
2. The client generates the atlas at startup from the world seed.
3. Material recipes (palette, spread, motif set, biome climate) are authored data, and the art
   preset sets the shade count, contrast, chroma, temperature and relief.
4. Sand, Snow and Moss become real materials synthesized procedurally, replacing the tinted
   placeholders; the tint values go back to white.
5. The chosen preset becomes the default in the recipes; the remaining presets stay as authoring
   options for future biomes or texture packs.

## Implementation

### 1. Material descriptors

Add an `assets/textures.ron` that lists one recipe per material. `kind.ron` keeps the atlas offset
and tint and references the material by name, so the mesher ([`KindSideTexture`](crates/core/src/voxel/kind.rs#L13))
is unchanged:

```ron
(
    materials: [
        (
            name: "grass_top",
            base: (lightness: 0.355, chroma: 0.125, hue: 143.0),
            spread: 0.10,
            motifs: [Tufts, Speckles],
        ),
        (
            name: "grass_side",
            layers: [
                (source: "dirt", mask: Full),
                (source: "grass_top", mask: (top_band: (height: 8.0, variation: 4.5, drips: 20, lip: true))),
            ],
        ),
        (
            name: "sand",
            base: (lightness: 0.800, chroma: 0.090, hue: 88.0),
            spread: 0.09,
            motifs: [Ripples, Grain, Sparkles],
        ),
        // remaining materials follow the same shape
    ],
)
```

```ron
up: (
    color: (1.0, 1.0, 1.0, 1.0),
    offset: (1, 0),
    material: "grass_top",
),
// sides: material "grass_side", down: material "dirt"
```

Validation belongs next to [`check_descs`](crates/core/src/voxel/kind.rs#L137): every referenced
material must exist, offsets must stay inside the atlas, and duplicate material names are rejected.

### 2. Client atlas generation

[`TexturesPlugin`](crates/client/src/textures.rs#L40) replaces the old PNG loading.
[`setup_textures`](crates/client/src/textures.rs#L55) initializes the kind descriptors, validates
the atlas layout and synthesizes the atlas with
[`generate_atlas`](crates/textures/src/lib.rs#L83); the image is `Rgba8UnormSrgb` with an explicit
nearest sampler. [`setup_material`](crates/client/src/textures.rs#L64) builds `ChunkMaterial` from
the generated handle, and the debug grid material shares it through
[`TerrainAtlas`](crates/client/src/textures.rs#L38). Generation is fast enough to run
synchronously; if it ever grows, it can move to the async task pool like the mesher.

### 3. Seed distribution

The seed is the world seed, not a per-client visual seed. The
[`ServerInfo`](crates/messages/src/lib.rs#L28) message carries it, the server sends it from
[`new_client_connected`](crates/server/src/net.rs#L162) using
[`GenesisServer::seed`](crates/server/src/genesis/server.rs#L60), and the client stores it in
[`WorldSeed`](crates/client/src/textures.rs#L28).
[`regenerate_atlas`](crates/client/src/textures.rs#L83) rewrites the atlas when it arrives. Until
then the client uses [`FALLBACK_WORLD_SEED`](crates/client/src/textures.rs#L24), so all clients
render the same world with the same textures.

### 4. `kind.ron` changes

The placeholder kinds now bind their own generated tiles and neutral tints:

| Kind | Offset | Tint |
| --- | --- | --- |
| Sand | (3, 0) | (1.0, 1.0, 1.0, 1.0) |
| Snow | (4, 0) | (1.0, 1.0, 1.0, 1.0) |
| Moss | (5, 0) | (1.0, 1.0, 1.0, 1.0) |

Dirt, Grass, Rock and Lamp keep their current offsets, so only the Sand, Moss and Snow meshes
change. Snow and Moss become face-specific for the side treatment:

| Kind | Binding |
| --- | --- |
| Snow | up `snow`, sides `snow_side`, down `dirt` |
| Moss | up `moss`, sides `moss_side`, down `dirt` |
| Sand | all faces `sand` (desert is sand top and below) |

`Grass` already binds `grass_top` up, `grass_side` on the sides and `dirt` down.

### 5. Archive invalidation

Changing an offset in `kind.ron` leaves saved chunk meshes pointing at the old tile, because
`tile_coord_start` is part of the vertex data.
[`CHUNK_MESH_VERSION`](crates/archive/src/asset.rs#L10) was bumped to 10 so an old archive is
remeshed from stored kinds instead of rendered with stale coordinates.

### 6. Shader sampling review

The 6% inset ([`voxel.wesl`](assets/shaders/voxel.wesl#L59)) exists to hide atlas bleed. With
nearest sampling and no mipmaps on a generated atlas, a half-texel inset (`0.5 / 32`) would recover
the outer pixels of every tile and remove the stretched border band. That is a separate visual
change and should be tested on its own.

### 7. Later phases

- **Tile variants**: 2–4 variants per Kind, chosen by a deterministic hash of the voxel position in
  the mesher or in the fragment shader. The atlas has 100 slots and the mesher already passes
  `tile_coord_start` per vertex, so variants are cheap, but per-voxel variation must not break
  greedy merging.
- **Per-biome tints for shared kinds**: biomes currently get distinct Kinds. If two biomes must
  share a Kind with different tints, the biome index has to reach the mesher (per column) or the
  shader (per vertex attribute), which is a larger chunk-data change.
- **Texture arrays**: if the atlas outgrows 100 tiles, a `texture_2d_array` with `REPEAT` removes
  the atlas and the UV clamp entirely.
- **Biome transitions**: once materials are per-biome, blending two materials across the transition
  band becomes a natural follow-up.
- **Optional artist source tiles**: the generator can gain an optional base-tile input for a
  material if an artist wants hand-authored structure later. That is an explicit extension, not a
  dependency of this design.

## Test plan

- `cargo test -p projekto_textures` covers atlas layout and offsets, determinism per style, seed
  variation, art-preset differences, style differences, layer-source validity, absence of flat
  tiles, and that a composite side resolves the grass band through the grass palette and the base
  through the dirt palette.
- `cargo run -p projekto_textures --example gen_atlas -- --style structural --art muted --seed 42`
  must produce a 320×320 atlas whose used offsets match `kind.ron`.
- The client test `shipped_kind_descriptors_match_the_generator_layout` asserts that every offset
  in `kind.ron` is produced by the generator, and `generated_atlas_has_the_configured_size` covers
  the image format and sampler.
- In-game validation boots the server and the windowless AI client and reads the client log lines
  `Generated terrain atlas for world seed <seed>` (fallback seed at startup, then the server seed),
  then captures with `projekto/ai_snapshot`. No terrain image file is involved.
- The client asserts the `kind.ron` atlas size and tile size against the generator at startup, so a
  config error fails fast instead of sampling outside the atlas.

## Open questions

- The `soft` preset is the chosen default. The remaining five stay available as authoring
  options, but nothing in the game switches between them yet.
- How much should the procedural motifs lean into stylization versus realism? Grass tufts and rock
  strata are the most visible materials; both have a "more motifs" and a "cleaner" direction to
  tune.
- Which composite blocks should ship first? The prototype covers the three biome surfaces (grass,
  snow, moss) plus an overgrown rock and a three-layer snowy grass; more blocks are data once their
  Kinds exist.
- Should palette tuning get a small preview tool (a grid of ramps and tiles for a range of seeds),
  or is the `gen_atlas` dump enough?
- Do we want an in-repo gallery of generated atlases per seed for review, or are the debug artifacts
  enough?

## References

- Oklab color space and gamut handling: <https://bottosson.github.io/posts/oklab/>
- Pixel-art palette practice: <https://saint11.org/blog/pixel-art-tutorials/>
- Tiling simplex noise: <https://jcgt.org/published/0011/01/02/>
- Constant-width Voronoi cracks: <https://iquilezles.org/articles/voronoilines/>
- Domain warping: <https://iquilezles.org/articles/warp/>
- Texture atlases, wrapping and mipmaps: <https://0fps.net/2013/07/09/texture-atlases-wrapping-and-mip-mapping/>
- Bevy texture-array example: <https://bevy.org/examples-webgpu/shaders/array-texture/>
- Bevy AI policy: <https://bevy.org/learn/contribute/policies/ai/>
