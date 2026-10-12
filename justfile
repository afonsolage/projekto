win_target := "x86_64-pc-windows-msvc"
win_dir := "target/" + win_target + "/debug"

alias s := server
alias c := client-win

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
    cargo run -p projekto_server {{args}}

# Cross-compile the client in WSL and run it natively on Windows.
client-win *args:
    #!/usr/bin/env bash
    set -euo pipefail
    # `env!("ASSETS_PATH")` is baked in at compile time, so it must be a Windows path.
    export ASSETS_PATH="$(wslpath -w assets)"
    cargo xwin build --target {{win_target}} -p projekto_client {{args}}
    # `dynamic_linking` needs the toolchain's std DLL next to the executable.
    cp -u "$(rustc --print sysroot)"/lib/rustlib/{{win_target}}/lib/std-*.dll {{win_dir}}/
    export BEVY_ASSET_ROOT="$(wslpath -w .)"
    export RUST_LOG="${RUST_LOG:-error,projekto=debug,projekto_client=trace}"
    export WSLENV="RUST_LOG:WGPU_BACKEND:BEVY_ASSET_ROOT"
    exec {{win_dir}}/client.exe
