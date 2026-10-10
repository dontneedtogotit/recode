#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 3 ]; then
  echo "Usage: $0 <version> <artifact> <notes>"
  exit 1
fi

VERSION="$1"
ARTIFACT="$2"
NOTES="$3"
MANIFEST_DIR="$(dirname "$ARTIFACT")"
MANIFEST="${MANIFEST_DIR}/recode-${VERSION}-linux-x86_64.AppImage.tar.gz.sha256"

if [ ! -f "${ARTIFACT}" ]; then
  echo "[recode] missing artifact: ${ARTIFACT}"
  exit 1
fi

ARTIFACT_URL="https://github.com/$(git remote get-url origin | sed -E 's/.*[:/]([^/]+\/[^/.]+)(\.git)?$/\1/' | sed -E 's/\.git$//')/releases/download/${VERSION}/recode-${VERSION}-linux-x86_64.AppImage.tar.gz"
if [ -z "${ARTIFACT_URL}" ] || [[ "${ARTIFACT_URL}" == https://github.com/releases/download/* ]]; then
  ARTIFACT_URL="file://${ARTIFACT}"
fi

cat > "${MANIFEST}" <<EOF
${VERSION}
${ARTIFACT_URL}
${NOTES}
$(sha256sum "${ARTIFACT}" | awk '{print $1}')
$(date -u +"%Y-%m-%dT%H:%M:%SZ")
EOF

echo "[recode] wrote update manifest to ${MANIFEST}"
