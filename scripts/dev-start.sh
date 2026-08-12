#!/bin/bash
# Archipelago Development Server Starter

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FRONTEND_DIR="$PROJECT_ROOT/neode-ui"
BACKEND_DIR="$PROJECT_ROOT/core"

# Quietly kill a port — avoids EAGAIN by not piping through xargs
kill_port() {
    local pids
    pids=$(lsof -ti:"$1" 2>/dev/null) || true
    if [ -n "$pids" ]; then
        echo "$pids" | while read -r pid; do
            kill -9 "$pid" 2>/dev/null || true
        done
        sleep 1
    fi
}

cleanup_ports() {
    kill_port 5959
    kill_port 8100
}

ensure_deps() {
    cd "$FRONTEND_DIR"
    if [ ! -d "node_modules" ]; then
        echo "   Installing dependencies..."
        npm install
    fi
}

if [ ! -d "$FRONTEND_DIR" ]; then
    echo "Frontend directory not found: $FRONTEND_DIR"
    exit 1
fi

echo ""
echo "Archipelago Dev Server"
echo ""

# Detect if running on a Linux dev machine (production-like mode available)
IS_LINUX=false
if [[ "$OSTYPE" == "linux"* ]]; then
    IS_LINUX=true
fi

echo "  0) Boot branding dev (GRUB theme, Plymouth, installer — patch + QEMU)"
echo "  1) Mock backend (UI dev — fastest, no Docker/Podman needed)"
echo "  2) Full stack (Rust backend + frontend)"
echo "  3) Setup mode (first-time password setup — mock)"
echo "  4) Onboarding mode (onboarding flow — mock)"
echo "  5) Existing user (login screen — mock)"
echo "  6) Boot mode (simulated 25s startup — mock)"
echo "  7) Testnet stack (signet Bitcoin + LND + ThunderHub via Podman)"
echo "  8) Manual instructions"
echo "  9) Container orchestration dev (live testing on .228)"
if [ "$IS_LINUX" = true ]; then
echo " 10) Production build (Linux only — build, install, restart all services)"
echo "     Mirrors ISO exactly: backend + frontend + Tor + WG + NostrVPN + nginx"
fi
echo ""
read -p "Enter choice [0-10]: " choice

case $choice in
    0)
        echo ""
        echo "Boot Branding Dev"
        echo ""
        # Find an ISO to patch
        ISO=$(ls -t ~/Desktop/archipelago-dev-*.iso 2>/dev/null | head -1)
        if [ -z "$ISO" ]; then
            ISO=$(ls -t "$PROJECT_ROOT/image-recipe/results/archipelago-"*.iso 2>/dev/null | head -1)
        fi

        DEV_BRANDING="$PROJECT_ROOT/image-recipe/dev-branding.sh"

        if [ -z "$ISO" ] || [ ! -f "$ISO" ]; then
            echo "  No ISO found to patch. Options:"
            echo ""
            echo "  a) Preview GRUB background only (instant):"
            echo "     python3 image-recipe/branding/generate-grub-background.py /tmp/grub-bg.png && open /tmp/grub-bg.png"
            echo ""
            echo "  b) Download an ISO from FileBrowser (http://192.0.2.10:8083)"
            echo "     then drop it on your Desktop and re-run this option."
            echo ""
            echo "  Files you can edit:"
            echo "    image-recipe/branding/grub-theme/background.png  — GRUB boot background"
            echo "    image-recipe/branding/grub-theme/theme.txt       — GRUB menu colors/layout"
            echo "    image-recipe/branding/plymouth-theme/logo.png    — Plymouth boot logo"
            echo "    image-recipe/branding/plymouth-theme/*.script    — Plymouth animation"
            echo ""
            exit 0
        fi

        echo "  ISO: $ISO"
        echo "  Edit these files, then this script patches and boots in QEMU:"
        echo "    branding/grub-theme/background.png  — GRUB background"
        echo "    branding/grub-theme/theme.txt       — GRUB menu theme"
        echo "    branding/plymouth-theme/logo.png    — Plymouth logo"
        echo ""

        if [ -f "$DEV_BRANDING" ]; then
            exec bash "$DEV_BRANDING" "$ISO"
        else
            echo "  dev-branding.sh not found at: $DEV_BRANDING"
            exit 1
        fi
        ;;
    1)
        echo ""
        echo "Starting frontend with mock backend..."
        cleanup_ports
        ensure_deps
        exec npm run dev:mock
        ;;
    2)
        echo ""
        echo "Starting full stack (Rust backend + frontend)..."
        cleanup_ports

        if [ ! -d "$BACKEND_DIR" ]; then
            echo "Backend directory not found: $BACKEND_DIR"
            exit 1
        fi

        cd "$BACKEND_DIR"
        if ! cargo check --bin archipelago > /tmp/archipelago-backend-check.log 2>&1; then
            echo "Backend build check failed. See /tmp/archipelago-backend-check.log"
            echo "Falling back to mock backend."
            ensure_deps
            exec npm run dev:mock
        fi

        echo "   Starting Rust backend..."
        export ARCHIPELAGO_DATA_DIR=/tmp/archipelago-dev
        export ARCHIPELAGO_DEV_DATA_DIR=/tmp/archipelago-dev
        export ARCHIPELAGO_DEV_MODE=true
        export ARCHIPELAGO_BIND=127.0.0.1:5959
        export ARCHIPELAGO_LOG_LEVEL=debug
        export ARCHIPELAGO_BITCOIN_SIMULATION=mock

        cargo run --bin archipelago > /tmp/archipelago-backend.log 2>&1 &
        BACKEND_PID=$!
        echo "   Backend PID: $BACKEND_PID (logs: /tmp/archipelago-backend.log)"

        echo "   Waiting for backend on port 5959..."
        for i in $(seq 1 60); do
            if lsof -ti:5959 >/dev/null 2>&1; then break; fi
            sleep 1
        done

        if ! lsof -ti:5959 >/dev/null 2>&1; then
            echo "Backend did not start. Falling back to mock."
            kill "$BACKEND_PID" 2>/dev/null || true
            ensure_deps
            exec npm run dev:mock
        fi

        echo "   Backend ready."
        trap "kill $BACKEND_PID 2>/dev/null" EXIT

        ensure_deps
        exec npm run dev
        ;;
    3)
        echo ""
        echo "Starting setup mode..."
        cleanup_ports
        ensure_deps
        VITE_DEV_MODE=setup exec npm run dev:mock
        ;;
    4)
        echo ""
        echo "Starting onboarding mode..."
        cleanup_ports
        ensure_deps
        VITE_DEV_MODE=onboarding exec npm run dev:mock
        ;;
    5)
        echo ""
        echo "Starting existing user mode..."
        cleanup_ports
        ensure_deps
        VITE_DEV_MODE=existing exec npm run dev:mock
        ;;
    6)
        echo ""
        echo "Starting boot mode (25s simulated startup)..."
        cleanup_ports
        ensure_deps
        VITE_DEV_MODE=boot exec npm run dev:mock
        ;;
    7)
        echo ""
        echo "Starting testnet stack (signet) via Podman/Docker..."

        # Check for a working container runtime (binary exists AND daemon responds)
        RUNTIME=""
        COMPOSE=""
        if command -v docker &>/dev/null && docker ps &>/dev/null; then
            RUNTIME="docker"
            COMPOSE="docker compose"
        elif command -v podman &>/dev/null && podman ps &>/dev/null; then
            if command -v podman-compose &>/dev/null; then
                RUNTIME="podman"
                COMPOSE="podman-compose"
            else
                RUNTIME="podman"
                COMPOSE="podman compose"
            fi
        fi

        if [ -z "$RUNTIME" ]; then
            if command -v podman &>/dev/null; then
                echo "   Podman machine not running — starting it..."
                if ! podman machine ls --format '{{.Name}}' 2>/dev/null | grep -q .; then
                    echo "   No Podman machine found — initializing..."
                    podman machine init
                fi
                podman machine start
                if podman ps &>/dev/null; then
                    if command -v podman-compose &>/dev/null; then
                        RUNTIME="podman"
                        COMPOSE="podman-compose"
                    else
                        RUNTIME="podman"
                        COMPOSE="podman compose"
                    fi
                else
                    echo "   Failed to start Podman machine."
                    exit 1
                fi
            elif command -v docker &>/dev/null; then
                echo ""
                echo "Docker is installed but the daemon isn't running."
                echo "Start Docker Desktop and try again."
                exit 1
            else
                echo ""
                echo "No container runtime found. Install one:"
                echo "  brew install podman podman-compose"
                echo "  # or"
                echo "  brew install --cask docker"
                exit 1
            fi
        fi

        echo "   Using: $RUNTIME"
        cd "$PROJECT_ROOT"

        echo "   Starting signet Bitcoin + LND + ThunderHub + Fedimint..."
        $COMPOSE -f docker-compose.testnet.yml up -d

        echo ""
        echo "   Testnet stack starting. Services:"
        echo "     ThunderHub:       http://localhost:3010  (password: thunderhub)"
        echo "     Fedimint Guardian: http://localhost:18175"
        echo "     LND REST:         http://localhost:8080"
        echo "     Bitcoin RPC:      localhost:38332"
        echo ""
        echo "   Get signet coins: https://signetfaucet.com"
        echo ""
        echo "   Also starting mock frontend..."
        cleanup_ports
        ensure_deps
        exec npm run dev:mock
        ;;
    8)
        echo ""
        echo "Manual Instructions"
        echo ""
        echo "UI development (mock backend, no Docker):"
        echo "  cd $FRONTEND_DIR"
        echo "  npm install && npm run dev:mock"
        echo ""
        echo "Dev modes (prepend to command):"
        echo "  VITE_DEV_MODE=setup       First-time setup flow"
        echo "  VITE_DEV_MODE=onboarding  Onboarding flow"
        echo "  VITE_DEV_MODE=existing    Login screen"
        echo "  VITE_DEV_MODE=boot        Boot sequence"
        echo ""
        echo "Testnet stack (requires Podman or Docker):"
        echo "  podman compose -f docker-compose.testnet.yml up -d"
        echo ""
        echo "Full stack (requires Rust toolchain):"
        echo "  Terminal 1: cd $BACKEND_DIR && cargo run --bin archipelago"
        echo "  Terminal 2: cd $FRONTEND_DIR && npm run dev"
        echo ""
        echo "Access: http://localhost:8100  (password: password123)"
        ;;
    9)
        echo ""
        echo "Container Orchestration Dev (live testing on .228)"
        echo "Syncs code, builds on server, runs orchestration smoke tests."
        echo ""
        exec "$SCRIPT_DIR/dev-container-test.sh"
        ;;
    10)
        if [ "$IS_LINUX" != true ]; then
            echo "Production build is only available on Linux dev machines."
            exit 1
        fi

        echo ""
        echo "Production Build — mirrors ISO install exactly"
        echo ""

        FAILED=0

        # Step 1: Build backend
        echo "[1/5] Building Rust backend (release)..."
        cd "$BACKEND_DIR/archipelago"
        if cargo build --release 2>&1 | tail -3; then
            RELEASE_BIN="$BACKEND_DIR/target/release/archipelago"
            sudo cp "$RELEASE_BIN" /usr/local/bin/archipelago
            sudo chmod +x /usr/local/bin/archipelago
            echo "   Backend installed: $(ls -lh /usr/local/bin/archipelago | awk '{print $5}')"
        else
            echo "   FAILED: cargo build --release"
            FAILED=1
        fi

        # Step 2: Type-check + build frontend
        echo "[2/5] Building frontend..."
        cd "$FRONTEND_DIR"
        if [ ! -d "node_modules" ]; then
            npm install
        fi
        if npx vue-tsc -b --noEmit 2>&1 | tail -3; then
            npm run build 2>&1 | tail -3
            sudo cp -r "$PROJECT_ROOT/web/dist/neode-ui/"* /opt/archipelago/web-ui/
            # Deploy AIUI (pre-built demo or source build)
            if [ -d "$PROJECT_ROOT/../AIUI/packages/app/dist" ]; then
                sudo cp -r "$PROJECT_ROOT/../AIUI/packages/app/dist/"* /opt/archipelago/web-ui/aiui/
                echo "   AIUI deployed from source build"
            elif [ -d "$PROJECT_ROOT/demo/aiui" ]; then
                sudo mkdir -p /opt/archipelago/web-ui/aiui/
                sudo cp -r "$PROJECT_ROOT/demo/aiui/"* /opt/archipelago/web-ui/aiui/
                echo "   AIUI deployed from demo/"
            fi
            echo "   Frontend deployed to /opt/archipelago/web-ui/"
        else
            echo "   FAILED: vue-tsc type check"
            FAILED=1
        fi

        # Step 3: Sync configs from repo
        echo "[3/5] Syncing configs..."
        sudo cp "$PROJECT_ROOT/image-recipe/configs/archipelago.service" /etc/systemd/system/archipelago.service
        sudo cp "$PROJECT_ROOT/image-recipe/configs/nginx-archipelago.conf" /etc/nginx/sites-available/archipelago
        sudo cp "$PROJECT_ROOT/image-recipe/configs/snippets/"*.conf /etc/nginx/snippets/ 2>/dev/null
        for unit in archipelago-tor-helper.service archipelago-tor-helper.path archipelago-wg.service archipelago-wg-address.service nostr-relay.service nostr-vpn.service; do
            sudo cp "$PROJECT_ROOT/image-recipe/configs/$unit" "/etc/systemd/system/$unit"
        done
        sudo cp "$PROJECT_ROOT/scripts/tor-helper.sh" /opt/archipelago/scripts/tor-helper.sh
        sudo chmod +x /opt/archipelago/scripts/tor-helper.sh
        sudo cp "$PROJECT_ROOT/scripts/archipelago-wg" /usr/local/bin/archipelago-wg
        sudo chmod +x /usr/local/bin/archipelago-wg
        echo "   Configs synced"

        # Step 4: Sync Tor hostnames
        echo "[4/5] Syncing Tor hostnames..."
        for svc in archipelago bitcoin electrumx lnd btcpay mempool fedimint; do
            dir="/var/lib/archipelago/tor/hidden_service_$svc"
            if [ -f "$dir/hostname" ]; then
                sudo cp "$dir/hostname" "/var/lib/archipelago/tor-hostnames/$svc"
            fi
        done
        sudo chown -R "$(whoami)":"$(whoami)" /var/lib/archipelago/tor-hostnames 2>/dev/null

        # Step 5: Reload and restart all services
        echo "[5/5] Restarting services..."
        sudo systemctl daemon-reload
        sudo nginx -t 2>&1 && sudo systemctl reload nginx
        sudo systemctl restart archipelago

        # Verify
        echo ""
        echo "Service Status:"
        for svc in tor@default archipelago-wg archipelago-wg-address nostr-relay nostr-vpn archipelago-tor-helper.path archipelago nginx; do
            STATUS=$(systemctl is-active "$svc" 2>/dev/null)
            if [ "$STATUS" = "active" ]; then
                printf "  %-30s active\n" "$svc"
            else
                printf "  %-30s FAILED\n" "$svc"
                FAILED=1
            fi
        done

        echo ""
        if [ "$FAILED" -eq 0 ]; then
            HOST_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
            ONION=$(cat /var/lib/archipelago/tor-hostnames/archipelago 2>/dev/null || echo "generating...")
            echo "All services running. Access:"
            echo "  LAN:   http://$HOST_IP"
            echo "  Tor:   http://$ONION"
            echo "  WG:    10.44.0.1"
            echo "  RPC:   http://127.0.0.1:5678/rpc/v1"
        else
            echo "Some services failed. Check: journalctl -u <service> --no-pager -n 20"
        fi
        ;;
    *)
        echo "Invalid choice"
        exit 1
        ;;
esac
