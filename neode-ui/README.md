# Archipelago Web UI

Vue 3 + TypeScript + Vite + Tailwind CSS + Pinia

The web interface for Archipelago — a self-sovereign Bitcoin Node OS.

## Quick Start

```bash
cd neode-ui
npm install
npm start
```

Visit **http://localhost:8100** — login with password: `password123`

This starts:
- Mock backend on port 5959 (no Docker required)
- Vite dev server on port 8100 with HMR

Stop with `npm stop`.

## Architecture

```
neode-ui/
├── src/
│   ├── api/              # RPC client (rpc-client.ts), WebSocket, container client
│   ├── stores/           # Pinia stores (app, container, appLauncher, monitoring)
│   ├── views/            # Page components (Dashboard, Marketplace, Settings, etc.)
│   ├── components/       # Reusable components (SplashScreen, AppSession, etc.)
│   ├── router/           # Vue Router configuration
│   ├── types/            # TypeScript type definitions
│   └── style.css         # Global styles + Tailwind utilities
├── public/assets/        # Static assets (images, fonts, app icons, audio)
├── mock-backend.js       # Mock backend server (simulates Rust backend)
├── docker/               # Docker configs (nginx, entrypoint)
└── vite.config.ts        # Vite config with backend proxy
```

## Dev Modes

The mock backend supports multiple startup modes via `VITE_DEV_MODE`:

| Mode | Command | Behavior |
|------|---------|----------|
| **default** | `npm start` | Fully set up, login screen |
| **existing** | `VITE_DEV_MODE=existing npm run dev:mock` | Same as default |
| **setup** | `VITE_DEV_MODE=setup npm run dev:mock` | First-time password setup flow |
| **onboarding** | `VITE_DEV_MODE=onboarding npm run dev:mock` | Post-setup onboarding flow |
| **boot** | `npm run dev:boot` | 25s simulated boot sequence |

## Mock Backend

The mock backend (`mock-backend.js`) simulates the full Rust backend for local development:

**Pre-installed apps** (always visible in My Apps):
- Bitcoin Core, LND, Electrs, Mempool, FileBrowser, LoraBell, Fedimint

**Marketplace**: 30+ curated apps with Docker images, install/uninstall simulation

**Features simulated**:
- Authentication (login, password change, TOTP 2FA)
- System metrics (CPU, memory, disk — randomized for realism)
- Node identity (DID, Nostr pubkey, Tor address)
- Federation (3 mock nodes with apps, metrics, trust levels)
- Mesh networking (4 LoRa peers, encrypted messaging, invoices)
- Peer-to-peer messaging
- FileBrowser API (mock file system with Music, Documents, Photos, Videos)
- DWN sync status
- Transport layer (mesh/LAN/Tor routing)
- Notifications (5 realistic entries)
- Claude AI chat proxy (requires `ANTHROPIC_API_KEY`)

**Container runtime**: If Docker/Podman is available, the mock backend will run real containers for installed apps. Otherwise, it simulates them.

## Demo Deployment (Portainer)

Deploy the demo via Docker Compose for showcasing:

```bash
docker compose -f docker-compose.demo.yml build
docker compose -f docker-compose.demo.yml up -d
```

Or deploy through **Portainer Stacks**:
1. Stacks > Add stack > name: `archy-demo`
2. Web editor: paste `docker-compose.demo.yml` contents
3. Add environment variable: `ANTHROPIC_API_KEY` (for Claude chat)
4. Deploy

Access at **http://your-host:4848** — password: `password123`

## Development Commands

```bash
npm start              # Start mock backend + Vite (recommended)
npm stop               # Stop all servers
npm run dev:mock       # Same as start, without port cleanup
npm run dev:boot       # Boot mode (simulated startup delay)
npm run backend:mock   # Mock backend only
npm run dev            # Vite only (needs backend running separately)
npm run dev:real       # Vite with real Rust backend

npm run build          # Production build (outputs to ../web/dist/neode-ui/)
npm run build:docker   # Build for Docker (no type checking)
npm run type-check     # TypeScript type checking
npm test               # Run tests
```

## Design System

### Glass Classes

| Class | Use |
|-------|-----|
| `.glass-card` | Content containers, modals, panels |
| `.glass-button` | ALL buttons (primary and secondary) |
| `.path-option-card` | Interactive cards with hover lift |
| `.info-card` | Status badges, metric displays |

### Tokens

- **Font**: Avenir Next (primary), Montserrat (`font-archipelago`)
- **Glass**: `bg: rgba(0,0,0,0.60)`, `blur: 24px`, `border: rgba(255,255,255,0.22)`
- **Accent**: `#fb923c` (Bitcoin orange), `#4ade80` (green), `#ef4444` (red)
- **Text**: `rgba(255,255,255,0.9)` primary, `rgba(255,255,255,0.6)` muted

### Rules

- Global CSS classes in `style.css` only — never inline Tailwind in components
- `.gradient-button` is **banned** — use `.glass-button`
- All components use `<script setup lang="ts">`

## API

```typescript
import { rpcClient } from '@/api/rpc-client'

await rpcClient.login('password')
await rpcClient.startPackage('bitcoin')
const metrics = await rpcClient.getMetrics()
```

State management via Pinia stores. WebSocket patches applied automatically.

## Build Output

- **Dev build**: `../web/dist/neode-ui/`
- **Docker build**: `dist/` (deployed to nginx)

## License

MIT
