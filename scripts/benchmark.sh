#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Roadmap targets:
# - startup <2s
# - IPC p95 <10ms
# - workspace creation <3s

echo "[recode] running Rust benchmarks..."
cargo test --test workspace_benchmark --manifest-path src-tauri/Cargo.toml
cargo test --lib --benches --manifest-path src-tauri/Cargo.toml || true

echo "[recode] running app startup benchmark..."
cargo test startup_benchmark --manifest-path src-tauri/Cargo.toml || true

echo "[recode] benchmark complete."