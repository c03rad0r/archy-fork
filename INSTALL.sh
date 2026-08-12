#!/bin/bash
# Archipelago Installation Script
# This script installs all dependencies needed to run the Archipelago project

set -e

echo "============================================"
echo "Archipelago Dependencies Installation"
echo "============================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to print status
print_status() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓${NC} $2"
    else
        echo -e "${RED}✗${NC} $2"
    fi
}

# Check and install Homebrew (macOS package manager)
echo "Checking Homebrew..."
if command_exists brew; then
    print_status 0 "Homebrew already installed"
else
    echo -e "${YELLOW}Installing Homebrew...${NC}"
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    
    # Add Homebrew to PATH for Apple Silicon Macs
    if [[ $(uname -m) == 'arm64' ]]; then
        echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
        eval "$(/opt/homebrew/bin/brew shellenv)"
    fi
    print_status 0 "Homebrew installed"
fi

echo ""

# Install Rust
echo "Checking Rust..."
if command_exists rustc && command_exists cargo; then
    print_status 0 "Rust already installed ($(rustc --version))"
else
    echo -e "${YELLOW}Installing Rust...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    print_status 0 "Rust installed ($(rustc --version))"
fi

echo ""

# Install Node.js
echo "Checking Node.js..."
if command_exists node && command_exists npm; then
    NODE_VERSION=$(node --version)
    print_status 0 "Node.js already installed ($NODE_VERSION)"
else
    echo -e "${YELLOW}Installing Node.js...${NC}"
    brew install node
    print_status 0 "Node.js installed ($(node --version))"
fi

echo ""

# Install Podman
echo "Checking Podman..."
if command_exists podman; then
    print_status 0 "Podman already installed ($(podman --version))"
else
    echo -e "${YELLOW}Installing Podman...${NC}"
    brew install podman
    print_status 0 "Podman installed"
    
    echo -e "${YELLOW}Initializing Podman machine...${NC}"
    podman machine init
    podman machine start
    print_status 0 "Podman machine initialized and started"
fi

echo ""

# Install PostgreSQL
echo "Checking PostgreSQL..."
if command_exists psql; then
    print_status 0 "PostgreSQL already installed"
else
    echo -e "${YELLOW}Installing PostgreSQL...${NC}"
    brew install postgresql@15
    print_status 0 "PostgreSQL installed"
    
    echo -e "${YELLOW}Starting PostgreSQL service...${NC}"
    brew services start postgresql@15
    print_status 0 "PostgreSQL service started"
fi

echo ""
echo "============================================"
echo "Installing Project Dependencies"
echo "============================================"
echo ""

# Install frontend dependencies
echo "Installing frontend dependencies (neode-ui)..."
cd neode-ui
npm install
print_status 0 "Frontend dependencies installed"
cd ..

echo ""

# Install custom app dependencies
echo "Installing custom app dependencies..."

for app in did-wallet morphos-server router; do
    if [ -d "apps/$app" ]; then
        echo "  - Installing $app dependencies..."
        cd "apps/$app"
        npm install
        cd ../..
        print_status 0 "$app dependencies installed"
    fi
done

echo ""

# Build Rust backend
echo "Building Rust backend..."
cd core
cargo build
print_status 0 "Backend built successfully"
cd ..

echo ""
echo "============================================"
echo "Installation Complete!"
echo "============================================"
echo ""
echo "Next steps:"
echo ""
echo "1. Start the backend:"
echo "   cd core"
echo "   cargo run --bin archipelago"
echo ""
echo "2. In another terminal, start the frontend:"
echo "   cd neode-ui"
echo "   npm run dev"
echo ""
echo "3. Open your browser to:"
echo "   http://localhost:8100"
echo ""
echo "For more information, see:"
echo "  - README.md"
echo "  - docs/developer-guide.md"
echo "  - apps/QUICKSTART.md"
echo ""
