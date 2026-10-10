#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo-audit >/dev/null 2>&1; then
  echo "[recode] cargo-audit not installed, installing..."
  cargo install cargo-audit --locked
fi

cargo audit --manifest-path src-tauri/Cargo.toml
