#!/usr/bin/env bash
set -euo pipefail

KEY_DIR="${HOME}/.config/recode/updater"
mkdir -p "${KEY_DIR}"

echo "[recode] generating updater signing keys in ${KEY_DIR}"
openssl genrsa -out "${KEY_DIR}/private.pem" 2048
openssl rsa -in "${KEY_DIR}/private.pem" -pubout -out "${KEY_DIR}/public.pem"

chmod 600 "${KEY_DIR}/private.pem"
chmod 644 "${KEY_DIR}/public.pem"

echo "[recode] updater keys generated"
echo "[recode] private: ${KEY_DIR}/private.pem"
echo "[recode] public:  ${KEY_DIR}/public.pem"
