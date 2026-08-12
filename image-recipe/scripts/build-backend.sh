#!/bin/bash
# Build Archipelago backend binary for Debian Linux

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BACKEND_DIR="$PROJECT_ROOT/core/archipelago"
OUTPUT_DIR="$SCRIPT_DIR/../build/backend"

echo "🔨 Building Archipelago backend..."
echo "   Source: $BACKEND_DIR"
echo "   Output: $OUTPUT_DIR"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if we should use Docker
USE_DOCKER=false
if [[ "$OSTYPE" == "darwin"* ]]; then
    USE_DOCKER=true
    echo "🍎 macOS detected - using Docker for Linux build"
elif ! command -v rustc >/dev/null 2>&1; then
    USE_DOCKER=true
    echo "⚠️  Rust not found - using Docker"
fi

if [ "$USE_DOCKER" = true ]; then
    echo "🐳 Building in Docker container..."
    docker run --rm \
        -v "$PROJECT_ROOT:/workspace" \
        -v "$OUTPUT_DIR:/output" \
        -w /workspace/core/archipelago \
        rust:trixie \
        sh -c '
            echo "📦 Installing build dependencies..."
            apt-get update && apt-get install -y pkg-config libssl-dev
            
            echo "🔨 Building Archipelago backend..."
            cargo build --release
            
            echo "📋 Copying binary to output..."
            cp ../target/release/archipelago /output/
            
            echo "✅ Build complete!"
            ls -lh /output/archipelago
        '
else
    # Native Linux build
    echo "🐧 Building natively..."
    cd "$BACKEND_DIR"
    cargo build --release
    
    cp "../target/release/archipelago" "$OUTPUT_DIR/archipelago"
fi

# Strip binary for smaller size
if [ -f "$OUTPUT_DIR/archipelago" ]; then
    if command -v strip >/dev/null 2>&1 && [[ "$OSTYPE" != "darwin"* ]]; then
        strip "$OUTPUT_DIR/archipelago"
    fi
    echo ""
    echo "✅ Backend built: $OUTPUT_DIR/archipelago"
    ls -lh "$OUTPUT_DIR/archipelago"
else
    echo "❌ Build failed - binary not found"
    exit 1
fi
