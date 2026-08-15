#!/bin/bash
# Console welcome banner (/etc/profile.d/archipelago.sh).
#
# CANONICAL COPY. Two consumers keep nodes in lockstep with it:
#   - bootstrap::run_welcome_banner_sync embeds it (include_str!) and
#     installs it at startup on ISO-installed nodes, so banner fixes
#     actually reach the deployed fleet via OTA;
#   - the ISO builder (image-recipe/_archived/build-auto-installer-iso.sh,
#     PROFILE heredoc) inlines the same content for fresh installs.
#
# Ensure /sbin and /usr/sbin are in PATH (needed for reboot, shutdown, etc.)
case ":$PATH:" in
    *:/sbin:*) ;; *) export PATH="$PATH:/sbin:/usr/sbin" ;;
esac
if [ -t 0 ] && [ -z "$ARCHIPELAGO_WELCOMED" ]; then
    export ARCHIPELAGO_WELCOMED=1

    # Wait for network (DHCP may not be ready yet on first boot).
    # The address shown must be one a LAN user can actually reach: the
    # default route's source address. `hostname -I` lists addresses in
    # interface order, so a node with WireGuard up advertised 10.44.0.1 —
    # its own tunnel address, present on EVERY node — as its "web ui",
    # which is unreachable off-tunnel and actively misleading after a
    # move to a new network (framework-pt, 2026-08-15).
    IP=""
    for i in 1 2 3 4 5; do
        IP=$(ip -4 route get 1.1.1.1 2>/dev/null | sed -n 's/.*src \([0-9.]*\).*/\1/p' | head -n1)
        # Offline LAN (no default route): first address that is not a
        # tunnel (10.44/16 WireGuard), CGNAT (100.64/10 Tailscale), or
        # loopback one.
        [ -n "$IP" ] || IP=$(hostname -I 2>/dev/null | tr ' ' '\n' \
            | grep -vE '^(10\.44\.|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.|127\.)' | head -n1)
        [ -n "$IP" ] || IP=$(hostname -I 2>/dev/null | awk '{print $1}')
        [ -n "$IP" ] && break
        sleep 2
    done

    O='\033[38;5;208m'
    OD='\033[38;5;130m'
    W='\033[1;37m'
    N='\033[0m'

    # The logo uses UTF-8 block-drawing glyphs. Switching back from the kiosk
    # (Xorg on vt1) can leave the console VT out of UTF-8 mode, which renders
    # them as garbage bytes ("sometimes corrupt"). ESC % G forces the VT into
    # UTF-8 mode so the logo always draws correctly.
    printf '\033%%G' 2>/dev/null || true
    clear
    echo -e "  ${O}▄▀█ █▀▄ █▀▀ █ █ █ █▀█ █▀▀ █   ▄▀█ █▀▀ █▀█${N}"
    echo -e "  ${O}█▀█ █▀▄ █   █▀█ █ █▀▀ ██▀ █   █▀█ █ █ █ █${N}"
    echo -e "  ${O}▀ ▀ ▀ ▀ ▀▀▀ ▀ ▀ ▀ ▀   ▀▀▀ ▀▀▀ ▀ ▀ ▀▀▀ ▀▀▀${N}"
    echo -e "                              ${OD}bitcoin node os${N}"
    if [ -n "$IP" ]; then
        echo -e "  ${W}web ui    http://$IP${N}"
        # The mDNS name survives any DHCP change — it is the address to
        # give people for a headless box that moves between networks.
        if systemctl is-active avahi-daemon >/dev/null 2>&1; then
            echo -e "  ${W}          http://$(hostname).local${N}"
        fi
        echo -e "  ${W}ssh       archipelago@$IP${N}"
        echo -e "  ${W}password  archipelago (SSH)${N}"
        echo -e "  ${OD}web ui asks you to create a password on first visit${N}"
    else
        echo -e "  ${OD}Waiting for network...${N}"
    fi
    if [ -b /dev/mapper/archipelago-data ] || [ -b /dev/mapper/archipelago_crypt ]; then
        echo -e "  ${OD}storage   LUKS2 encrypted${N}"
    fi
    # The kiosk's Xorg runs on vt1 (see archipelago-kiosk-launcher: "Xorg :0
    # vt1"), so Ctrl+Alt+F1 IS the kiosk and a terminal is on another VT (F2,
    # where systemd auto-spawns a getty). The old hints had these backwards —
    # they sent you to an empty black VT7 and there was no way back to the kiosk.
    if systemctl is-active archipelago-kiosk.service >/dev/null 2>&1; then
        echo -e "  ${OD}display   Kiosk active (Ctrl+Alt+F2 for terminal)${N}"
    else
        echo -e "  ${OD}display   Console (Ctrl+Alt+F1 for kiosk)${N}"
    fi
    echo ""
fi
