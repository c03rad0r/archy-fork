#!/bin/bash
# ─────────────────────────────────────────────────────────────────
# Archipelago Install TUI Library
# Sourced by auto-install.sh to add animations to the installer.
# If not sourced, installer falls back to plain text output.
# ─────────────────────────────────────────────────────────────────
# Revert: remove the "source" line in auto-install.sh and
#         this file. Installer reverts to plain step/ok/fail output.
# ─────────────────────────────────────────────────────────────────

[ -n "${_INSTALL_TUI_LOADED:-}" ] && return 0
_INSTALL_TUI_LOADED=1

# ── Extra colors (plain installer only has basic set) ──────────
ORANGE_GLOW=$'\033[38;5;220m'
GREEN_DIM=$'\033[38;5;22m'
GREEN_BRIGHT=$'\033[38;5;46m'
DARK=$'\033[38;5;235m'

# ── Terminal setup ─────────────────────────────────────────────
TW=$(tput cols 2>/dev/null || echo 80)
TH=$(tput lines 2>/dev/null || echo 24)
[[ $TW -gt 100 ]] && TW=100

LOGO_W=43
LOGO_PAD=$(( (TW - LOGO_W) / 2 ))
[[ $LOGO_PAD -lt 0 ]] && LOGO_PAD=0
LOGO_PADS=$(printf "%*s" "$LOGO_PAD" "")

# ── Primitives ─────────────────────────────────────────────────
hide_cursor() { tput civis 2>/dev/null || true; }
show_cursor() { tput cnorm 2>/dev/null || true; }
goto() { printf "\033[%d;%dH" "$1" "$2"; }
clear_line() { printf "\033[K"; }

# ── Hacker glyphs ─────────────────────────────────────────────
HEXCHARS='0123456789abcdef'
SPIN_FRAMES='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'

tui_rand_hex() {
    local len="${1:-8}" out=""
    for _ in $(seq 1 "$len"); do
        out="${out}${HEXCHARS:RANDOM % 16:1}"
    done
    echo -n "$out"
}

# ── ASCII Logo ─────────────────────────────────────────────────
LOGO_FRONT=(
    '▄▀█ █▀▄ █▀▀ █ █ █ █▀█ █▀▀ █   ▄▀█ █▀▀ █▀█'
    '█▀█ █▀▄ █   █▀█ █ █▀▀ ██▀ █   █▀█ █ █ █ █'
    '▀ ▀ ▀ ▀ ▀▀▀ ▀ ▀ ▀ ▀   ▀▀▀ ▀▀▀ ▀ ▀ ▀▀▀ ▀▀▀'
)

# ── Boot scan effect ──────────────────────────────────────────
tui_boot_scan() {
    clear
    hide_cursor

    local messages=(
        "POST: memory check ............ OK"
        "BIOS: AES-NI .................. detected"
        "UEFI: secure boot ............. disabled"
        "USB:  boot media .............. verified"
        "NET:  interface ............... link up"
        "INIT: loading archipelago ....."
    )

    for i in $(seq 1 6); do
        local addr data
        addr=$(tui_rand_hex 8)
        data=$(tui_rand_hex 32)
        goto $i 1
        printf "%s%b0x%s  %s%b" "$PADS" "$DARK" "$addr" "$data" "$NC"
        sleep 0.02
    done

    local row=3
    for msg in "${messages[@]}"; do
        goto $row 1; clear_line
        printf "%s%b" "$PADS" "$ORANGE_DIM"
        local i=0
        while [[ $i -lt ${#msg} ]]; do
            printf "%s" "${msg:$i:1}"
            i=$((i + 1))
            sleep 0.01
        done
        printf "%b\n" "$NC"
        row=$((row + 1))
        sleep 0.04
    done

    sleep 0.3
    for r in $(seq 1 $((row + 2))); do goto $r 1; clear_line; done
    sleep 0.2
}

# ── Logo decrypt reveal ───────────────────────────────────────
tui_logo_decrypt_reveal() {
    local row="${1:-3}"
    local iterations=6
    local scramble_chars='█▓▒░╳◆▀▄▌▐┃━╋╬╪'
    local front_col=$((LOGO_PAD + 1))
    local shadow_col=$((LOGO_PAD + 3))

    # Draw shadow layer (static, dark)
    for li in 0 1 2; do
        goto $((row + li + 1)) "$shadow_col"
        printf "%b%s%b" "$DARK" "${LOGO_FRONT[$li]}" "$NC"
    done

    # Decrypt front layer
    for iter in $(seq 1 "$iterations"); do
        local color
        case $iter in
            1|2) color="$ORANGE_DIM" ;;
            3|4) color="$ORANGE" ;;
            *)   color="$ORANGE_BRIGHT" ;;
        esac

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

# ── Logo celebration strobe ───────────────────────────────────
tui_logo_celebrate() {
    local row="${1:-3}"
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

# ── CRT power-on scan line ────────────────────────────────────
tui_crt_on() {
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

# ── Progress bar with bouncing Bitcoin symbol ─────────────────
# Usage: tui_progress_bar <pid> <message>
# Runs until process $pid exits. Shows progress bar + bouncing ₿
tui_progress_bar() {
    local pid=$1 msg=$2
    local frames='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
    local bar_width=36
    local fi=0 tick=0
    local est_ticks=200  # rough estimate for progress

    # Bouncing ₿ setup
    local term_h=$TH
    local b_row=12 b_col=10
    local b_dr=1 b_dc=1
    local b_min_row=10 b_max_row=$((term_h - 2))
    [[ $b_max_row -lt 14 ]] && b_max_row=14
    local b_min_col=4 b_max_col=$((TW - 4))
    local b_colors=("$ORANGE" "$ORANGE_BRIGHT" "$ORANGE_GLOW" "$ORANGE_DIM" "$WHITE")
    local b_ci=0
    local b_prev_row=$b_row b_prev_col=$b_col

    while kill -0 "$pid" 2>/dev/null; do
        # Spinner
        printf "\r%s  %b%s %s%b" "$PADS" "$ORANGE" "${frames:fi%10:1}" "$msg" "$NC"
        fi=$((fi + 1))
        tick=$((tick + 1))

        # Progress bar (estimate-based since we don't know real progress)
        local pct=$(( tick * 95 / est_ticks ))
        [[ $pct -gt 95 ]] && pct=95
        local filled=$(( pct * bar_width / 100 ))
        local empty=$(( bar_width - filled ))
        local bar_f="" bar_e=""
        for _ in $(seq 1 "$filled" 2>/dev/null); do bar_f="${bar_f}█"; done
        for _ in $(seq 1 "$empty" 2>/dev/null); do bar_e="${bar_e}░"; done

        # Draw bar below spinner
        goto 22 1; clear_line
        printf "%s  %b%s%b%s%b %b%d%%%b" "$PADS" "$ORANGE" "$bar_f" "$DARK" "$bar_e" "$NC" "$ORANGE_DIM" "$pct" "$NC"

        # Bouncing ₿
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

    # Clean up
    goto "$b_prev_row" "$b_prev_col"; printf " "
    goto 22 1; clear_line
    # Clear bouncing area
    for r in $(seq "$b_min_row" "$b_max_row"); do goto "$r" 1; clear_line; done
    printf "\r%s  %b✓ %s%b\n" "$PADS" "$ORANGE_BRIGHT" "$msg" "$NC"
}

# ── Flashing completion message ───────────────────────────────
tui_flash_remove_usb() {
    local row="${1:-20}"
    for _ in $(seq 1 5); do
        goto "$row" 1; clear_line
        printf "%s%b%b  >>> REMOVE THE USB DRIVE NOW <<<%b\n" "$PADS" "$ORANGE" "$BOLD" "$NC"
        sleep 0.5
        goto "$row" 1; clear_line
        printf "%s%b  >>> REMOVE THE USB DRIVE NOW <<<%b\n" "$PADS" "$ORANGE_DIM" "$NC"
        sleep 0.5
    done
    goto "$row" 1; clear_line
    printf "%s%b%b  >>> REMOVE THE USB DRIVE NOW <<<%b\n" "$PADS" "$ORANGE" "$BOLD" "$NC"
}

# ── Override: replace plain spinner with progress bar ─────────
# Call this to replace the default spinner() with the animated version.
# Only for long operations (base system install, bootloader).
tui_enable_progress_spinner() {
    spinner() {
        tui_progress_bar "$1" "$2"
    }
}

# ── Welcome screen (replaces plain logo) ─────────────────────
tui_welcome() {
    tui_crt_on
    tui_boot_scan
    clear
    hide_cursor
    tui_logo_decrypt_reveal 3
    # Subtitle
    local sub_pad=$(printf "%*s" $((LOGO_PAD + LOGO_W - 15)) "")
    goto 7 1
    printf "%s%b%s%b\n" "$sub_pad" "$ORANGE_DIM" "bitcoin node os" "$NC"
    echo ""
}

# ── Completion screen (replaces plain message) ────────────────
tui_complete() {
    tui_logo_celebrate 3
    show_cursor
}

# Mark as loaded — installer checks this
TUI_AVAILABLE=1
