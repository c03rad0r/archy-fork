#!/bin/bash
# Check build dependencies and provide installation instructions

set -e

echo "🔍 Checking build dependencies..."
echo ""

MISSING_DEPS=0

# Check Rust
if command -v rustc >/dev/null 2>&1; then
    echo "✅ Rust: $(rustc --version)"
else
    echo "❌ Rust: Not found"
    echo "   Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    MISSING_DEPS=$((MISSING_DEPS + 1))
fi

# Check Node.js
if command -v node >/dev/null 2>&1; then
    NODE_VERSION=$(node --version)
    NODE_MAJOR=$(echo "$NODE_VERSION" | cut -d. -f1 | tr -d 'v')
    if [ "$NODE_MAJOR" -ge 18 ]; then
        echo "✅ Node.js: $NODE_VERSION"
    else
        echo "⚠️  Node.js: $NODE_VERSION (need 18+)"
        MISSING_DEPS=$((MISSING_DEPS + 1))
    fi
else
    echo "❌ Node.js: Not found"
    echo "   Install: https://nodejs.org/"
    MISSING_DEPS=$((MISSING_DEPS + 1))
fi

# Check Docker
if command -v docker >/dev/null 2>&1; then
    if docker info >/dev/null 2>&1; then
        echo "✅ Docker: $(docker --version)"
    else
        echo "⚠️  Docker: Installed but daemon not running"
        echo "   Start Docker Desktop or: sudo systemctl start docker"
    fi
else
    echo "❌ Docker: Not found"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "   Install: https://www.docker.com/products/docker-desktop"
        MISSING_DEPS=$((MISSING_DEPS + 1))
    else
        echo "   Install: https://docs.docker.com/get-docker/"
        echo "   (Optional on Linux if building natively)"
    fi
fi

# Check xorriso (for ISO creation)
if command -v xorriso >/dev/null 2>&1; then
    echo "✅ xorriso: Installed"
else
    echo "❌ xorriso: Not found (needed for ISO creation)"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "   Install: brew install xorriso"
    else
        echo "   Install: apt-get install xorriso"
    fi
    MISSING_DEPS=$((MISSING_DEPS + 1))
fi

# Check 7z (for ISO extraction)
if command -v 7z >/dev/null 2>&1; then
    echo "✅ 7z: Installed"
else
    echo "❌ 7z: Not found (needed for ISO extraction)"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "   Install: brew install p7zip"
    else
        echo "   Install: apt-get install p7zip-full"
    fi
    MISSING_DEPS=$((MISSING_DEPS + 1))
fi

echo ""
if [ $MISSING_DEPS -eq 0 ]; then
    echo "✅ All dependencies satisfied!"
    exit 0
else
    echo "❌ Missing $MISSING_DEPS dependency/dependencies"
    echo "   Please install missing dependencies before building"
    exit 1
fi
