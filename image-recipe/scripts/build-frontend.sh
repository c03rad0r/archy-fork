#!/bin/bash
# Build Archipelago frontend for production

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FRONTEND_DIR="$PROJECT_ROOT/neode-ui"
OUTPUT_DIR="$SCRIPT_DIR/../build/frontend"

echo "🎨 Building Archipelago frontend..."
echo "   Source: $FRONTEND_DIR"
echo "   Output: $OUTPUT_DIR"
echo ""

# Check if Node.js is installed
if ! command -v node >/dev/null 2>&1; then
    echo "❌ Node.js not found. Please install Node.js 18+"
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Install dependencies if needed
cd "$FRONTEND_DIR"
if [ ! -d "node_modules" ]; then
    echo "📦 Installing frontend dependencies..."
    npm install
fi

# Build frontend
echo "🔨 Building frontend..."
DOCKER_BUILD=true npm run build || npm run build

# Copy built files
if [ -d "dist" ]; then
    cp -r dist/* "$OUTPUT_DIR/"
elif [ -d "../web/dist/neode-ui" ]; then
    cp -r ../web/dist/neode-ui/* "$OUTPUT_DIR/"
else
    echo "❌ Build output not found"
    exit 1
fi

echo "✅ Frontend built: $OUTPUT_DIR"
du -sh "$OUTPUT_DIR"
