#!/usr/bin/env bash
set -euo pipefail

DESKTOP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/128x128/apps"

mkdir -p "$DESKTOP_DIR"
mkdir -p "$ICON_DIR"

cp packaging/omarchy/recode.desktop "$DESKTOP_DIR/"
cp src-tauri/icons/icon.png "$ICON_DIR/recode.png" 2>/dev/null || true

echo "[recode] desktop entry installed"
echo "[recode] run from app menu or: recode"
