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

# Run the server on Linux.
server *args:
    cargo run -p projekto_server --features remote-debug {{args}}

# Inspect region archives.
archive *args:
    cargo run -q -p projekto_archive_cli -- {{args}}

# Cross-compile the windowless AI client and run it natively on Windows.
ai-client *args:
    #!/usr/bin/env bash
    set -euo pipefail
    export ASSETS_PATH="$(wslpath -w assets)"
    cargo xwin build --target {{win_target}} -p projekto_client --features ai {{args}}
    cp -u "$(rustc --print sysroot)"/lib/rustlib/{{win_target}}/lib/std-*.dll {{win_dir}}/
    export BEVY_ASSET_ROOT="$(wslpath -w .)"
    export PROJEKTO_VIEW_RADIUS="${PROJEKTO_VIEW_RADIUS:-16}"
    export RUST_LOG="${RUST_LOG:-error,projekto=debug,projekto_client=debug}"
    export WSLENV="RUST_LOG:WGPU_BACKEND:BEVY_ASSET_ROOT:PROJEKTO_VIEW_RADIUS"
    exec {{win_dir}}/client.exe

# Cross-compile the client in WSL and run it natively on Windows.
client-win *args:
    #!/usr/bin/env bash
    set -euo pipefail
    # `env!("ASSETS_PATH")` is baked in at compile time, so it must be a Windows path.
    export ASSETS_PATH="$(wslpath -w assets)"
    cargo xwin build --target {{win_target}} -p projekto_client --features remote-debug {{args}}
    # `dynamic_linking` needs the toolchain's std DLL next to the executable.
    cp -u "$(rustc --print sysroot)"/lib/rustlib/{{win_target}}/lib/std-*.dll {{win_dir}}/
    export BEVY_ASSET_ROOT="$(wslpath -w .)"
    export RUST_LOG="${RUST_LOG:-error,projekto=debug,projekto_client=trace}"
    export WSLENV="RUST_LOG:WGPU_BACKEND:BEVY_ASSET_ROOT:PROJEKTO_VIEW_RADIUS"
    exec {{win_dir}}/client.exe
