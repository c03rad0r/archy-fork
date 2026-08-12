#!/bin/bash
# Podman Installation and Configuration Script for Archipelago
# Configures Podman for rootless operation

set -e

echo "🐳 Configuring Podman for rootless operation..."

if ! command -v catatonit >/dev/null 2>&1; then
    if command -v apt-get >/dev/null 2>&1; then
        apt-get update || true
        apt-get install -y catatonit || true
    elif command -v dnf >/dev/null 2>&1; then
        dnf install -y catatonit || true
    elif command -v apk >/dev/null 2>&1; then
        apk add catatonit || true
    fi
fi

command -v catatonit >/dev/null 2>&1 || echo "WARNING: catatonit not installed; Podman init-enabled containers may fail"

# Ensure archipelago user exists
if ! id "archipelago" &>/dev/null; then
    echo "Creating archipelago user..."
    adduser -D -s /bin/bash archipelago
fi

# Create Podman configuration directories
mkdir -p /home/archipelago/.config/containers
mkdir -p /home/archipelago/.local/share/containers/storage

# Configure storage
cat > /home/archipelago/.config/containers/storage.conf <<EOF
[storage]
driver = "overlay"
runroot = "/run/user/$(id -u archipelago)/containers"
graphroot = "/home/archipelago/.local/share/containers/storage"
EOF

# Configure registries (use Docker Hub and quay.io)
mkdir -p /home/archipelago/.config/containers/registries.conf.d
cat > /home/archipelago/.config/containers/registries.conf <<EOF
unqualified-search-registries = ["docker.io", "ghcr.io", "quay.io", "source.archipelago-foundation.org"]

[[registry]]
location = "source.archipelago-foundation.org"
insecure = true
EOF

# Set up subuid and subgid for rootless containers
if ! grep -q "^archipelago:" /etc/subuid; then
    echo "archipelago:100000:65536" >> /etc/subuid
fi

if ! grep -q "^archipelago:" /etc/subgid; then
    echo "archipelago:100000:65536" >> /etc/subgid
fi

# Create systemd user service directory
mkdir -p /home/archipelago/.config/systemd/user

# Enable lingering for archipelago user (allows user services to run without login)
loginctl enable-linger archipelago || true

# Ensure /run/user/1000 exists for podman socket
mkdir -p /run/user/1000
chown archipelago:archipelago /run/user/1000
chmod 700 /run/user/1000

# Enable podman API socket for archipelago user (backend connects via this)
su - archipelago -c "XDG_RUNTIME_DIR=/run/user/1000 systemctl --user enable podman.socket" || true
su - archipelago -c "XDG_RUNTIME_DIR=/run/user/1000 systemctl --user start podman.socket" || true

# Set proper permissions
chown -R archipelago:archipelago /home/archipelago/.config
chown -R archipelago:archipelago /home/archipelago/.local

echo "✅ Podman configuration complete!"
