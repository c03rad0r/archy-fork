#!/bin/bash
#
# Archipelago Disk Installer
# Installs Archipelago Bitcoin Node OS to a disk
#

set -e

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║   🏝️  ARCHIPELAGO DISK INSTALLER                          ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "❌ Please run as root: sudo $0"
    exit 1
fi

# Install required tools if missing
echo "📦 Checking required tools..."
NEED_INSTALL=""
for tool in parted debootstrap; do
    if ! command -v $tool >/dev/null 2>&1; then
        NEED_INSTALL="$NEED_INSTALL $tool"
    fi
done

if [ -n "$NEED_INSTALL" ]; then
    echo "📥 Installing required tools:$NEED_INSTALL"
    apt-get update
    apt-get install -y $NEED_INSTALL dosfstools e2fsprogs
    echo ""
fi

# List available disks
echo "📋 Available disks:"
echo ""
lsblk -d -o NAME,SIZE,MODEL | grep -v loop | grep -v sr
echo ""

# Get target disk
read -p "Enter target disk (e.g., sda, nvme0n1): " TARGET_DISK
TARGET_DEVICE="/dev/$TARGET_DISK"

if [ ! -b "$TARGET_DEVICE" ]; then
    echo "❌ Device $TARGET_DEVICE not found"
    exit 1
fi

# Confirm
echo ""
echo "⚠️  WARNING: This will ERASE ALL DATA on $TARGET_DEVICE"
echo ""
read -p "Type 'yes' to continue: " CONFIRM

if [ "$CONFIRM" != "yes" ]; then
    echo "Aborted."
    exit 1
fi

echo ""
echo "🔧 Partitioning $TARGET_DEVICE..."

# Unmount any existing partitions
umount ${TARGET_DEVICE}* 2>/dev/null || true

# Create GPT partition table
parted -s "$TARGET_DEVICE" mklabel gpt

# Create partitions:
# 1. EFI System Partition (512MB)
# 2. Root partition (remaining space)
parted -s "$TARGET_DEVICE" mkpart primary fat32 1MiB 513MiB
parted -s "$TARGET_DEVICE" set 1 esp on
parted -s "$TARGET_DEVICE" mkpart primary ext4 513MiB 100%

# Wait for partitions to appear
sleep 2

# Determine partition names (handle both /dev/sdX and /dev/nvmeXnYpZ naming)
if [[ "$TARGET_DISK" == nvme* ]]; then
    EFI_PART="${TARGET_DEVICE}p1"
    ROOT_PART="${TARGET_DEVICE}p2"
else
    EFI_PART="${TARGET_DEVICE}1"
    ROOT_PART="${TARGET_DEVICE}2"
fi

echo "🗂️  Formatting partitions..."

# Format EFI partition
mkfs.vfat -F32 -n ARCHIPELAGO "$EFI_PART"

# Format root partition
mkfs.ext4 -L archipelago-root "$ROOT_PART"

echo "📦 Mounting partitions..."

# Create mount points
mkdir -p /mnt/archipelago
mount "$ROOT_PART" /mnt/archipelago
mkdir -p /mnt/archipelago/boot/efi
mount "$EFI_PART" /mnt/archipelago/boot/efi

echo "📋 Installing base system..."

# Install base system using debootstrap
if command -v debootstrap >/dev/null 2>&1; then
    debootstrap --arch=amd64 trixie /mnt/archipelago http://deb.debian.org/debian
else
    echo "❌ debootstrap not found. Installing..."
    apt-get update && apt-get install -y debootstrap
    debootstrap --arch=amd64 trixie /mnt/archipelago http://deb.debian.org/debian
fi

echo "⚙️  Configuring system..."

# Mount virtual filesystems for chroot
mount --bind /dev /mnt/archipelago/dev
mount --bind /dev/pts /mnt/archipelago/dev/pts
mount --bind /proc /mnt/archipelago/proc
mount --bind /sys /mnt/archipelago/sys
mount --bind /sys/firmware/efi/efivars /mnt/archipelago/sys/firmware/efi/efivars 2>/dev/null || true
mount --bind /run /mnt/archipelago/run

# Create fstab
cat > /mnt/archipelago/etc/fstab <<EOF
# Archipelago Bitcoin Node OS
UUID=$(blkid -s UUID -o value "$ROOT_PART")  /         ext4  errors=remount-ro  0  1
UUID=$(blkid -s UUID -o value "$EFI_PART")   /boot/efi vfat  umask=0077         0  1
EOF

# Set hostname
echo "archipelago" > /mnt/archipelago/etc/hostname

# Configure hosts
cat > /mnt/archipelago/etc/hosts <<EOF
127.0.0.1   localhost
127.0.1.1   archipelago

::1         localhost ip6-localhost ip6-loopback
EOF
chmod 644 /mnt/archipelago/etc/hosts

# Install bootloader and essential packages in chroot
echo "📦 Configuring package sources..."

# Create sources.list
cat > /mnt/archipelago/etc/apt/sources.list <<EOF
deb http://deb.debian.org/debian trixie main contrib non-free non-free-firmware
deb http://deb.debian.org/debian trixie-updates main contrib non-free non-free-firmware
deb http://security.debian.org/debian-security trixie-security main contrib non-free non-free-firmware
EOF

echo "📥 Updating package lists..."
chroot /mnt/archipelago apt-get update

echo "🔒 Applying Debian security updates..."
chroot /mnt/archipelago apt-get -y full-upgrade

echo "📦 Installing kernel and bootloader..."
chroot /mnt/archipelago apt-get install -y linux-image-amd64 grub-efi-amd64 grub-efi-amd64-signed shim-signed

echo "📦 Installing essential packages..."
chroot /mnt/archipelago apt-get install -y \
    sudo \
    network-manager \
    wpasupplicant \
    wireless-regdb \
    iw \
    rfkill \
    pciutils \
    usbutils \
    polkitd \
    openssh-server \
    curl \
    wget \
    htop \
    tmux \
    vim-tiny \
    nano \
    ca-certificates \
    chrony

echo "📦 Installing container tools..."
chroot /mnt/archipelago apt-get install -y podman catatonit || echo "⚠️  Podman/catatonit not available in base repos, will use containers.io later"

echo "🔧 Installing GRUB bootloader..."
# Need to run grub-install inside chroot with proper environment
chroot /mnt/archipelago /bin/bash -c "
    grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=archipelago --recheck 2>&1 || \
    grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=archipelago --removable 2>&1 || \
    echo '⚠️  GRUB install had issues, trying alternative...'
"
chroot /mnt/archipelago update-grub

echo "👤 Creating archipelago user..."
# Create user first, then add to groups that exist
chroot /mnt/archipelago useradd -m -s /bin/bash archipelago || echo "User may already exist"
chroot /mnt/archipelago usermod -aG sudo archipelago || true
chroot /mnt/archipelago usermod -aG podman archipelago 2>/dev/null || true

# Set password using chpasswd
echo "archipelago:archipelago" | chroot /mnt/archipelago chpasswd

echo "⚙️  Enabling services..."
chroot /mnt/archipelago systemctl enable NetworkManager || true
chroot /mnt/archipelago systemctl enable polkit || chroot /mnt/archipelago systemctl enable polkit.service || true
chroot /mnt/archipelago systemctl enable ssh || chroot /mnt/archipelago systemctl enable sshd || true
chroot /mnt/archipelago systemctl enable chrony || true

mkdir -p /mnt/archipelago/etc/polkit-1/rules.d
cat > /mnt/archipelago/etc/polkit-1/rules.d/49-archipelago-networkmanager.rules <<'EOF'
polkit.addRule(function(action, subject) {
    if (subject.user == "archipelago" && action.id.indexOf("org.freedesktop.NetworkManager.") == 0) {
        return polkit.Result.YES;
    }
});
EOF
chmod 644 /mnt/archipelago/etc/polkit-1/rules.d/49-archipelago-networkmanager.rules

# Remove policy-rc.d so services can start on first boot
rm -f /mnt/archipelago/usr/sbin/policy-rc.d

echo "💾 Creating swap file..."
TOTAL_MEM_KB=$(chroot /mnt/archipelago grep MemTotal /proc/meminfo 2>/dev/null | awk '{print $2}')
SWAP_GB=${TOTAL_MEM_KB:+$((TOTAL_MEM_KB / 1024 / 1024))}
SWAP_GB=${SWAP_GB:-4}
[ "$SWAP_GB" -gt 8 ] && SWAP_GB=8
[ "$SWAP_GB" -lt 2 ] && SWAP_GB=2
fallocate -l ${SWAP_GB}G /mnt/archipelago/swapfile 2>/dev/null || dd if=/dev/zero of=/mnt/archipelago/swapfile bs=1G count=$SWAP_GB status=progress
chmod 600 /mnt/archipelago/swapfile
chroot /mnt/archipelago mkswap /swapfile
echo '/swapfile none swap sw 0 0' >> /mnt/archipelago/etc/fstab
echo "✅ Created ${SWAP_GB}G swap"

echo "📁 Creating Archipelago directories..."
chroot /mnt/archipelago mkdir -p /var/lib/archipelago/{data,config,containers}
chroot /mnt/archipelago mkdir -p /etc/archipelago
chroot /mnt/archipelago chown -R archipelago:archipelago /var/lib/archipelago

echo "✅ Base system configured"

# Copy Archipelago files
echo "📋 Installing Archipelago components..."

BOOT_MEDIA=""
for dev in /run/live/medium /lib/live/mount/medium /cdrom; do
    if [ -d "$dev/archipelago" ]; then
        BOOT_MEDIA="$dev"
        break
    fi
done

if [ -n "$BOOT_MEDIA" ]; then
    echo "   Copying from: $BOOT_MEDIA/archipelago"
    
    # Copy entire archipelago directory
    mkdir -p /mnt/archipelago/opt/archipelago
    cp -r "$BOOT_MEDIA/archipelago/"* /mnt/archipelago/opt/archipelago/ 2>/dev/null || true
    
    # Install binaries
    if [ -d "$BOOT_MEDIA/archipelago/bin" ]; then
        cp "$BOOT_MEDIA/archipelago/bin/"* /mnt/archipelago/usr/local/bin/ 2>/dev/null || true
        chmod +x /mnt/archipelago/usr/local/bin/* 2>/dev/null || true
    fi
    
    # Install scripts
    if [ -d "$BOOT_MEDIA/archipelago/scripts" ]; then
        mkdir -p /mnt/archipelago/opt/archipelago/scripts
        cp "$BOOT_MEDIA/archipelago/scripts/"* /mnt/archipelago/opt/archipelago/scripts/ 2>/dev/null || true
        chmod +x /mnt/archipelago/opt/archipelago/scripts/*.sh 2>/dev/null || true
    fi
    
    # Install app manifests
    if [ -d "$BOOT_MEDIA/archipelago/apps" ]; then
        mkdir -p /mnt/archipelago/etc/archipelago
        cp -r "$BOOT_MEDIA/archipelago/apps" /mnt/archipelago/etc/archipelago/
    fi

    # Mesh radio udev rule: stable /dev/mesh-radio symlink + dialout perms
    # for known LoRa USB-serial chips (this installer never shipped it)
    for p in "$BOOT_MEDIA/99-mesh-radio.rules" "$BOOT_MEDIA/archipelago/configs/99-mesh-radio.rules"; do
        if [ -f "$p" ]; then
            mkdir -p /mnt/archipelago/etc/udev/rules.d
            cp "$p" /mnt/archipelago/etc/udev/rules.d/99-mesh-radio.rules
            chmod 644 /mnt/archipelago/etc/udev/rules.d/99-mesh-radio.rules
            break
        fi
    done
    
    # Create profile.d script for welcome message on login
    mkdir -p /mnt/archipelago/etc/profile.d
    cat > /mnt/archipelago/etc/profile.d/archipelago.sh <<'PROFILE_EOF'
#!/bin/bash
# Archipelago welcome message
if [ -t 0 ] && [ -z "$ARCHIPELAGO_WELCOMED" ]; then
    export ARCHIPELAGO_WELCOMED=1
    IP=$(hostname -I 2>/dev/null | awk '{print $1}')
    echo ""
    echo "  ╔═══════════════════════════════════════════════════════════╗"
    echo "  ║   🏝️  ARCHIPELAGO BITCOIN NODE OS                         ║"
    echo "  ╚═══════════════════════════════════════════════════════════╝"
    echo ""
    if [ -n "$IP" ]; then
        echo "  ┌─────────────────────────────────────────────────────────────┐"
        echo "  │  🌐 Web UI:  http://$IP:5678                            │"
        echo "  │  📡 SSH:     ssh archipelago@$IP                        │"
        echo "  └─────────────────────────────────────────────────────────────┘"
        echo ""
    fi
    echo "  Commands:"
    echo "    archipelago          - Start backend server"
    echo "    archipelago-menu     - Open setup menu"
    echo ""
fi
PROFILE_EOF
    chmod +x /mnt/archipelago/etc/profile.d/archipelago.sh
    
    # Create systemd service to auto-start archipelago backend
    mkdir -p /mnt/archipelago/etc/systemd/system
    cat > /mnt/archipelago/etc/systemd/system/archipelago.service <<'SERVICE_EOF'
[Unit]
Description=Archipelago Backend Server
After=network-online.target
Wants=network-online.target
# Never start before the data volume (if one exists) is mounted — without this
# the service races the mount on cold boot and "[FAILED]"-loops until it lands (B17)
RequiresMountsFor=/var/lib/archipelago

[Service]
Type=simple
User=archipelago
ExecStart=/usr/local/bin/archipelago
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
SERVICE_EOF
    
    # Enable the service
    chroot /mnt/archipelago systemctl enable archipelago.service 2>/dev/null || true
fi

echo "🧹 Cleaning up..."

# Unmount in reverse order
sync
umount /mnt/archipelago/run 2>/dev/null || true
umount /mnt/archipelago/sys/firmware/efi/efivars 2>/dev/null || true
umount /mnt/archipelago/sys 2>/dev/null || true
umount /mnt/archipelago/proc 2>/dev/null || true
umount /mnt/archipelago/dev/pts 2>/dev/null || true
umount /mnt/archipelago/dev 2>/dev/null || true
umount /mnt/archipelago/boot/efi 2>/dev/null || true
umount /mnt/archipelago 2>/dev/null || true

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║   ✅ INSTALLATION COMPLETE!                               ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "Remove the USB drive and reboot to start Archipelago."
echo ""
echo "Default login:"
echo "  Username: archipelago"
echo "  Password: archipelago"
echo ""
echo "⚠️  Please change the password after first login!"
echo ""
