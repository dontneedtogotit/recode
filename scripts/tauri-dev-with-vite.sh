#!/usr/bin/env bash
set -euo pipefail

export GDK_BACKEND=x11
export RECODE_DEV=1

FRONTEND_PORT=9666 # Must match src-tauri/tauri.conf.json build.devUrl.
FRONTEND_URL="http://localhost:${FRONTEND_PORT}"
DEV_PID=''
VITE_LOG=''

cleanup() {
  if [[ -n "${DEV_PID:-}" ]]; then
    kill "$DEV_PID" 2>/dev/null || true
    wait "$DEV_PID" 2>/dev/null || true
  fi
  if [[ -n "${VITE_LOG:-}" ]]; then
    rm -f -- "$VITE_LOG"
  fi
}
trap cleanup EXIT

if lsof -nPiTCP:"${FRONTEND_PORT}" -sTCP:LISTEN 2>/dev/null | grep -q .; then
  echo "[recode] port ${FRONTEND_PORT} is in use; stop that server before starting recode" >&2
  exit 1
fi

echo "[recode] building frontend..."
npm run build

echo "[recode] starting frontend dev server on port ${FRONTEND_PORT}..."
VITE_LOG="$(mktemp -t recode-vite.XXXXXX.log)"
node node_modules/vite/bin/vite.js --host 127.0.0.1 --port "$FRONTEND_PORT" --strictPort > "$VITE_LOG" 2>&1 &
DEV_PID=$!

echo "[recode] waiting for frontend dev server at ${FRONTEND_URL}..."
for i in $(seq 1 60); do
  if ! kill -0 "$DEV_PID" 2>/dev/null; then
    echo "[recode] Vite exited before it was ready" >&2
    break
  fi
  if curl -sf "${FRONTEND_URL}" >/dev/null 2>&1; then
    echo "[recode] frontend ready"
    break
  fi
  sleep 1
done

if ! curl -sf "${FRONTEND_URL}" >/dev/null 2>&1; then
  echo "[recode] frontend did not become ready in time"
  echo "[recode] vite log:"
  tail -n 40 "$VITE_LOG" || true
  exit 1
fi

echo "[recode] launching tauri app..."
cargo run --manifest-path src-tauri/Cargo.toml
