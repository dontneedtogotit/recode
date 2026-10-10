#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: $0 <artifact>"
  exit 1
fi

ARTIFACT="$1"
KEY_DIR="${HOME}/.config/recode/updater"
PRIVATE_KEY="${KEY_DIR}/private.pem"
SIGNATURE_FILE="${ARTIFACT}.sig"

if [ ! -f "${PRIVATE_KEY}" ]; then
  echo "[recode] missing updater private key at ${PRIVATE_KEY}"
  echo "[recode] run scripts/setup-updater-keys.sh first"
  exit 1
fi

echo "[recode] signing ${ARTIFACT}..."
openssl dgst -sha256 -sign "${PRIVATE_KEY}" -out "${SIGNATURE_FILE}" "${ARTIFACT}"
echo "[recode] signature written to ${SIGNATURE_FILE}"
