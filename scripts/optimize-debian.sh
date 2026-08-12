#!/bin/bash
# Debian Linux optimization script for Archipelago
# Optimizes system settings for container workloads

set -e

echo "⚡ Optimizing Debian Linux for container workloads..."

# CPU Governor - set to performance for better container performance
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]; then
    echo "performance" > /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 2>/dev/null || true
fi

# I/O Scheduler - use none for NVMe or mq-deadline for SATA
if command -v lsblk >/dev/null 2>&1; then
    for disk in $(lsblk -d -o NAME -n); do
        if [ -f "/sys/block/$disk/queue/scheduler" ]; then
            # Prefer none (for NVMe) or mq-deadline (for SATA SSD)
            if grep -q "none" "/sys/block/$disk/queue/scheduler"; then
                echo none > "/sys/block/$disk/queue/scheduler" 2>/dev/null || true
            elif grep -q "mq-deadline" "/sys/block/$disk/queue/scheduler"; then
                echo mq-deadline > "/sys/block/$disk/queue/scheduler" 2>/dev/null || true
            fi
        fi
    done
fi

# Increase file descriptor limits
cat >> /etc/security/limits.conf <<EOF
* soft nofile 65536
* hard nofile 65536
root soft nofile 65536
root hard nofile 65536
EOF

# Optimize network settings for container networking
cat >> /etc/sysctl.d/99-archipelago.conf <<EOF
# Container networking optimizations
net.core.somaxconn = 4096
net.ipv4.tcp_max_syn_backlog = 4096
net.core.netdev_max_backlog = 5000
net.ipv4.ip_local_port_range = 1024 65535

# Container storage optimizations
vm.swappiness = 10
vm.dirty_ratio = 15
vm.dirty_background_ratio = 5

# Enable IP forwarding for containers
net.ipv4.ip_forward = 1
EOF

# Apply sysctl settings
sysctl --system >/dev/null 2>&1 || true

# Remove policy-rc.d if present — leftover from chroot build, blocks service starts
rm -f /usr/sbin/policy-rc.d 2>/dev/null || true

# Ensure NTP time sync via chrony (more reliable than systemd-timesyncd)
if ! dpkg -l chrony >/dev/null 2>&1; then
    echo "🕐 Installing chrony for NTP time sync..."
    apt-get update -qq && apt-get install -y chrony 2>/dev/null || true
fi
systemctl enable chrony 2>/dev/null || true
systemctl start chrony 2>/dev/null || true
timedatectl set-ntp true 2>/dev/null || true

# Ensure swap exists — prevents OOM kills on memory-constrained nodes
TOTAL_MEM_KB=$(grep MemTotal /proc/meminfo | awk '{print $2}')
TOTAL_MEM_GB=$((TOTAL_MEM_KB / 1024 / 1024))
SWAP_SIZE_GB=$((TOTAL_MEM_GB > 8 ? 8 : TOTAL_MEM_GB))
if [ ! -f /swapfile ]; then
    echo "💾 Creating ${SWAP_SIZE_GB}G swap file..."
    fallocate -l ${SWAP_SIZE_GB}G /swapfile
    chmod 600 /swapfile
    mkswap /swapfile
    swapon /swapfile
    if ! grep -q '/swapfile' /etc/fstab; then
        echo '/swapfile none swap sw 0 0' >> /etc/fstab
    fi
    echo "✅ Swap created: ${SWAP_SIZE_GB}G"
else
    echo "✅ Swap file already exists"
    swapon /swapfile 2>/dev/null || true
fi

echo "✅ Debian optimization complete!"
