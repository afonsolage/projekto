win_target := "x86_64-pc-windows-msvc"
win_dir := "target/" + win_target + "/debug"

alias s := server
alias c := client-win
alias cia := ai-client

# List available recipes.
default:
    @just --list

# Run the same validations as CI locally (requires `cargo install cargo-udeps`).
ci:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo udeps

# Run the manual terrain streaming, meshing and LOD walk benchmark.
terrain-walk *args:
    cargo test -p projekto_server --test terrain_walk -- --ignored --nocapture {{args}}

# Run the 10-minute, double-speed terrain streaming stress test.
terrain-stress *args:
    PROJEKTO_TERRAIN_TEST_SPEED=20 PROJEKTO_TERRAIN_TEST_WALK_CHUNKS=750 cargo test -p projekto_server --test terrain_walk -- --ignored --nocapture {{args}}

# Render a top-down height map, surface map and vertical slice of the generated terrain.
terrain-map *args:
    cargo run -q -p projekto_server --example terrain_map -- {{args}}

# Check the release configuration without debug tooling.
check-release:
    ASSETS_PATH="$(pwd)/assets" cargo check --release -p projekto --bin client
    cargo check --release -p projekto --bin server

# Check the debug-tools feature configuration.
check-debug:
    cargo check -p projekto --bin client --features debug-tools
    cargo check -p projekto --bin ai-client --features debug-tools
    cargo check -p projekto --bin server --features debug-tools
    cargo clippy -p projekto --bins --features debug-tools -- -D warnings

# Run the server on Linux.
server *args:
    cargo run -p projekto --bin server --features debug-tools {{args}}

# Inspect region archives.
archive *args:
    cargo run -q -p projekto_archive_cli -- {{args}}

# Cross-compile the windowless AI client and run it natively on Windows.
ai-client *args:
    #!/usr/bin/env bash
    set -euo pipefail
    export ASSETS_PATH="$(wslpath -w assets)"
    cargo xwin build --target {{win_target}} -p projekto --bin ai-client --features debug-tools {{args}}
    cp -u "$(rustc --print sysroot)"/lib/rustlib/{{win_target}}/lib/std-*.dll {{win_dir}}/
    export BEVY_ASSET_ROOT="$(wslpath -w .)"
    export PROJEKTO_VIEW_RADIUS="${PROJEKTO_VIEW_RADIUS:-16}"
    export RUST_LOG="${RUST_LOG:-error,projekto=debug,projekto_client=debug}"
    export WSLENV="RUST_LOG:WGPU_BACKEND:BEVY_ASSET_ROOT:PROJEKTO_VIEW_RADIUS"
    exec {{win_dir}}/ai-client.exe

# Cross-compile the client in WSL and run it natively on Windows.
client-win *args:
    #!/usr/bin/env bash
    set -euo pipefail
    # `env!("ASSETS_PATH")` is baked in at compile time, so it must be a Windows path.
    export ASSETS_PATH="$(wslpath -w assets)"
    cargo xwin build --target {{win_target}} -p projekto --bin client --features debug-tools {{args}}
    # `dynamic_linking` needs the toolchain's std DLL next to the executable.
    cp -u "$(rustc --print sysroot)"/lib/rustlib/{{win_target}}/lib/std-*.dll {{win_dir}}/
    export BEVY_ASSET_ROOT="$(wslpath -w .)"
    export RUST_LOG="${RUST_LOG:-error,projekto=debug,projekto_client=trace}"
    export WSLENV="RUST_LOG:WGPU_BACKEND:BEVY_ASSET_ROOT:PROJEKTO_VIEW_RADIUS"
    exec {{win_dir}}/client.exe
