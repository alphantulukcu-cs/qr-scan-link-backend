#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <image_repo> [version]"
  echo "Example: $0 alphantulukcucs/scan-link v1.0.0"
  exit 1
fi

IMAGE_REPO="$1"
VERSION="${2:-$(date +%Y.%m.%d-%H%M%S)}"
PUSH_LATEST="${PUSH_LATEST:-false}"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRANCH_INTEL_UI_CONTEXT="${BRANCH_INTEL_UI_CONTEXT:-$PROJECT_ROOT/../Deamon-ui/branch-intel-ui}"
QR_SCANNER_UI_CONTEXT="${QR_SCANNER_UI_CONTEXT:-$PROJECT_ROOT/../Deamon-ui/qr-scanner-ui}"

BACKEND_IMAGE="${IMAGE_REPO}:backend-${VERSION}"
BRANCH_INTEL_UI_IMAGE="${IMAGE_REPO}:branch-ui-${VERSION}"
QR_SCANNER_UI_IMAGE="${IMAGE_REPO}:qr-ui-${VERSION}"

echo "Building images with version: ${VERSION}"

docker build \
  -t "${BACKEND_IMAGE}" \
  -f "${PROJECT_ROOT}/Dockerfile" \
  "${PROJECT_ROOT}"

docker build \
  -t "${BRANCH_INTEL_UI_IMAGE}" \
  -f "${BRANCH_INTEL_UI_CONTEXT}/Dockerfile" \
  "${BRANCH_INTEL_UI_CONTEXT}"

docker build \
  -t "${QR_SCANNER_UI_IMAGE}" \
  -f "${QR_SCANNER_UI_CONTEXT}/Dockerfile" \
  "${QR_SCANNER_UI_CONTEXT}"

echo "Pushing images to registry..."
docker push "${BACKEND_IMAGE}"
docker push "${BRANCH_INTEL_UI_IMAGE}"
docker push "${QR_SCANNER_UI_IMAGE}"

if [[ "${PUSH_LATEST}" == "true" ]]; then
  docker tag "${BACKEND_IMAGE}" "${IMAGE_REPO}:backend-latest"
  docker tag "${BRANCH_INTEL_UI_IMAGE}" "${IMAGE_REPO}:branch-ui-latest"
  docker tag "${QR_SCANNER_UI_IMAGE}" "${IMAGE_REPO}:qr-ui-latest"

  docker push "${IMAGE_REPO}:backend-latest"
  docker push "${IMAGE_REPO}:branch-ui-latest"
  docker push "${IMAGE_REPO}:qr-ui-latest"
fi

cat <<EOF

Done.

Use these in .env.registry on the private machine:
BACKEND_IMAGE=${BACKEND_IMAGE}
BRANCH_INTEL_UI_IMAGE=${BRANCH_INTEL_UI_IMAGE}
QR_SCANNER_UI_IMAGE=${QR_SCANNER_UI_IMAGE}
EOF
