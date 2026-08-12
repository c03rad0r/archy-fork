#!/bin/bash
# Build Indeehub container image from the indeehub-frontend project
# Usage: ./build-from-prototype.sh [path-to-indeehub-frontend]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_FRONTEND="$HOME/Projects/indeehub-frontend"
FRONTEND_DIR="${1:-$DEFAULT_FRONTEND}"
IMAGE_TAG="localhost/indeedhub:latest"

if [ ! -d "$FRONTEND_DIR" ]; then
    echo "Indeehub frontend not found at: $FRONTEND_DIR"
    echo "   Set path: $0 /path/to/indeehub-frontend"
    exit 1
fi

if [ ! -f "$FRONTEND_DIR/package.json" ]; then
    echo "No package.json found in $FRONTEND_DIR — is this the right directory?"
    exit 1
fi

# Determine container runtime
RUNTIME="podman"
if ! command -v podman >/dev/null 2>&1; then
    RUNTIME="docker"
fi

echo "Building Indeehub from $FRONTEND_DIR using $SCRIPT_DIR/Dockerfile"
$RUNTIME build -t "$IMAGE_TAG" -f "$SCRIPT_DIR/Dockerfile" "$FRONTEND_DIR"

echo "Built $IMAGE_TAG"
echo ""
echo "You can now install Indeehub from the App Store in Archipelago."
echo "Or run directly: $RUNTIME run -d --name indeedhub -p 8190:3000 $IMAGE_TAG"
