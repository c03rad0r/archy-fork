#!/bin/bash
#
# archipelago main menu
# interactive setup for archipelago bitcoin node os
#

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Colors (256-color — works on Linux console with fbcon)
O=$'\033[38;5;208m'   # Orange
W=$'\033[1;37m'       # Bold white
D=$'\033[38;5;242m'   # Dim
C=$'\033[38;5;37m'    # Cyan
G=$'\033[38;5;35m'    # Green
R=$'\033[38;5;196m'   # Red
Y=$'\033[38;5;220m'   # Yellow
N=$'\033[0m'          # Reset

# Adaptive centering
get_width() { TW=$(tput cols 2>/dev/null || echo 60); [ "$TW" -gt 120 ] && TW=120; }
get_width
cc() { local s=$(echo -e "$1" | sed 's/\x1b\[[0-9;]*m//g'); local p=$(( (TW - ${#s}) / 2 )); [ $p -lt 0 ] && p=0; printf "%*s" "$p" ""; echo -e "$1"; }

# Box helpers (Claude-style rounded corners)
bw() { echo $((TW > 52 ? 52 : TW - 4)); }
btop() { local w=$(bw); local t="╭"; for i in $(seq 1 $((w-2))); do t="${t}─"; done; cc "${D}${t}╮${N}"; }
bbox() { local w=$(bw); local s=$(echo -e "$1" | sed 's/\x1b\[[0-9;]*m//g'); local pad=$((w - 2 - ${#s})); [ $pad -lt 0 ] && pad=0; local r=""; for i in $(seq 1 $pad); do r="${r} "; done; cc "${D}│${N} $1${r}${D}│${N}"; }
bbot() { local w=$(bw); local b="╰"; for i in $(seq 1 $((w-2))); do b="${b}─"; done; cc "${D}${b}╯${N}"; }
hrule() { local len=$((TW > 50 ? 50 : TW - 4)); local hr=""; for i in $(seq 1 $len); do hr="${hr}─"; done; cc "${D}${hr}${N}"; }

# Install required tools on first run (for live mode)
install_required_tools() {
    if [ -f /tmp/.archipelago-tools-installed ]; then
        return 0
    fi

    local NEED_TOOLS=0
    for tool in parted debootstrap mkfs.ext4 mkfs.vfat; do
        if ! command -v $tool >/dev/null 2>&1; then
            NEED_TOOLS=1
            break
        fi
    done

    if [ $NEED_TOOLS -eq 1 ]; then
        echo ""
        cc "${D}installing required tools...${N}"
        echo ""
        sudo apt-get update -qq 2>/dev/null
        sudo apt-get install -y parted debootstrap dosfstools e2fsprogs 2>/dev/null
        cc "${G}tools installed${N}"
        echo ""
        sleep 1
    fi

    touch /tmp/.archipelago-tools-installed
}

install_required_tools

show_banner() {
    get_width
    clear
    echo ""
    echo -e "  ${O}▄▀█ █▀▄ █▀▀ █ █ █ █▀█ █▀▀ █   ▄▀█ █▀▀ █▀█${N}"
    echo -e "  ${O}█▀█ █▀▄ █   █▀█ █ █▀▀ ██▀ █   █▀█ █ █ █ █${N}"
    echo -e "  ${O}▀ ▀ ▀ ▀ ▀▀▀ ▀ ▀ ▀ ▀   ▀▀▀ ▀▀▀ ▀ ▀ ▀▀▀ ▀▀▀${N}"
    echo -e "  ${D}bitcoin node os${N}"
    echo ""
}

show_status() {
    if [ -d /run/live ]; then
        cc "${R}live mode${N} ${D}(changes won't persist)${N}"
    else
        cc "${G}installed${N}"
    fi
    echo ""

    local podman_ok=0
    command -v podman >/dev/null 2>&1 && podman_ok=1

    if [ $podman_ok -eq 1 ] && podman ps 2>/dev/null | grep -q bitcoind; then
        local blocks=$(podman exec bitcoind bitcoin-cli getblockcount 2>/dev/null || echo "syncing")
        cc "${G}bitcoin${N}    ${D}running ($blocks blocks)${N}"
    elif [ $podman_ok -eq 1 ] && podman ps -a 2>/dev/null | grep -q bitcoind; then
        cc "${Y}bitcoin${N}    ${D}stopped${N}"
    fi

    if [ $podman_ok -eq 1 ] && podman ps 2>/dev/null | grep -q lnd; then
        cc "${G}lightning${N}  ${D}running${N}"
    elif [ $podman_ok -eq 1 ] && podman ps -a 2>/dev/null | grep -q lnd; then
        cc "${Y}lightning${N}  ${D}stopped${N}"
    fi

    echo ""
}

main_menu() {
    while true; do
        show_banner
        show_status

        # Connection info
        IP=$(hostname -I 2>/dev/null | awk '{print $1}')
        [ -z "$IP" ] && IP=$(ip -4 addr show | grep -oP '(?<=inet\s)\d+(\.\d+){3}' | grep -v '127.0.0.1' | head -1)

        if [ -n "$IP" ]; then
            if pgrep -f "archipelago" >/dev/null 2>&1; then
                cc "${C}web ui${N}  ${W}http://$IP${N}"
            else
                cc "${C}web ui${N}  ${D}http://$IP${N} ${Y}(not started)${N}"
            fi
            cc "${C}ssh${N}     ${D}archipelago@$IP${N}"
        else
            cc "${D}no network detected${N}"
        fi

        echo ""
        hrule
        echo ""
        cc "${D}r${N}  refresh status        ${D}w${N}  start web ui"
        echo ""
        cc "${O}1${N}  install to disk       ${O}5${N}  view logs"
        cc "${O}2${N}  setup bitcoin core    ${O}6${N}  network settings"
        cc "${O}3${N}  setup lightning       ${O}7${N}  system info"
        cc "${O}4${N}  setup btcpay server"
        echo ""
        cc "${D}q  quit${N}"
        echo ""

        local pad=$(( (TW - 18) / 2 ))
        [ $pad -lt 0 ] && pad=0
        printf "%*s" "$pad" ""
        read -p "select option: " choice

        case $choice in
            r|R)
                ;;
            w|W)
                echo ""
                if command -v archipelago >/dev/null 2>&1; then
                    if pgrep -f "archipelago" >/dev/null 2>&1; then
                        cc "${G}backend already running${N}"
                    else
                        cc "${D}starting backend on port 5678...${N}"
                        nohup archipelago >/tmp/archipelago.log 2>&1 &
                        sleep 2
                        if pgrep -f "archipelago" >/dev/null 2>&1; then
                            cc "${G}backend started${N}"
                        else
                            cc "${R}failed — see /tmp/archipelago.log${N}"
                        fi
                    fi
                    IP=$(hostname -I 2>/dev/null | awk '{print $1}')
                    echo ""
                    cc "open in browser:  ${W}http://$IP${N}"
                else
                    cc "${R}binary not found at /usr/local/bin/archipelago${N}"
                fi
                echo ""
                read -sp "  press enter to continue..."
                ;;
            1)
                if [ -f "$SCRIPT_DIR/install-to-disk.sh" ]; then
                    sudo bash "$SCRIPT_DIR/install-to-disk.sh"
                else
                    echo "  installer not found at: $SCRIPT_DIR"
                fi
                read -sp "  press enter to continue..."
                ;;
            2)
                if [ -f "$SCRIPT_DIR/setup-bitcoin.sh" ]; then
                    bash "$SCRIPT_DIR/setup-bitcoin.sh"
                else
                    echo "  bitcoin setup script not found."
                fi
                read -sp "  press enter to continue..."
                ;;
            3)
                if [ -f "$SCRIPT_DIR/setup-lnd.sh" ]; then
                    bash "$SCRIPT_DIR/setup-lnd.sh"
                else
                    echo "  lnd setup script not found."
                fi
                read -sp "  press enter to continue..."
                ;;
            4)
                setup_btcpay
                read -sp "  press enter to continue..."
                ;;
            5)
                view_logs
                ;;
            6)
                network_settings
                read -sp "  press enter to continue..."
                ;;
            7)
                system_info
                read -sp "  press enter to continue..."
                ;;
            q|Q)
                echo ""
                exit 0
                ;;
            *)
                sleep 0.5
                ;;
        esac
    done
}

setup_btcpay() {
    show_banner
    cc "${W}btcpay server setup${N}"
    cc "${D}self-hosted bitcoin payment processor${N}"
    echo ""

    if ! podman ps | grep -q bitcoind; then
        cc "${R}bitcoin core must be running first${N}"
        return
    fi

    local pad=$(( (TW - 30) / 2 ))
    [ $pad -lt 0 ] && pad=0
    printf "%*s" "$pad" ""
    read -p "setup btcpay server? [y/N]: " SETUP
    if [[ ! "$SETUP" =~ ^[Yy]$ ]]; then
        return
    fi

    echo ""
    cc "${D}pulling btcpay server image...${N}"
    podman pull "${BTCPAY_IMAGE}"
    mkdir -p ~/.btcpay

    echo ""
    cc "${D}full setup: https://docs.btcpayserver.org${N}"
    echo ""
}

view_logs() {
    show_banner
    cc "${W}view logs${N}"
    echo ""
    cc "${O}1${N}  ${D}bitcoin core${N}"
    cc "${O}2${N}  ${D}lnd${N}"
    cc "${O}3${N}  ${D}system journal${N}"
    cc "${D}b  back${N}"
    echo ""

    local pad=$(( (TW - 10) / 2 ))
    [ $pad -lt 0 ] && pad=0
    printf "%*s" "$pad" ""
    read -p "select: " choice

    case $choice in
        1)
            if podman ps -a | grep -q bitcoind; then
                podman logs -f --tail 50 bitcoind
            else
                cc "${D}bitcoin core not running${N}"
                read -sp "  press enter..."
            fi
            ;;
        2)
            if podman ps -a | grep -q lnd; then
                podman logs -f --tail 50 lnd
            else
                cc "${D}lnd not running${N}"
                read -sp "  press enter..."
            fi
            ;;
        3)
            journalctl -f
            ;;
    esac
}

network_settings() {
    show_banner
    cc "${W}network settings${N}"
    echo ""

    IP=$(hostname -I | awk '{print $1}')
    cc "${C}ip${N}  ${W}$IP${N}"
    echo ""

    cc "${D}interfaces:${N}"
    ip -br addr | grep -v "^lo" | while read line; do
        cc "  ${D}$line${N}"
    done
    echo ""

    cc "${D}service ports:${N}"
    cc "  ${D}8332  bitcoin rpc    9735  lightning p2p${N}"
    cc "  ${D}8333  bitcoin p2p   10009  lightning grpc${N}"
    echo ""
}

system_info() {
    show_banner
    cc "${W}system information${N}"
    echo ""

    cc "${C}host${N}     ${D}$(hostname)${N}"
    cc "${C}kernel${N}   ${D}$(uname -r)${N}"
    cc "${C}uptime${N}   ${D}$(uptime -p 2>/dev/null || echo 'unknown')${N}"
    echo ""

    local cpu=$(grep "model name" /proc/cpuinfo 2>/dev/null | head -1 | cut -d: -f2 | xargs)
    [ -n "$cpu" ] && cc "${C}cpu${N}      ${D}${cpu}${N}"

    local mem_total=$(free -h 2>/dev/null | grep Mem | awk '{print $2}')
    local mem_used=$(free -h 2>/dev/null | grep Mem | awk '{print $3}')
    [ -n "$mem_total" ] && cc "${C}memory${N}   ${D}${mem_used} / ${mem_total}${N}"
    echo ""

    cc "${D}disk:${N}"
    df -h / | tail -1 | awk '{printf "  root: %s / %s (%s used)\n", $3, $2, $5}' | while read line; do
        cc "${D}${line}${N}"
    done

    if [ -d ~/.bitcoin ]; then
        local btc_size=$(du -sh ~/.bitcoin 2>/dev/null | cut -f1)
        cc "  ${D}bitcoin: $btc_size${N}"
    fi
    echo ""

    if command -v podman >/dev/null 2>&1; then
        cc "${D}containers:${N}"
        podman ps --format "  {{.Names}}: {{.Status}}" 2>/dev/null | while read line; do
            cc "${D}${line}${N}"
        done
    fi
    echo ""
}

main_menu
