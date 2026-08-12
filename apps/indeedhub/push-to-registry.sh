#!/bin/bash
# Build and push Indeehub container image to a registry
# Usage: ./push-to-registry.sh [version]
#
# Environment variables:
#   REGISTRY    - Registry host (default: ghcr.io)
#   NAMESPACE   - Registry namespace (default: archipelago-os)
#   RUNTIME     - Container runtime (default: podman)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRONTEND_DIR="${INDEEHUB_FRONTEND:-$HOME/Projects/indeehub-frontend}"
VERSION="${1:-latest}"
REGISTRY="${REGISTRY:-source.archipelago-foundation.org}"
NAMESPACE="${NAMESPACE:-lfg2025}"
IMAGE_NAME="indeedhub"
RUNTIME="${RUNTIME:-podman}"

FULL_TAG="${REGISTRY}/${NAMESPACE}/${IMAGE_NAME}:${VERSION}"
LATEST_TAG="${REGISTRY}/${NAMESPACE}/${IMAGE_NAME}:latest"

if [ ! -d "$FRONTEND_DIR" ]; then
    echo "Indeehub frontend not found at: $FRONTEND_DIR"
    echo "Set INDEEHUB_FRONTEND=/path/to/indeehub-frontend"
    exit 1
fi

echo "=== Indeehub Container Registry Push ==="
echo "Source:    $FRONTEND_DIR"
echo "Image:     $FULL_TAG"
echo "Runtime:   $RUNTIME"
echo ""

# Step 1: Build for linux/amd64 (target architecture)
echo "[1/3] Building image..."
$RUNTIME build --platform linux/amd64 \
    -t "$FULL_TAG" \
    -t "$LATEST_TAG" \
    -t "localhost/${IMAGE_NAME}:latest" \
    -t "localhost/${IMAGE_NAME}:${VERSION}" \
    -f "$SCRIPT_DIR/Dockerfile" \
    "$FRONTEND_DIR"

echo "[2/3] Pushing to registry..."
# Login check
if ! $RUNTIME login --get-login "$REGISTRY" >/dev/null 2>&1; then
    echo ""
    echo "Not logged in to $REGISTRY."
    echo "Run: $RUNTIME login $REGISTRY"
    exit 1
fi

$RUNTIME push "$FULL_TAG"
if [ "$VERSION" != "latest" ]; then
    $RUNTIME push "$LATEST_TAG"
fi

echo ""
echo "[3/3] Done!"
echo ""
echo "Image pushed: $FULL_TAG"
if [ "$VERSION" != "latest" ]; then
    echo "Also tagged:  $LATEST_TAG"
fi
echo ""
echo "Federated nodes can now install via:"
echo "  podman pull $FULL_TAG"
echo ""
echo "Update marketplace dockerImage to: $FULL_TAG"
