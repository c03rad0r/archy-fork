#!/bin/bash
# Build Archipelago Vue.js frontend for production

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "🎨 Building Archipelago Frontend (Vue.js)"
echo ""

# Navigate to frontend directory
cd "$PROJECT_ROOT/neode-ui"

# Check if node_modules exists
if [ ! -d "node_modules" ]; then
    echo "📦 Installing dependencies..."
    npm install || {
        echo "❌ npm install failed"
        exit 1
    }
fi

# Build for production
echo "🔨 Building production bundle (skipping type check for speed)..."
npm run build:docker || {
    echo "❌ Build failed"
    exit 1
}

# Create output directory
BUILD_DIR="$SCRIPT_DIR/build/frontend"
mkdir -p "$BUILD_DIR"

# Copy dist to build directory (check both possible output locations)
echo "📋 Copying build artifacts..."
if [ -d "../web/dist/neode-ui" ]; then
    cp -r ../web/dist/neode-ui/* "$BUILD_DIR/" || {
        echo "❌ Failed to copy build artifacts"
        exit 1
    }
elif [ -d "dist" ]; then
    cp -r dist/* "$BUILD_DIR/" || {
        echo "❌ Failed to copy build artifacts"
        exit 1
    }
else
    echo "❌ Build output not found"
    exit 1
fi

echo ""
echo "✅ Frontend built successfully!"
echo "   Output: $BUILD_DIR"
echo "   Files:"
ls -lh "$BUILD_DIR" | head -10
echo ""
