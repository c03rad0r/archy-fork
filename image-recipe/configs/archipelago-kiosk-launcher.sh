#!/bin/bash

# TearFree BEFORE X starts: bare Xorg with the stock modesetting driver has
# no vsync and no compositor, so video page-flips land mid-scanout — visible
# tearing on every kiosk (reported 2026-08-11, "really bad" on IndeedHub
# playback). The modesetting driver's TearFree option double-buffers the
# scanout at the driver level: no compositor needed, one frame of latency,
# no interaction with the 2026-06-28 choppy-audio GPU decisions. Written
# here (idempotently) rather than baked into the image so existing kiosk
# nodes pick it up through the launcher's own OTA path (bootstrap.rs
# reinstalls this script on every node).
XORG_CONF_DIR=/etc/X11/xorg.conf.d
XORG_TEARFREE="$XORG_CONF_DIR/20-archipelago-kiosk-tearfree.conf"
mkdir -p "$XORG_CONF_DIR"
if [ ! -f "$XORG_TEARFREE" ] || ! grep -q TearFree "$XORG_TEARFREE"; then
    cat > "$XORG_TEARFREE" <<'EOF'
# Written by archipelago-kiosk-launcher — vsynced scanout for kiosk video.
Section "Device"
    Identifier "Archipelago Kiosk GPU"
    Driver "modesetting"
    Option "TearFree" "true"
EndSection
EOF
fi

# Start a dedicated X server for the attached kiosk display.
/usr/bin/Xorg :0 vt1 -nolisten tcp -keeptty &
XPID=$!

export DISPLAY=:0
export HOME=/home/archipelago

X_READY=false
for _ in $(seq 1 30); do
    if kill -0 "$XPID" 2>/dev/null && xrandr --query >/tmp/archipelago-kiosk-xrandr.txt 2>/dev/null; then
        X_READY=true
        break
    fi
    sleep 0.5
done

if [ "$X_READY" != "true" ]; then
    echo 'ERROR: Xorg failed to become ready'
    kill "$XPID" 2>/dev/null || true
    exit 1
fi

# Settings-managed display overrides (system.kiosk-display.set writes this
# file, then restarts the kiosk): may set ARCHIPELAGO_KIOSK_SCALE,
# ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH or ARCHIPELAGO_KIOSK_MAX_WIDTH.
[ -f /etc/archipelago/kiosk-display.conf ] && . /etc/archipelago/kiosk-display.conf

KIOSK_SAFE_AREA_X_PX=${ARCHIPELAGO_KIOSK_SAFE_AREA_X_PX:-}
KIOSK_SAFE_AREA_Y_PX=${ARCHIPELAGO_KIOSK_SAFE_AREA_Y_PX:-}

# Actual mode of the kiosk output — configure_display overwrites these so
# Chromium's window matches the panel instead of assuming 1080p (a 1366x768
# laptop panel otherwise gets a clipped oversized window).
KIOSK_MODE_W=1920
KIOSK_MODE_H=1080

configure_display() {
    command -v xrandr >/dev/null 2>&1 || return 0

    local output mode internal width height
    output=$(awk '/ connected/ && $1 !~ /^eDP|^LVDS/{print $1; exit}' /tmp/archipelago-kiosk-xrandr.txt)
    [ -n "$output" ] || output=$(awk '/ connected/{print $1; exit}' /tmp/archipelago-kiosk-xrandr.txt)
    [ -n "$output" ] || return 0

    # Pick the EDID-preferred ("+") mode, falling back to the first-listed
    # mode (EDID lists native first). Deliberately ignore "*" (currently
    # active) — trusting "active" lets a bad clone/mirror state from a
    # previous boot perpetuate itself forever instead of self-healing.
    mode=$(awk -v out="$output" '
        $1 == out { active = 1; next }
        active && /^[[:space:]]+[0-9]+x[0-9]+/ {
            if ($0 ~ /\+/ && !preferred) { preferred = $1 }
            if (!first) first = $1
        }
        active && /^[^[:space:]]/ { active = 0 }
        END { if (preferred) print preferred; else if (first) print first }
    ' /tmp/archipelago-kiosk-xrandr.txt)
    [ -n "$mode" ] || mode=1920x1080

    # Optional resolution cap for weak hardware: ARCHIPELAGO_KIOSK_MAX_WIDTH=1920
    # drops a 4K panel to its best <=1920-wide mode (the TV upscales) so the
    # software rasterizer paints 1/4 the pixels. Off by default — native mode
    # is sharp and fits the tile budget now that --window-size is in DIPs.
    local max_w=${ARCHIPELAGO_KIOSK_MAX_WIDTH:-0}
    if [ "$max_w" -gt 0 ] 2>/dev/null && [ "${mode%x*}" -gt "$max_w" ] 2>/dev/null; then
        local capped
        capped=$(awk -v out="$output" -v maxw="$max_w" '
            $1 == out { active = 1; next }
            active && /^[[:space:]]+[0-9]+x[0-9]+/ {
                split($1, wh, "x")
                if (wh[1] + 0 <= maxw && wh[1] + 0 > best_w) { best_w = wh[1] + 0; best = $1 }
            }
            active && /^[^[:space:]]/ { active = 0 }
            END { if (best) print best }
        ' /tmp/archipelago-kiosk-xrandr.txt)
        [ -n "$capped" ] && mode=$capped
    fi

    # Kiosk should use one native output. A spanning desktop makes Chromium land
    # on the laptop panel or stretch across both outputs.
    for internal in $(awk '/ connected/ && $1 ~ /^eDP|^LVDS/{print $1}' /tmp/archipelago-kiosk-xrandr.txt); do
        [ "$internal" = "$output" ] || xrandr --output "$internal" --off 2>/dev/null || true
    done

    xrandr --output "$output" \
        --primary \
        --mode "$mode" \
        --pos 0x0 \
        --scale 1x1 \
        --panning 0x0 \
        --transform none 2>/dev/null || true

    width=${mode%x*}
    height=${mode#*x}
    case "$width:$height" in *[!0-9:]*|:*) width=1920; height=1080 ;; esac
    KIOSK_MODE_W=$width
    KIOSK_MODE_H=$height

    # Browser safe-area fallback for TVs that crop edges. Driver underscan is
    # preferable, but many Intel HDMI outputs do not expose that property.
    KIOSK_SAFE_AREA_X_PX=${KIOSK_SAFE_AREA_X_PX:-$((width * 3 / 100))}
    KIOSK_SAFE_AREA_Y_PX=${KIOSK_SAFE_AREA_Y_PX:-$((height * 3 / 100))}
}

configure_display

# --- Kiosk UI scaling for large / high-res displays -----------------------
# REVERT: set env ARCHIPELAGO_KIOSK_SCALE=1 (per-node, no rebuild), or restore
# the hardcoded --force-device-scale-factor=1 below to disable entirely.
#
# A big TV reports its full native resolution as the CSS viewport, so a 4K
# panel becomes a 3840px-wide viewport and the UI renders tiny — and a
# keyboard-less kiosk can't zoom. Derive Chromium's device-scale-factor from
# the detected panel width so the *effective* CSS viewport lands near a
# comfortable target. Panels >=2560 wide get a 1920-wide layout (4K -> scale
# 2.0 — user-validated on a 72" TV: spacious desktop layout, 2x sharp);
# smaller panels get a 1280-wide layout (1920 -> 1.50, laptops -> 1.0).
if [ -z "${ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH:-}" ]; then
    if [ "$KIOSK_MODE_W" -ge 2560 ] 2>/dev/null; then
        ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH=1920
    else
        ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH=1280
    fi
fi
KIOSK_TARGET_CSS_WIDTH=$ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH
if [ -n "${ARCHIPELAGO_KIOSK_SCALE:-}" ]; then
    KIOSK_SCALE=$ARCHIPELAGO_KIOSK_SCALE
else
    KIOSK_SCALE=$(awk -v w="$KIOSK_MODE_W" -v t="$KIOSK_TARGET_CSS_WIDTH" \
        'BEGIN{if(t<=0)t=1280; s=w/t; s=int(s*4+0.5)/4; if(s<1)s=1; if(s>3)s=3; printf "%.2f", s}')
fi

# Chromium's --window-size is in DIPs (CSS px), not physical pixels: at scale S
# the window paints S x larger. This X session has no window manager, so
# --kiosk/--start-fullscreen cannot snap an oversized window back to the panel
# — it just hangs off the right/bottom edges (content cropped, background art
# offscreen). Size the window in DIPs so DIPs x scale = the panel exactly.
KIOSK_WIN_W=$(awk -v w="$KIOSK_MODE_W" -v s="$KIOSK_SCALE" 'BEGIN{printf "%d", w/s}')
KIOSK_WIN_H=$(awk -v h="$KIOSK_MODE_H" -v s="$KIOSK_SCALE" 'BEGIN{printf "%d", h/s}')

xhost +SI:localuser:archipelago 2>/dev/null || true
xsetroot -solid black 2>/dev/null || true
xset s off 2>/dev/null || true
xset -dpms 2>/dev/null || true
xset s noblank 2>/dev/null || true

pkill -u archipelago -f 'chromium.*localhost' 2>/dev/null || true
sleep 1

# ── Graphics tier ────────────────────────────────────────────────────────
# One flag set does not fit all kiosk hardware. The June-2026 choppy-audio
# incident (#36) proved HD 5500-era boxes melt with GPU rasterization on
# (under X11 they fall back to software compositing while a GPU process
# burns 55-92% CPU) — but holding MODERN iGPUs to those same defensive
# flags (single raster thread, raster ban) froze framework-pt outright on
# the animated network map at 4K (2026-08-13). So: two tiers.
#
#   legacy — the proven HD 5500 tuning: --in-process-gpu, ONE raster
#            thread, GpuRasterization banned via --disable-features.
#   modern — --in-process-gpu, Chromium's default raster threads,
#            GpuRasterization allowed.
#
# KIOSK_GRAPHICS in /etc/archipelago/kiosk-display.conf overrides:
#   auto (default) | performance (force legacy) | quality (force modern)
# Set from Settings → Display; sourced with the rest of the conf above.
#
# Auto classifies by CPU model string — transparent and greppable, and the
# iGPU generation tracks the CPU generation on this hardware. Rules:
#   * "NNth Gen Intel"      → only stamped on gen 10+ model names → modern
#   * AMD Ryzen             → modern
#   * Intel iN-NNNNN (5 dig)→ gen 10+ desktop → modern
#   * Intel iN-8xxx/9xxx    → gen 8/9 → modern
#   * anything else         → legacy (fail conservative: slow-but-stable)
detect_graphics_tier() {
    case "${KIOSK_GRAPHICS:-auto}" in
        performance) echo legacy; return ;;
        quality)     echo modern; return ;;
    esac
    _cpu=$(grep -m1 '^model name' /proc/cpuinfo 2>/dev/null || true)
    case "$_cpu" in
        *"Gen Intel"*) echo modern; return ;;
        *AMD*Ryzen*)   echo modern; return ;;
    esac
    _num=$(printf '%s' "$_cpu" | grep -oE 'i[3579]-[0-9]{4,5}' | head -1 | cut -d- -f2)
    case "$_num" in
        [0-9][0-9][0-9][0-9][0-9]) echo modern; return ;;  # 5 digits = gen 10+
        [89][0-9][0-9][0-9])       echo modern; return ;;  # gen 8/9
    esac
    echo legacy
}

if [ -e /dev/dri/card0 ] || [ -e /dev/dri/renderD128 ]; then
    GRAPHICS_TIER=$(detect_graphics_tier)
    if [ "$GRAPHICS_TIER" = "modern" ]; then
        GPU_FLAGS="--in-process-gpu"
        EXTRA_DISABLED_FEATURES=""
    else
        GPU_FLAGS="--in-process-gpu --num-raster-threads=1"
        EXTRA_DISABLED_FEATURES=",GpuRasterization"
    fi
    # Hardware VIDEO DECODE (VA-API) on GPU hardware — both tiers. Decode
    # offload REDUCES the CPU pressure that caused the choppy-audio
    # incident, it doesn't re-create it. Falls back silently to software
    # decode when the platform lacks a va driver — never a black player.
    # IgnoreDriverChecks: older Intel gens (HD 5500-era kiosks) are wrongly
    # blocklisted upstream.
    ENABLE_FEATURES="OverlayScrollbar,VaapiVideoDecodeLinuxGL,VaapiIgnoreDriverChecks"
    echo "archipelago-kiosk: graphics tier=$GRAPHICS_TIER (KIOSK_GRAPHICS=${KIOSK_GRAPHICS:-auto})"
else
    # GPU-less / headless server (no /dev/dri): no GPU at all.
    GRAPHICS_TIER=headless
    GPU_FLAGS="--disable-gpu --num-raster-threads=1"
    EXTRA_DISABLED_FEATURES=",GpuRasterization"
    ENABLE_FEATURES="OverlayScrollbar"
fi

ARCHIPELAGO_UID=$(id -u archipelago)

while true; do
    # A profile lock left by a previous boot encodes <hostname>-<pid>; after a
    # hostname change (node rename) Chromium reads it as another computer
    # holding the profile and refuses to start — with --noerrdialogs that is an
    # invisible failure and the kiosk black-screens forever. Any Chromium that
    # owned the lock is dead by now (pkill above / previous loop iteration).
    rm -f /var/lib/archipelago/chromium-kiosk/Singleton{Lock,Cookie,Socket}
    # XDG_RUNTIME_DIR must be passed explicitly — without it Chromium's audio
    # backend can't find PipeWire-Pulse's socket at /run/user/<uid>/pulse/native,
    # falls back to raw ALSA "default", fails to connect, and produces no audio
    # at all with no visible error (--noerrdialogs suppresses it).
    # OverlayScrollbar: thin auto-hiding scrollbars (the Chrome-on-a-remote-
    # device look) instead of classic X11 scrollbar chrome — a kiosk TV showed
    # a permanent fat scrollbar on scrollable views (Peers).
    # Force a DARK color-scheme preference. The main UI hardcodes its dark
    # theme, but the bundled AIUI app themes via `@media (prefers-color-scheme)`
    # and defaults to its LIGHT variant (white panels) when the browser reports
    # no preference — which a minimal kiosk X session does. Two independent dark
    # signals so this survives a Chromium enum change: GTK_THEME is version-
    # independent and can never force light (worst case: no effect), and the
    # blink-settings flag (0 = kDark in modern Chromium) reinforces it. Neither
    # can regress the always-dark main UI.
    sudo -u archipelago env DISPLAY=:0 HOME=/home/archipelago GTK_THEME=Adwaita:dark XDG_RUNTIME_DIR=/run/user/$ARCHIPELAGO_UID chromium --kiosk \
      --app=http://localhost/kiosk?safe_area_x=${KIOSK_SAFE_AREA_X_PX:-0}\&safe_area_y=${KIOSK_SAFE_AREA_Y_PX:-0} \
      --blink-settings=preferredColorScheme=0 \
      --noerrdialogs \
      --disable-infobars \
      --disable-translate \
      --no-first-run \
      --check-for-update-interval=31536000 \
      --disable-features=TranslateUI,MetricsReporting,AutofillServerCommunication,PasswordManagerEnabled${EXTRA_DISABLED_FEATURES} \
      --enable-features=$ENABLE_FEATURES \
      --disable-session-crashed-bubble \
      --disable-save-password-bubble \
      --disable-suggestions-service \
      --disable-component-update \
      $GPU_FLAGS \
      --renderer-process-limit=2 \
      --window-size=${KIOSK_WIN_W},${KIOSK_WIN_H} \
      --window-position=0,0 \
      --start-fullscreen \
      --force-device-scale-factor=${KIOSK_SCALE} \
      --disable-background-networking \
      --disable-background-timer-throttling \
      --disable-backgrounding-occluded-windows \
      --disable-breakpad \
      --disable-metrics \
      --disable-metrics-reporting \
      --disable-domain-reliability \
      --js-flags="--max-old-space-size=256" \
      --remote-debugging-port=9222 \
      --user-data-dir=/var/lib/archipelago/chromium-kiosk
      # --remote-debugging-port binds to 127.0.0.1 only. It feeds the
      # backend's CDP input bridge (api/handler/cdp.rs), which dispatches
      # companion-app remote input as TRUSTED browser events — the only way
      # taps/keys/scrolls reach inside cross-origin app iframes. Chromium's
      # default origin check stays on (no --remote-allow-origins), so web
      # pages cannot open the debug socket.
    sleep 3
done

kill "$XPID" 2>/dev/null || true
