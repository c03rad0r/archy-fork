#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────
# Archipelago Install TUI Demo — 80s Hacker Edition
# Run: bash scripts/install-tui-demo.sh
# Ctrl+C to exit at any time.
# ─────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Colors — everything orange unless noted ─────────────────────
ORANGE=$'\033[38;5;208m'
ORANGE_DIM=$'\033[38;5;130m'
ORANGE_BRIGHT=$'\033[38;5;214m'
ORANGE_GLOW=$'\033[38;5;220m'
GREEN=$'\033[32m'
GREEN_DIM=$'\033[38;5;22m'
GREEN_BRIGHT=$'\033[38;5;46m'
WHITE=$'\033[1;37m'
DIM=$'\033[38;5;242m'
DIMMER=$'\033[38;5;238m'
DARK=$'\033[38;5;235m'
NC=$'\033[0m'
BOLD=$'\033[1m'

# ── Terminal setup ──────────────────────────────────────────────
TW=$(tput cols 2>/dev/null || echo 80)
TH=$(tput lines 2>/dev/null || echo 24)
[[ $TW -gt 100 ]] && TW=100

BW=56
[[ $BW -gt $((TW - 4)) ]] && BW=$((TW - 4))
INNER=$((BW - 2))

PAD=$(( (TW - BW) / 2 ))
[[ $PAD -lt 0 ]] && PAD=0
PADS=$(printf "%*s" "$PAD" "")

LOGO_W=43
LOGO_PAD=$(( (TW - LOGO_W) / 2 ))
[[ $LOGO_PAD -lt 0 ]] && LOGO_PAD=0
LOGO_PADS=$(printf "%*s" "$LOGO_PAD" "")

cleanup() {
    tput cnorm 2>/dev/null
    tput sgr0 2>/dev/null
    echo ""
}
trap cleanup EXIT INT TERM

# ── Primitives ──────────────────────────────────────────────────
hide_cursor() { tput civis 2>/dev/null || true; }
show_cursor() { tput cnorm 2>/dev/null || true; }
goto() { printf "\033[%d;%dH" "$1" "$2"; }
clear_line() { printf "\033[K"; }
p() { printf "%s%b\n" "$PADS" "$1"; }
pn() { printf "%s%b" "$PADS" "$1"; }

hrule() {
    local len=$((INNER < 50 ? INNER : 50))
    local hr=""
    for _ in $(seq 1 "$len"); do hr="${hr}*"; done
    p "${ORANGE_DIM}${hr}${NC}"
}

# ── Hacker glyphs ──────────────────────────────────────────────
HEXCHARS='0123456789abcdef'
SPIN_FRAMES='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'

rand_hex() {
    local len="${1:-8}" out=""
    for _ in $(seq 1 "$len"); do
        out="${out}${HEXCHARS:RANDOM % 16:1}"
    done
    echo -n "$out"
}

# ── Boot scan effect ───────────────────────────────────────────
boot_scan() {
    clear
    hide_cursor

    local messages=(
        "POST: memory check ............ 16384MB OK"
        "BIOS: AES-NI .................. detected"
        "UEFI: secure boot ............. disabled"
        "SATA: TOSHIBA MQ01ACF0 ........ 465.8G"
        "USB:  boot media .............. verified"
        "NET:  interface enp0s31f6 ...... link up"
        "INIT: loading archipelago ....."
    )

    for i in $(seq 1 8); do
        local addr data
        addr=$(rand_hex 8)
        data=$(rand_hex 32)
        goto $i 1
        printf "%s%b0x%s  %s%b" "$PADS" "$DARK" "$addr" "$data" "$NC"
        sleep 0.02
    done

    local row=3
    for msg in "${messages[@]}"; do
        goto $row 1; clear_line
        pn "${ORANGE_DIM}"
        local i=0
        while [[ $i -lt ${#msg} ]]; do
            printf "%s" "${msg:$i:1}"
            i=$((i + 1))
            if [[ "${msg:$i:1}" == "." ]]; then sleep 0.005; else sleep 0.012; fi
        done
        printf "%b\n" "$NC"
        row=$((row + 1))
        sleep 0.05
    done

    sleep 0.3
    for r in $(seq 1 $((row + 2))); do goto $r 1; clear_line; done
    sleep 0.2
}

# ── ASCII Logo: A R C H I P E L A G O ─────────────────────────
# 43 chars wide, 3 lines tall. Correct spelling!

LOGO_FRONT=(
    '▄▀█ █▀▄ █▀▀ █ █ █ █▀█ █▀▀ █   ▄▀█ █▀▀ █▀█'
    '█▀█ █▀▄ █   █▀█ █ █▀▀ ██▀ █   █▀█ █ █ █ █'
    '▀ ▀ ▀ ▀ ▀▀▀ ▀ ▀ ▀ ▀   ▀▀▀ ▀▀▀ ▀ ▀ ▀▀▀ ▀▀▀'
)

# 3D shadow: draw shadow (dark, offset +1,+2) then front on top
draw_logo_3d_at() {
    local row="$1" color="${2:-$ORANGE}"
    local front_col=$((LOGO_PAD + 1))
    local shadow_col=$((LOGO_PAD + 3))

    for li in 0 1 2; do
        goto $((row + li + 1)) "$shadow_col"
        printf "%b%s%b" "$DARK" "${LOGO_FRONT[$li]}" "$NC"
    done
    for li in 0 1 2; do
        goto $((row + li)) "$front_col"
        printf "%b%s%b" "$color" "${LOGO_FRONT[$li]}" "$NC"
    done
}

draw_logo_flat() {
    for line in "${LOGO_FRONT[@]}"; do
        printf "%s%b%s%b\n" "$LOGO_PADS" "${1:-$ORANGE}" "$line" "$NC"
    done
}

# Decrypt reveal with 3D shadow
logo_decrypt_reveal() {
    local row="$1"
    local iterations=7
    local scramble_chars='█▓▒░╳◆▀▄▌▐┃━╋╬╪'
    local front_col=$((LOGO_PAD + 1))
    local shadow_col=$((LOGO_PAD + 3))

    # Draw shadow layer first (static, dark)
    for li in 0 1 2; do
        goto $((row + li + 1)) "$shadow_col"
        printf "%b%s%b" "$DARK" "${LOGO_FRONT[$li]}" "$NC"
    done

    # Decrypt front layer
    for iter in $(seq 1 "$iterations"); do
        for li in 0 1 2; do
            local real="${LOGO_FRONT[$li]}"
            local out=""
            local len=${#real}
            local resolve=$(( iter * len / iterations ))
            local ci=0
            while [[ $ci -lt $len ]]; do
                local ch="${real:$ci:1}"
                if [[ $ci -lt $resolve ]]; then
                    out="${out}${ch}"
                elif [[ "$ch" == " " ]]; then
                    out="${out} "
                else
                    out="${out}${scramble_chars:RANDOM % ${#scramble_chars}:1}"
                fi
                ci=$((ci + 1))
            done

            local color="$DARK"
            case $iter in
                1) color="$DARK" ;; 2) color="$DIMMER" ;; 3) color="$DIM" ;;
                4) color="$ORANGE_DIM" ;; 5) color="$ORANGE_DIM" ;;
                6) color="$ORANGE" ;; 7) color="$ORANGE" ;;
            esac

            goto $((row + li)) "$front_col"
            printf "%b%s%b" "$color" "$out" "$NC"
        done
        sleep 0.07
    done

    # Glow pulse
    for color in "$ORANGE_BRIGHT" "$ORANGE_GLOW" "$ORANGE_BRIGHT" "$ORANGE"; do
        for li in 0 1 2; do
            goto $((row + li)) "$front_col"
            printf "\033[K%b%s%b" "$color" "${LOGO_FRONT[$li]}" "$NC"
        done
        sleep 0.05
    done
}

# Quick glow pulse on existing logo
logo_glow_pulse() {
    local row="$1" cycles="${2:-2}"
    local col=$((LOGO_PAD + 1))
    for _ in $(seq 1 "$cycles"); do
        for color in "$ORANGE" "$ORANGE_BRIGHT" "$ORANGE_GLOW" "$ORANGE_BRIGHT" "$ORANGE"; do
            for li in 0 1 2; do
                goto $((row + li)) "$col"
                printf "\033[K%b%s%b" "$color" "${LOGO_FRONT[$li]}" "$NC"
            done
            sleep 0.04
        done
    done
}

# Celebration: logo strobes with color party
logo_celebrate() {
    local row="$1"
    local col=$((LOGO_PAD + 1))
    local party_colors=("$ORANGE" "$ORANGE_GLOW" "$WHITE" "$ORANGE_BRIGHT" "$GREEN_BRIGHT" "$ORANGE_GLOW" "$ORANGE")
    for color in "${party_colors[@]}"; do
        for li in 0 1 2; do
            goto $((row + li)) "$col"
            printf "\033[K%b%s%b" "$color" "${LOGO_FRONT[$li]}" "$NC"
        done
        sleep 0.06
    done
}

# Screen wipe transition
screen_transition() {
    for r in $(seq 1 "$TH"); do
        goto "$r" 1
        printf "%b%*s%b" "$ORANGE_DIM" "$TW" "" "$NC"
        sleep 0.005
    done
    sleep 0.05
    clear
}

# CRT power-on scan line
crt_on() {
    hide_cursor; clear
    for r in $(seq 1 "$TH"); do
        goto "$r" 1
        printf "%b%*s%b" "$ORANGE_DIM" "$TW" "" "$NC"
        if [[ $r -gt 1 ]]; then goto $((r - 1)) 1; clear_line; fi
        sleep 0.008
    done
    goto "$TH" 1; clear_line
    sleep 0.1; clear
}

# ── Phase data ─────────────────────────────────────────────────
PHASE_NAMES=(
    "Checking tools"
    "Detecting disks"
    "Creating partitions"
    "Formatting partitions"
    "Installing base system"
    "Encrypting data partition"
    "Installing bootloader"
)
PHASE_DETAILS=(
    "parted, mkfs, cryptsetup"
    "/dev/sda (465.8G) — TOSHIBA MQ01ACF0"
    "BIOS boot + EFI + root + data"
    "FAT32, ext4, LUKS2"
    "debootstrap → Debian 13 minimal"
    "AES-256-XTS (AES-NI detected)"
    "GRUB: BIOS + UEFI hybrid"
)
PHASE_DURATIONS=(8 6 12 10 40 15 10)

# ── Header (logo + right-aligned subtitle) ────────────────────
# "bitcoin node os" right-aligned to match logo's right edge
SUBTITLE_PAD=$(printf "%*s" $((LOGO_PAD + LOGO_W - 15)) "")
HEADER_LINES=6  # 3 logo + shadow row + subtitle + blank

draw_header() {
    draw_logo_flat
    printf "%s%b%s%b\n" "$SUBTITLE_PAD" "$ORANGE_DIM" "bitcoin node os" "$NC"
    p ""
}

draw_header_3d() {
    local start_row="$1"
    local logo_row=$((start_row))
    # Blank for logo + shadow + subtitle
    goto "$start_row" 1
    for _ in $(seq 1 5); do p ""; done
    printf "%s%b%s%b\n" "$SUBTITLE_PAD" "$ORANGE_DIM" "bitcoin node os" "$NC"

    sleep 0.15
    logo_decrypt_reveal "$logo_row"
}

# ── Phase drawing (all orange) ─────────────────────────────────
draw_phase_pending() {
    p "  ${DIMMER}[${1}/7]  ${PHASE_NAMES[$(($1-1))]}${NC}"
}

draw_phase_running() {
    p "  ${ORANGE}[${1}/7]  ${PHASE_NAMES[$(($1-1))]}${NC} ${ORANGE_BRIGHT}◐${NC}"
}

draw_phase_done() {
    local name="${PHASE_NAMES[$(($1-1))]}"
    local dot_count=$((34 - ${#name}))
    [[ $dot_count -lt 2 ]] && dot_count=2
    local dots=""
    for _ in $(seq 1 "$dot_count"); do dots="${dots}."; done
    p "  ${ORANGE_DIM}[${1}/7]  ${name} ${DARK}${dots}${NC} ${ORANGE_BRIGHT}✓${NC}"
}

draw_phase_done_compact() {
    p "  ${ORANGE_DIM}[${1}/7]  ${PHASE_NAMES[$(($1-1))]}${NC}  ${ORANGE_BRIGHT}✓${NC}"
}

simulate_work() {
    local ticks=$1 row=$2 phase=$3 fi=0
    local col=$((PAD + 2 + 10 + ${#PHASE_NAMES[$((phase-1))]} + 2))
    for _ in $(seq 1 "$ticks"); do
        goto "$row" "$col"
        printf "%b%s%b" "$ORANGE" "${SPIN_FRAMES:fi%10:1}" "$NC"
        fi=$((fi + 1))
        sleep 0.1
    done
}

simulate_work_with_bar() {
    local ticks=$1 row=$2 phase=$3 bar_row=$4 fi=0
    local col=$((PAD + 2 + 10 + ${#PHASE_NAMES[$((phase-1))]} + 2))
    local bar_width=36

    # Bouncing ₿ — DVD screensaver style
    local b_row=$((bar_row + 3)) b_col=$((PAD + 4))
    local b_dr=1 b_dc=1
    local b_min_row=$((bar_row + 3))
    local b_max_row=$((TH - 2))
    [[ $b_max_row -lt $((b_min_row + 3)) ]] && b_max_row=$((b_min_row + 3))
    local b_min_col=$((PAD + 2))
    local b_max_col=$((PAD + BW - 2))
    local b_colors=("$ORANGE" "$ORANGE_BRIGHT" "$ORANGE_GLOW" "$ORANGE_DIM" "$WHITE")
    local b_ci=0
    local b_prev_row=$b_row b_prev_col=$b_col

    for t in $(seq 1 "$ticks"); do
        goto "$row" "$col"
        printf "%b%s%b" "$ORANGE" "${SPIN_FRAMES:fi%10:1}" "$NC"
        fi=$((fi + 1))

        local pct=$(( t * 100 / ticks ))
        local filled=$(( pct * bar_width / 100 ))
        local empty=$(( bar_width - filled ))
        local bar_f="" bar_e=""
        for _ in $(seq 1 "$filled" 2>/dev/null); do bar_f="${bar_f}█"; done
        for _ in $(seq 1 "$empty" 2>/dev/null); do bar_e="${bar_e}░"; done

        goto "$bar_row" 1; clear_line
        p "  ${ORANGE}${bar_f}${DARK}${bar_e}${NC} ${ORANGE_DIM}${pct}%%${NC}"

        goto "$b_prev_row" "$b_prev_col"; printf " "

        b_row=$((b_row + b_dr)); b_col=$((b_col + b_dc))

        if [[ $b_row -ge $b_max_row ]] || [[ $b_row -le $b_min_row ]]; then
            b_dr=$(( -b_dr )); b_row=$((b_row + b_dr))
            b_ci=$(( (b_ci + 1) % ${#b_colors[@]} ))
        fi
        if [[ $b_col -ge $b_max_col ]] || [[ $b_col -le $b_min_col ]]; then
            b_dc=$(( -b_dc )); b_col=$((b_col + b_dc))
            b_ci=$(( (b_ci + 1) % ${#b_colors[@]} ))
        fi

        goto "$b_row" "$b_col"
        printf "%b₿%b" "${b_colors[$b_ci]}" "$NC"
        b_prev_row=$b_row; b_prev_col=$b_col

        sleep 0.1
    done

    goto "$b_prev_row" "$b_prev_col"; printf " "
    goto "$bar_row" 1; clear_line
    for r in $(seq "$b_min_row" "$b_max_row"); do goto "$r" 1; clear_line; done
}

typewrite() {
    local text="$1" delay="${2:-0.025}"
    pn "${ORANGE}"
    local i=0
    while [[ $i -lt ${#text} ]]; do
        printf "%s" "${text:$i:1}"
        i=$((i + 1))
        sleep "$delay"
    done
    printf "%b\n" "$NC"
}

# ── SCREEN 1: Welcome ─────────────────────────────────────────
screen_welcome() {
    crt_on
    boot_scan

    clear
    hide_cursor

    local start_row=$(( (TH - 16) / 2 ))
    [[ $start_row -lt 2 ]] && start_row=2

    draw_header_3d "$start_row"

    local prompt_row=$((start_row + HEADER_LINES + 2))
    goto "$prompt_row" 1
    local prompt_text="  Press Enter to install  │  Ctrl+C for shell"
    pn "${ORANGE_DIM}"
    local i=0
    while [[ $i -lt ${#prompt_text} ]]; do
        printf "%s" "${prompt_text:$i:1}"
        i=$((i + 1))
        sleep 0.018
    done
    printf "%b" "$NC"

    # Logo breathing while cursor blinks
    local logo_at=$((start_row + 1))
    local front_col=$((LOGO_PAD + 1))
    for _ in $(seq 1 3); do
        goto "$prompt_row" $((PAD + ${#prompt_text} + 2))
        printf "%b▌%b" "$ORANGE" "$NC"
        for li in 0 1 2; do
            goto $((logo_at + li)) "$front_col"
            printf "\033[K%b%s%b" "$ORANGE_BRIGHT" "${LOGO_FRONT[$li]}" "$NC"
        done
        sleep 0.4
        goto "$prompt_row" $((PAD + ${#prompt_text} + 2))
        printf " "
        for li in 0 1 2; do
            goto $((logo_at + li)) "$front_col"
            printf "\033[K%b%s%b" "$ORANGE" "${LOGO_FRONT[$li]}" "$NC"
        done
        sleep 0.4
    done
}

# ── SCREEN 2: Disk Detection ──────────────────────────────────
screen_disk_detect() {
    screen_transition
    hide_cursor

    local row=2
    goto $row 1
    draw_header
    row=$((row + HEADER_LINES))

    goto $row 1
    draw_phase_running 1
    simulate_work 8 $row 1
    goto $row 1; draw_phase_done 1
    row=$((row + 2))

    goto $row 1
    draw_phase_running 2
    simulate_work 6 $row 2
    goto $row 1; draw_phase_done 2
    row=$((row + 2))

    goto $row 1
    typewrite "  Found: /dev/sda (465.8G) — TOSHIBA MQ01ACF0" 0.02
    row=$((row + 2))

    goto $row 1; hrule; row=$((row + 2))

    goto $row 1
    p "${ORANGE}  ⚠  All data on /dev/sda will be erased.${NC}"
    row=$((row + 2))

    goto $row 1
    p "${ORANGE_DIM}  Press Enter to install  │  Ctrl+C to cancel${NC}"

    for _ in $(seq 1 4); do
        goto $row $((PAD + 49))
        printf "%b▌%b" "$ORANGE" "$NC"
        sleep 0.4
        goto $row $((PAD + 49))
        printf " "
        sleep 0.4
    done
}

# ── SCREEN 3: Installation ────────────────────────────────────
screen_install() {
    screen_transition
    hide_cursor

    local row=2
    goto $row 1
    draw_header
    local phase_start=$((row + HEADER_LINES))

    for i in $(seq 0 6); do
        goto $((phase_start + i * 2)) 1
        draw_phase_pending $((i + 1))
    done

    local bar_row=$((phase_start + 14 + 1))
    goto $bar_row 1; hrule
    local status_row=$((bar_row + 2))

    for i in $(seq 0 6); do
        local pr=$((phase_start + i * 2))
        local pnum=$((i + 1))
        local dur=${PHASE_DURATIONS[$i]}

        goto $pr 1; clear_line
        draw_phase_running $pnum

        goto $status_row 1; clear_line
        p "  ${ORANGE_DIM}${PHASE_DETAILS[$i]}${NC}"

        if [[ $dur -gt 15 ]]; then
            simulate_work_with_bar "$dur" "$pr" "$pnum" "$((status_row - 1))"
        else
            simulate_work "$dur" "$pr" "$pnum"
        fi

        goto $pr 1; clear_line
        draw_phase_done $pnum
    done

    goto $((bar_row + 1)) 1; clear_line
    goto $status_row 1; clear_line

    logo_glow_pulse 3 2
    sleep 0.3
}

# ── SCREEN 4: Complete ─────────────────────────────────────────
screen_complete() {
    screen_transition
    hide_cursor

    local row=2
    goto $row 1
    draw_header
    logo_celebrate 3
    row=$((row + HEADER_LINES))

    for i in $(seq 1 7); do
        goto $row 1
        draw_phase_done_compact $i
        row=$((row + 1))
    done

    row=$((row + 1))
    goto $row 1; hrule; row=$((row + 2))

    # Success flash
    for color in "$ORANGE_DIM" "$ORANGE" "$ORANGE_BRIGHT" "$ORANGE"; do
        goto $row 1; clear_line
        p "  ${color}✓ Installation Complete${NC}"
        sleep 0.06
    done
    row=$((row + 2))

    goto $row 1
    typewrite "  After reboot, access from any device:" 0.02
    row=$((row + 2))

    # URL in orange
    goto $row 1
    p "  ${ORANGE}http://192.0.2.11${NC}"
    row=$((row + 2))

    # Credentials — white, NOT orange (user request)
    goto $row 1
    p "  ${WHITE}SSH       ssh archipelago@192.0.2.11${NC}"
    row=$((row + 1))
    goto $row 1
    p "  ${WHITE}Password  archipelago${NC}"
    row=$((row + 1))
    goto $row 1
    p "  ${WHITE}Web Login create your password on first visit${NC}"
    row=$((row + 2))

    goto $row 1; hrule; row=$((row + 2))

    for _ in $(seq 1 5); do
        goto $row 1; clear_line
        p "${ORANGE}${BOLD}  >>> REMOVE THE USB DRIVE NOW <<<${NC}"
        sleep 0.5
        goto $row 1; clear_line
        p "${ORANGE_DIM}  >>> REMOVE THE USB DRIVE NOW <<<${NC}"
        sleep 0.5
    done
    goto $row 1; clear_line
    p "${ORANGE}${BOLD}  >>> REMOVE THE USB DRIVE NOW <<<${NC}"

    goto $((row + 2)) 1
    p "${ORANGE_DIM}  Press Enter to reboot${NC}"
    sleep 3
}

# ── Main ───────────────────────────────────────────────────────
main() {
    echo ""
    echo "  ${ORANGE}▌${NC} ${ORANGE}Archipelago Install TUI Demo${NC}"
    echo "  ${ORANGE_DIM}  Each screen auto-advances. Ctrl+C to exit.${NC}"
    echo ""
    sleep 2

    screen_welcome
    sleep 0.3
    screen_disk_detect
    sleep 0.3
    screen_install
    sleep 0.3
    screen_complete

    show_cursor
    echo ""
    p "${ORANGE_DIM}** Demo complete **${NC}"
    echo ""
}

main "$@"
