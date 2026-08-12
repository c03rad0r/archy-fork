#!/bin/bash
# Build script for Archipelago apps
# Usage: ./build.sh [app-id] [--dev]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPS_DIR="$SCRIPT_DIR"

# Determine container runtime
RUNTIME="auto"
if command -v podman >/dev/null 2>&1 && podman info >/dev/null 2>&1; then
    RUNTIME="podman"
elif command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    RUNTIME="docker"
else
    echo "❌ No container runtime available (Podman or Docker required)"
    exit 1
fi

echo "🐳 Using runtime: $RUNTIME"

# Build function
build_app() {
    local app_id=$1
    local app_dir="$APPS_DIR/$app_id"
    local dev_mode=$2
    
    if [ ! -d "$app_dir" ]; then
        echo "❌ App directory not found: $app_dir"
        return 1
    fi
    
    if [ ! -f "$app_dir/Dockerfile" ]; then
        echo "⚠️  No Dockerfile found for $app_id, skipping..."
        return 0
    fi
    
    echo ""
    echo "🔨 Building $app_id..."
    
    local image_tag="archipelago/$app_id:latest"
    if [ "$dev_mode" = "--dev" ]; then
        image_tag="archipelago/$app_id:dev"
    fi
    
    cd "$app_dir"
    
    # For Node.js apps, install dependencies first
    if [ -f "package.json" ]; then
        echo "   Installing Node.js dependencies..."
        npm install --production=false
    fi
    
    # Build the image
    echo "   Building container image: $image_tag"
    $RUNTIME build -t "$image_tag" .
    
    if [ $? -eq 0 ]; then
        echo "✅ $app_id built successfully: $image_tag"
    else
        echo "❌ Failed to build $app_id"
        return 1
    fi
}

# Main logic
if [ $# -eq 0 ]; then
    # Build all apps
    echo "🔨 Building all Archipelago apps..."
    for app_dir in "$APPS_DIR"/*/; do
        if [ -d "$app_dir" ] && [ -f "$app_dir/manifest.yml" ]; then
            app_id=$(basename "$app_dir")
            build_app "$app_id" "$@"
        fi
    done
else
    # Build specific app
    app_id=$1
    shift
    build_app "$app_id" "$@"
fi

echo ""
echo "✅ Build complete!"
