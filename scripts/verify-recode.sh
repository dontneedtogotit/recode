#!/usr/bin/env bash
# Six independent gates; a green summary requires all six to pass.
set -u
cd "$(dirname "$0")/.." || exit 1
logs=$(mktemp -d "${TMPDIR:-/tmp}/recode-verify.XXXXXXXX") || exit 1
trap 'rm -rf "$logs"' EXIT
failed=0

check() {
  local name=$1
  shift
  printf '\n[%s] %s\n' "$name" "$*"
  if "$@" >"$logs/$name.log" 2>&1; then
    printf '[PASS] %s\n' "$name"
  else
    local code=$?
    printf '[FAIL] %s (exit %s)\n' "$name" "$code"
    tail -35 "$logs/$name.log"
    failed=1
  fi
}

check native-ipc-wiring python3 scripts/check-ipc-wiring.py
check ipc-checker-regressions python3 -m unittest discover -s scripts -p test_check_ipc_wiring.py
check frontend-types-and-bundle npm run build
check frontend-contracts npm run test
check rust-contracts cargo test --workspace
check native-binary cargo check -p recode --bin recode
if [ "$failed" -ne 0 ]; then
  printf '\nVerification failed; no green result is implied by filtered output.\n' >&2
  exit 1
fi
printf '\nAll six independent gates passed.\n'
