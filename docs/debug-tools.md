# Debug tools

These tools are only compiled for debug builds. The remote API is also gated behind the
`remote-debug` feature, which the `just` recipes enable.

## Client hotkeys

- **F3** — toggle the client/server RAM overlay.
- **F4** — toggle the chunk grid.
- **F5** — toggle chunk coordinate labels.
- **F7** — capture a debug report.
- **F8** — place a pin on the hovered chunk.
- **F9** — clear all pins.

The bottom-left overlay shows the hovered chunk, its `ChunkDebug` summary and the pin count.
Chunks without a mesh are drawn in red in the grid and labels.

## Debug reports

**F7** writes `debug_reports/<timestamp>/` relative to `BEVY_ASSET_ROOT` (the project root when
launched through `just c`):

- `screenshot.png` — the frame at capture time.
- `report.json` — camera pose, hovered chunk, nearby `ChunkDebug` values, pins, client and
  server memory.
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
`remote-debug` feature. `just s` and `just c` enable it automatically.

- Main app: `127.0.0.1:15702`
- Render subapp: `127.0.0.1:15703`

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

The Windows client listens on Windows localhost. Execute `curl.exe` through WSL to reach it:

```bash
curl.exe -s http://127.0.0.1:15702 -H 'content-type: application/json' \
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
- `projekto/report` — triggers a debug report and screenshot

BRP can mutate the world. Never enable the feature in a release build or expose it outside
localhost.
