#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

BIN_NAME="recode"
BIN_CANDIDATES=(
  "target/release/recode"
  "src-tauri/target/release/recode"
)

BIN_SRC=""
for candidate in "${BIN_CANDIDATES[@]}"; do
  if [ -f "$candidate" ]; then
    BIN_SRC="$candidate"
    break
  fi
done

if [ -z "$BIN_SRC" ]; then
  echo "[recode] release binary not found, building..."
  cargo build --release --bin recode --manifest-path src-tauri/Cargo.toml
  for candidate in "${BIN_CANDIDATES[@]}"; do
    if [ -f "$candidate" ]; then
      BIN_SRC="$candidate"
      break
    fi
  done
fi

if [ -z "$BIN_SRC" ]; then
  echo "[recode] error: release binary still not found after build"
  exit 1
fi

if [ "$(id -u)" -eq 0 ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="${HOME}/.local/bin"
fi
mkdir -p "$INSTALL_DIR"

echo "[recode] installing binary to ${INSTALL_DIR}..."
if [ "${RECODE_INSTALL_SYMLINK:-0}" = "1" ]; then
  ln -sf "$(pwd)/${BIN_SRC}" "${INSTALL_DIR}/${BIN_NAME}"
  echo "[recode] installed as symlink for immediate updates"
else
  cp "$BIN_SRC" "${INSTALL_DIR}/${BIN_NAME}"
  chmod +x "${INSTALL_DIR}/${BIN_NAME}"
  echo "[recode] installed binary"
fi

echo "[recode] install complete"
echo "[recode] run with: ${BIN_NAME}"
