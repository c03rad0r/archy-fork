#!/usr/bin/env bash

# Neode Development Server Stop Script
# This script stops all running Neode dev servers

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo ""
echo -e "${YELLOW}🛑 Stopping Neode Development Servers...${NC}"
echo ""

# Function to kill processes on a port
kill_port() {
    local port=$1
    local name=$2
    if lsof -ti:$port > /dev/null 2>&1; then
        echo -e "${BLUE}   Stopping $name on port $port...${NC}"
        lsof -ti:$port | xargs kill -9 2>/dev/null || true
        echo -e "${GREEN}   ✅ $name stopped${NC}"
    else
        echo -e "${YELLOW}   ℹ️  $name not running on port $port${NC}"
    fi
}

# Stop all node processes related to Neode
echo -e "${BLUE}🔍 Finding Neode processes...${NC}"
echo ""

# Kill by port
kill_port 5959 "Mock Backend"
kill_port 8100 "Vite Dev Server"
kill_port 8101 "Vite Dev Server (alt)"
kill_port 8102 "Vite Dev Server (alt)"

echo ""

# Kill any remaining concurrently processes
if pgrep -f "concurrently.*mock-backend" > /dev/null; then
    echo -e "${BLUE}   Stopping concurrently processes...${NC}"
    pkill -f "concurrently.*mock-backend" 2>/dev/null || true
    echo -e "${GREEN}   ✅ Concurrently stopped${NC}"
fi

# Kill any remaining node processes running mock-backend or vite
if pgrep -f "mock-backend.js" > /dev/null; then
    echo -e "${BLUE}   Stopping mock-backend.js...${NC}"
    pkill -f "mock-backend.js" 2>/dev/null || true
    echo -e "${GREEN}   ✅ Mock backend stopped${NC}"
fi

if pgrep -f "vite.*neode-ui" > /dev/null; then
    echo -e "${BLUE}   Stopping vite...${NC}"
    pkill -f "vite.*neode-ui" 2>/dev/null || true
    echo -e "${GREEN}   ✅ Vite stopped${NC}"
fi

echo ""
echo -e "${GREEN}✅ All Neode development servers stopped!${NC}"
echo ""

echo ""

