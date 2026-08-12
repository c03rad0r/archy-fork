# 🚀 Neode Development Scripts

Quick reference for starting and stopping the Neode development environment.

## Quick Start

### Start Everything (Recommended)
```bash
npm start
# or
./start-dev.sh
```

This will:
- ✅ Check and clean up any processes on ports 5959, 8100-8102
- ✅ Start Docker Desktop if it's not running (waits up to 60 seconds)
- ✅ Start the mock backend (port 5959)
- ✅ Start Vite dev server (port 8100)
- ✅ Display status with color-coded output

**Access the app:**
- **Frontend**: http://localhost:8100
- **Backend RPC**: http://localhost:5959/rpc/v1
- **WebSocket**: ws://localhost:5959/ws/db

**Login credentials:**
- Password: `password123`

### Stop Everything
```bash
npm stop
# or
./stop-dev.sh
```

This will cleanly shut down:
- Mock backend server
- Vite dev server
- All related processes

---

## Individual Commands

### Run Mock Backend Only
```bash
npm run backend:mock
```

### Run Vite Only
```bash
npm run dev
```

### Run Both (without cleanup)
```bash
npm run dev:mock
```

### Run with Real Rust Backend
```bash
# Terminal 1: Start Rust backend
cd ../core
cargo run --release

# Terminal 2: Start Vite
npm run dev:real
```

---

## Troubleshooting

### Port Already in Use
If you see port conflicts, run:
```bash
npm stop
```

Then start again:
```bash
npm start
```

### Kill All Node Processes (Nuclear Option)
```bash
pkill -9 node
```

### Check What's Running on a Port
```bash
# Check port 5959
lsof -i :5959

# Check port 8100
lsof -i :8100
```

### View Logs
If running in background, logs are in:
```bash
tail -f /tmp/neode-dev.log
```

---

## Features

### Mock Backend
- **Docker Optional** - Apps run for real if Docker/Podman is available, otherwise simulated
- **Auto-Detection** - Automatically detects container runtime and adapts
- **WebSocket Support** - Real-time state updates via JSON patches
- **Pre-loaded Apps** - 7 apps always visible in My Apps

### Pre-installed Apps (always running in mock mode)
- `bitcoin` - Bitcoin Core (port 8332)
- `lnd` - Lightning Network Daemon (port 8080)
- `electrs` - Electrum Server in Rust (port 50001)
- `mempool` - Blockchain explorer (port 4080)
- `filebrowser` - Web file manager (port 8083)
- `lorabell` - LoRa doorbell (no UI port)
- `fedimint` - Federated Bitcoin mint (port 8175)

Additional apps can be installed from the Marketplace (30+ available).

---

## Development Workflow

1. **Start servers:**
   ```bash
   npm start
   ```

2. **Open browser:**
   ```
   http://localhost:8100
   ```

3. **Login:**
   ```
   password123
   ```

4. **Make changes** - Vite HMR will reload instantly

5. **Stop servers when done:**
   ```bash
   npm stop
   ```

---

## Build Commands

### Development Build
```bash
npm run build
```

### Docker Build (no type checking)
```bash
npm run build:docker
```

### Type Check Only
```bash
npm run type-check
```

### Preview Production Build
```bash
npm run preview
```

---

## Script Details

### start-dev.sh
- Checks all required ports (5959, 8100-8102)
- Kills any existing processes on those ports
- Verifies node_modules are installed
- Starts both servers with concurrently
- Handles Ctrl+C gracefully
- Color-coded output for easy reading

### stop-dev.sh
- Finds all Neode-related processes
- Kills by port (5959, 8100-8102)
- Kills by process name (mock-backend, vite, concurrently)
- Confirms each shutdown with status messages
- Color-coded output

---

## Tips

- Always use `npm start` for the cleanest experience
- Run `npm stop` before switching branches if there are backend changes
- Vite will try alternate ports (8101, 8102) if 8100 is busy
- Mock backend simulates 1.5s installation delay for realism

---

## Known Issues

### Node.js Version Warning
```
You are using Node.js 20.18.2. Vite requires Node.js version 20.19+ or 22.12+.
```

**To fix:**
```bash
# Using nvm (recommended)
nvm install 22
nvm use 22

# Or upgrade directly
brew upgrade node
```

The warning is non-fatal - Vite still works, but upgrading is recommended.

---

Happy coding! 🎨⚡
