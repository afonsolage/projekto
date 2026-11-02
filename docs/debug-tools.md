# Debug tools

These tools are gated behind the `debug-tools` feature. `remote-debug` enables
`debug-tools` plus the Bevy Remote Protocol, and `ai` enables `remote-debug` for the
windowless client. The `just` recipes enable the relevant features; a normal release build
does not compile `projekto_debug` or the debug data collection.

## Enabling and disabling

The client and server expose their debug functionality as Bevy plugin groups:

- `projekto_client::debug::DebugToolsPlugins`
- `projekto_server::debug::DebugToolsPlugins`

Add a group explicitly when the feature is enabled, or disable individual plugins with
`PluginGroupBuilder::disable` (for example, keeping data/BRP while disabling the UI).

## Client hotkeys

- **F3** — toggle the client/server RAM overlay.
- **F4** — toggle the chunk grid.
- **F5** — toggle chunk coordinate labels.
- **F7** — capture a debug report.
- **F8** — place a pin on the hovered chunk.
- **F9** — clear all pins.

The bottom-left overlay shows the hovered chunk, its `ChunkDebug` summary, the camera chunk,
the crosshair chunk and the pin count.

The grid and labels are world-space. F4 enables chunk boundary lines in the debug chunk
material: each fragment compares its world position with the 16-block chunk lattice, so the
lines follow the actual terrain instead of a synthetic plane or per-chunk height. The
production chunk material does not contain the debug shader branch. F5 draws `(x, z)` labels
at chunk centres. The crosshair chunk is the chunk where the camera centre ray crosses the
configured grid height, because a single screen ray has no surface depth.

## Debug reports

**F7** writes `debug_reports/<timestamp>/` relative to `BEVY_ASSET_ROOT` (the project root when
launched through `just c`):

- `screenshot.png` — the frame at capture time.
- `report.json` — camera pose, hovered and crosshair chunks, nearby `ChunkDebug` values, pins,
  client and server memory.
- `README.txt` — the commands below for inspecting the chunk in the archive.

## Chunk pins

**F8** creates a `DebugPin` on the hovered chunk at the picked world position. Pins are visible
in the world and are included in reports. They are also reflected for BRP (`projekto/pins`).

## Archive CLI

```text
just archive info   archive/region [--json]
just archive list   archive/region [--chunks] [--json]
just archive holes  archive/region [--center x,z] [--radius N] [--limit N] [--json]
just archive verify archive/region [--json]
just archive dump   archive/region <x> <z> [--map] [--json]
```

`holes` reports chunks whose persisted mesh is empty even though their `Kind` has a surface,
which is the signature of a rendering hole.

## Remote API (BRP)

The server and client expose the Bevy Remote Protocol over HTTP when built with the
`remote-debug` feature. `just s` and `just c` enable it automatically; it implies
`debug-tools`.

- Server main app: `127.0.0.1:15702`
- Client main app: `127.0.0.1:15712` (visible client) / `127.0.0.1:15722` (AI client)
- Client render subapp: `127.0.0.1:15703`

The client uses a different main port because WSL forwards Windows `localhost:15702` to the
server running in WSL.

From WSL the server is directly reachable:

```bash
curl -s http://127.0.0.1:15702 -H 'content-type: application/json' -d '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "world.query",
  "params": {
    "data": { "components": ["projekto_debug::ChunkDebug"], "option": "All", "has": [] },
    "strict": false,
    "filter": { "with": [], "without": [] }
  }
}'
```

The Windows client listens on Windows localhost port 15712. Execute `curl.exe` through WSL to
reach it:

```bash
curl.exe -s http://127.0.0.1:15712 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"world.list_components"}'
```

Built-in BRP methods include `world.query`, `world.get_components`, `world.get_resources`,
`world.mutate_components`, `world.observe+watch` and `schedule.graph`.

### Custom server methods

- `projekto/chunk_info` — `{ "x": i32, "z": i32 }`
- `projekto/chunks` — `{ "x": i32, "z": i32, "radius": i32 }`
- `projekto/landscape`
- `projekto/memory`
- `projekto/remesh` — invalidates and queues a chunk and its neighbours
- `projekto/teleport` — `{ "x": i32, "z": i32, "radius": u8 }`
- `projekto/set_voxel` — `{ "chunk_x", "chunk_z", "x", "y", "z", "kind" }`

### Custom client methods

- `projekto/chunk_info` — `{ "x": i32, "z": i32 }`
- `projekto/pins`
- `projekto/pin` — `{ "x": i32, "z": i32, "position": [f32; 3]? }`
- `projekto/unpin` — `{ "x": i32, "z": i32 }`
- `projekto/memory`
- `projekto/player_landscape`
- `projekto/chunk_at_screen` — `{ "x": f32, "y": f32, "plane_y": f32? }` projects a screen point
  onto the grid plane and returns its chunk
- `projekto/set_camera` — `{ "position": [f32; 3], "target": [f32; 3] }` moves the active fly-by
  camera (useful for AI-driven snapshots)
- `projekto/report` — triggers a debug report and screenshot

BRP can mutate the world. Never enable the feature in a release build or expose it outside
localhost.

## AI client (`just cia`)

`just cia` builds the same client with the `ai` feature (which implies `remote-debug` and
`debug-tools`) and runs it on Windows without creating a window:

- windowless, one update per second;
- renders the world to a 960x540 offscreen image;
- grid and world-space chunk labels enabled by default;
- writes `debug_reports/ai/latest.png` every 5 seconds;
- exposes BRP on `127.0.0.1:15722` and the render subapp on `15703`;
- does not accept keyboard/mouse input.

Request an on-demand snapshot:

```bash
curl.exe -s http://127.0.0.1:15722 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"projekto/ai_snapshot"}'
```

The image is written to `debug_reports/ai/latest.png` in the project root (via
`BEVY_ASSET_ROOT`) after the next frame. The AI client should be run instead of, not alongside,
the visible client: both use the render BRP port `15703`.

Use the server custom methods (`projekto/teleport`, `projekto/chunk_info`, etc.) and the client BRP
methods (`projekto/chunk_info`, `projekto/pins`, `projekto/memory`) to drive and inspect the
windowless session.
