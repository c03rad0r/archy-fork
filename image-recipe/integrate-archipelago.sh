#!/bin/bash
# Integrate Archipelago backend and frontend into custom ISO

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "🔗 Integrating Archipelago Components into ISO"
echo ""

# Check if backend exists
BACKEND_BIN="$SCRIPT_DIR/build/backend/archipelago"
if [ ! -f "$BACKEND_BIN" ]; then
    echo "❌ Backend binary not found at $BACKEND_BIN"
    echo "   Run: ./build-backend.sh first"
    exit 1
fi

# Check if frontend exists
FRONTEND_DIR="$SCRIPT_DIR/build/frontend"
if [ ! -d "$FRONTEND_DIR" ] || [ ! -f "$FRONTEND_DIR/index.html" ]; then
    echo "❌ Frontend build not found at $FRONTEND_DIR"
    echo "   Run: ./build-frontend.sh first"
    exit 1
fi

echo "✅ Found backend binary: $(du -h "$BACKEND_BIN" | cut -f1)"
echo "✅ Found frontend files"
echo ""

# Now rebuild the ISO with the integrated components
export INCLUDE_BACKEND="$BACKEND_BIN"
export INCLUDE_FRONTEND="$FRONTEND_DIR"

echo "🔨 Rebuilding ISO with Archipelago components..."
./build-custom-iso.sh

echo ""
echo "✅ Integration complete!"
echo ""
