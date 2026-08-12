#!/bin/bash
# Archipelago audio router — keep audio flowing out of the display the user is
# actually looking at.
#
# The kiosk's Chromium plays through PipeWire-Pulse, but WirePlumber's stock
# profile priorities rank the laptop's analog output above HDMI, so a node
# driving a TV plays video sound out of its own tiny speakers (or nowhere).
# ALSA jack detection (ELD) tells us when an HDMI/DP sink has a listening
# monitor; this daemon polls that via the card's profile availability and:
#
#   - switches the card to the best *available* HDMI stereo profile (the
#     "+input:analog-stereo" combined variant when offered, so the mic keeps
#     working), and back to analog when HDMI is unplugged;
#   - keeps the default sink pointed at the routed output and migrates any
#     live streams so playback follows a hot-plug without a page reload;
#   - unmutes the routed sink (HDMI additionally forced to 100% — the TV owns
#     the real volume control; analog volume is left where the user set it).
#
# Runs as the archipelago user (systemd system unit with User=archipelago),
# talks only to the user-session PipeWire; polling is a few pactl calls every
# 5s — negligible. Surround profiles are deliberately ignored: stereo is the
# lowest-common-denominator every TV decodes.

#   - re-modesets an external output once when it is connected but no ELD
#     reports a monitor: the kiosk's boot-time Xorg modeset can beat the
#     i915→HDA audio-component bind, the ELD notify is lost, and every HDMI
#     profile stays "available: no" forever (no sound, no error). One
#     off/on cycle re-delivers the ELD (verified on Framework PT / LG TV).

RUNTIME_DIR="/run/user/$(id -u)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$RUNTIME_DIR}"
export DISPLAY="${DISPLAY:-:0}"

# The user manager socket-activates pipewire, but after a live package install
# (bootstrap self-heal) nothing has poked it yet — start best-effort.
systemctl --user start pipewire.socket pipewire-pulse.socket 2>/dev/null || true
systemctl --user start wireplumber.service 2>/dev/null || true

LAST_SINK=""
NUDGED_OUTPUTS=""

# Re-deliver a lost ELD by cycling the connected external output once. Guarded:
# only when X answers, only when NO ELD anywhere reports a monitor, and only
# once per connector while it stays connected (a display with no audio support
# never produces an ELD — without the flag we would blank it every pass).
eld_nudge_once() {
    # Any ELD already valid → audio path is live; reset the nudge memory.
    if grep -q "monitor_present[[:space:]]*1" /proc/asound/card*/eld* 2>/dev/null; then
        NUDGED_OUTPUTS=""
        return 0
    fi

    local xrandr_out conn name output mode
    xrandr_out=$(xrandr --query 2>/dev/null) || return 0

    for conn in /sys/class/drm/card*-*/status; do
        [ -e "$conn" ] || continue
        [ "$(cat "$conn" 2>/dev/null)" = "connected" ] || continue
        name=${conn%/status}; name=${name##*/card?-}
        case "$name" in eDP*|LVDS*) continue ;; esac
        case " $NUDGED_OUTPUTS " in *" $name "*) continue ;; esac

        # DRM connector names match the modesetting driver's output names.
        output=$(printf '%s\n' "$xrandr_out" | awk -v n="$name" '$1 == n && $2 == "connected" {print $1; exit}')
        [ -n "$output" ] || continue

        # Keep the mode the kiosk chose (it may have capped a 4K panel);
        # --auto only as a fallback.
        mode=$(printf '%s\n' "$xrandr_out" | awk -v out="$output" '
            $1 == out { active = 1; next }
            active && /^[[:space:]]+[0-9]+x[0-9]+/ { if ($0 ~ /\*/) { print $1; exit } }
            active && /^[^[:space:]]/ { active = 0 }')

        NUDGED_OUTPUTS="$NUDGED_OUTPUTS $name"
        xrandr --output "$output" --off 2>/dev/null || true
        sleep 1
        if [ -n "$mode" ]; then
            xrandr --output "$output" --mode "$mode" 2>/dev/null \
                || xrandr --output "$output" --auto 2>/dev/null || true
        else
            xrandr --output "$output" --auto 2>/dev/null || true
        fi
        sleep 2
    done
}

route_once() {
    local cards_dump card active want sink default_sink
    cards_dump=$(pactl list cards 2>/dev/null) || return 0
    [ -n "$cards_dump" ] || return 0

    # One line per card: "<card>\t<active>\t<wanted-profile>"
    # wanted = highest-priority available output:hdmi-stereo* profile, else
    # highest-priority available output:analog* profile.
    while IFS=$'\t' read -r card active want; do
        [ -n "$card" ] && [ -n "$want" ] || continue
        if [ "$active" != "$want" ]; then
            pactl set-card-profile "$card" "$want" 2>/dev/null || true
        fi
    done < <(printf '%s\n' "$cards_dump" | awk '
        function flush() {
            if (card != "") print card "\t" active "\t" (hdmi != "" ? hdmi : analog)
            card=""; active=""; hdmi=""; analog=""; hdmi_p=-1; analog_p=-1
        }
        /^Card #/ { flush() }
        /^\tName: / { card=$2 }
        /^\t\toutput:/ {
            line=$0; sub(/^\t\t/, "", line)
            prof=line; sub(/: .*/, "", prof)
            prio=0
            if (match(line, /priority: [0-9]+/)) prio=substr(line, RSTART+10, RLENGTH-10)+0
            avail = (line ~ /available: yes/ || line ~ /availability unknown/)
            if (!avail) next
            if (prof ~ /^output:hdmi-stereo/) { if (prio > hdmi_p) { hdmi=prof; hdmi_p=prio } }
            else if (prof ~ /^output:analog/)  { if (prio > analog_p) { analog=prof; analog_p=prio } }
        }
        /^\tActive Profile: / { active=$3 }
        END { flush() }
    ')

    # Point the default sink at HDMI when one exists, else the first sink.
    sink=$(pactl list short sinks 2>/dev/null | awk '/hdmi/{print $2; exit}')
    [ -n "$sink" ] || sink=$(pactl list short sinks 2>/dev/null | awk 'NR==1{print $2}')
    [ -n "$sink" ] || return 0

    default_sink=$(pactl get-default-sink 2>/dev/null)
    if [ "$sink" != "$default_sink" ] || [ "$sink" != "$LAST_SINK" ]; then
        pactl set-default-sink "$sink" 2>/dev/null || true
        pactl set-sink-mute "$sink" 0 2>/dev/null || true
        case "$sink" in
            *hdmi*) pactl set-sink-volume "$sink" 100% 2>/dev/null || true ;;
        esac
        # Migrate live streams so playing audio follows the hot-plug.
        pactl list short sink-inputs 2>/dev/null | while read -r id _; do
            pactl move-sink-input "$id" "$sink" 2>/dev/null || true
        done
        LAST_SINK="$sink"
    fi
}

while true; do
    eld_nudge_once
    route_once
    sleep 5
done
