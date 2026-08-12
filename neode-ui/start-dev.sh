#!/usr/bin/env bash

# Neode Development Server Startup Script
# This script starts both the mock backend and Vite dev server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo ""
echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║${NC}                                                            ${BLUE}║${NC}"
echo -e "${BLUE}║${NC}   ${GREEN}🚀 Starting Neode Development Environment${NC}             ${BLUE}║${NC}"
echo -e "${BLUE}║${NC}                                                            ${BLUE}║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Function to check if a port is in use
check_port() {
    lsof -ti:$1 > /dev/null 2>&1
}

# Function to kill process on a port (avoids xargs which causes EAGAIN on macOS)
kill_port() {
    local port=$1
    local pids
    pids=$(lsof -ti:"$port" 2>/dev/null) || true
    if [ -n "$pids" ]; then
        echo -e "${YELLOW}  Port $port in use, cleaning up...${NC}"
        echo "$pids" | while read -r pid; do
            kill -9 "$pid" 2>/dev/null || true
        done
        sleep 1
    fi
}

# Clean up function
cleanup() {
    echo ""
    echo -e "${YELLOW}🛑 Shutting down servers...${NC}"
    
    # Kill all node processes started by this script
    if [ ! -z "$BACKEND_PID" ]; then
        kill $BACKEND_PID 2>/dev/null || true
    fi
    if [ ! -z "$VITE_PID" ]; then
        kill $VITE_PID 2>/dev/null || true
    fi
    
    # Also kill concurrently if it's running
    pkill -f "concurrently" 2>/dev/null || true
    
    echo -e "${GREEN}✅ Servers stopped${NC}"
    exit 0
}

# Set up cleanup trap
trap cleanup SIGINT SIGTERM EXIT

# Check for required ports and clean them up if needed
echo -e "${BLUE}🔍 Checking ports...${NC}"
kill_port 5959  # Mock backend
kill_port 8100  # Vite dev server
kill_port 8101  # Potential fallback port
kill_port 8102  # Another fallback port
kill_port 5173  # AIUI dev server

echo -e "${GREEN}✅ Ports cleared${NC}"
echo ""

# Mock backend handles app simulation — no Docker required for dev
echo -e "${GREEN}   Mock backend will simulate all apps${NC}"
echo ""

# Check if node_modules exists
if [ ! -d "node_modules" ]; then
    echo -e "${YELLOW}⚠️  node_modules not found. Running npm install...${NC}"
    npm install
    echo ""
fi

# Start the servers using npm script
echo -e "${BLUE}🚀 Starting servers...${NC}"
echo ""

# Use npm run dev:mock (includes AIUI dev server automatically)
exec npm run dev:mock

