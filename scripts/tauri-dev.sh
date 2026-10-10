#!/usr/bin/env bash
set -euo pipefail

export GDK_BACKEND=x11
export RECODE_DEV=1

DIST_DIR="./dist"
FRONTEND_PORT="${RECODE_DEV_PORT:-9666}"
FRONTEND_URL="http://localhost:${FRONTEND_PORT}"

cleanup() {
  echo "[recode] cleaning up..."
  pkill -f "recode" 2>/dev/null || true
}
trap cleanup EXIT

if [ ! -d "$DIST_DIR" ]; then
  echo "[recode] missing frontend build; running npm run build first..."
  npm run build
fi

echo "[recode] serving built frontend assets on port ${FRONTEND_PORT}..."
python3 -m http.server "${FRONTEND_PORT}" --directory "${DIST_DIR}" > /tmp/recode-vite.log 2>&1 &
SERVER_PID=$!
sleep 1

for i in $(seq 1 60); do
  if curl -sf "${FRONTEND_URL}" >/dev/null 2>&1; then
    echo "[recode] frontend ready"
    break
  fi
  sleep 1
done

if ! curl -sf "${FRONTEND_URL}" >/dev/null 2>&1; then
  echo "[recode] frontend did not become ready in time"
  echo "[recode] http-server log:"
  tail -n 40 /tmp/recode-vite.log || true
  exit 1
fi

echo "[recode] launching tauri app..."
echo "[recode] if no window appears, check /tmp/recode-tauri.log"

cargo run --manifest-path src-tauri/Cargo.toml 2>&1 | tee /tmp/recode-tauri.log
