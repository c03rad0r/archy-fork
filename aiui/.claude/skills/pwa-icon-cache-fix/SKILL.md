---
name: pwa-icon-cache-fix
description: Use when the user reports a PWA icon not updating, stale PWA icon, wrong icon after install, or any PWA caching issue. Also applies when changing PWA icons in a Vite + vite-plugin-pwa project.
version: 2.0.0
---

# PWA Icon Cache Fix

## Problem

PWA icons are cached at FOUR independent layers:
1. **Service worker cache** (Workbox precache)
2. **Browser HTTP cache**
3. **Browser manifest resources** (Chromium stores resized icons in its profile data, keyed by a permanent extension ID tied to the origin — NEVER re-fetched even after uninstall/reinstall)
4. **macOS .app bundle** (`.icns` file baked into the `.app` in `~/Applications/`)

Query string cache busting (`?v=2`) and uninstall/reinstall do NOT fix this. Chromium reuses the same extension ID for the same origin, so it keeps the old cached icons.

## Fix Steps

### 1. Verify icon files on disk and server are correct

```bash
# Visual check
Read packages/app/public/pwa-192x192.png
Read packages/app/public/pwa-512x512.png

# Hash match check
curl -s http://localhost:5173/pwa-192x192.png | md5
md5 -q packages/app/public/pwa-192x192.png
```

### 2. Find the PWA's Chromium extension ID

Read the installed `.app` bundle's `Info.plist` to get the `CrAppModeShortcutID`:

```bash
plutil -p "~/Applications/Brave Browser Apps.localized/AIUI.app/Contents/Info.plist" | grep CrAppModeShortcutID
```

This returns an ID like `idemibpphagihbobmgmaojhjfidlfpdl`.

### 3. Overwrite the cached icons in browser profile

Chromium stores resized icons at:
`~/Library/Application Support/BraveSoftware/Brave-Browser/Default/Web Applications/Manifest Resources/{ID}/Icons/`

Overwrite every size using `sips`:

```bash
ICON_DIR="~/Library/Application Support/BraveSoftware/Brave-Browser/Default/Web Applications/Manifest Resources/{ID}/Icons"
SRC="packages/app/public/pwa-512x512.png"
for size in 32 48 64 96 128 192 256 512; do
  sips -z $size $size "$SRC" --out "${ICON_DIR}/${size}.png"
done
```

### 4. Rebuild the macOS .icns in the .app bundle

```bash
ICONSET="/tmp/aiui.iconset"
mkdir -p "$ICONSET"
SRC="packages/app/public/pwa-512x512.png"
sips -z 16 16 "$SRC" --out "$ICONSET/icon_16x16.png"
sips -z 32 32 "$SRC" --out "$ICONSET/icon_16x16@2x.png"
sips -z 32 32 "$SRC" --out "$ICONSET/icon_32x32.png"
sips -z 64 64 "$SRC" --out "$ICONSET/icon_32x32@2x.png"
sips -z 128 128 "$SRC" --out "$ICONSET/icon_128x128.png"
sips -z 256 256 "$SRC" --out "$ICONSET/icon_128x128@2x.png"
sips -z 256 256 "$SRC" --out "$ICONSET/icon_256x256.png"
sips -z 512 512 "$SRC" --out "$ICONSET/icon_256x256@2x.png"
sips -z 512 512 "$SRC" --out "$ICONSET/icon_512x512.png"
cp "$SRC" "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o "~/Applications/Brave Browser Apps.localized/AIUI.app/Contents/Resources/app.icns"
```

### 5. Flush macOS icon cache

```bash
touch "~/Applications/Brave Browser Apps.localized/AIUI.app"
killall Finder
killall Dock
```

### 6. Bump PWA_CACHE_VERSION in main.ts

Increment the `PWA_CACHE_VERSION` constant — this nukes all SW caches on next page load for web-layer caching.

### 7. Delete stale build artifacts

Remove old `dist/` and `dev-dist/` SW/manifest files.

## Browser-Specific Paths

- **Brave**: `~/Library/Application Support/BraveSoftware/Brave-Browser/Default/Web Applications/`
- **Chrome**: `~/Library/Application Support/Google/Chrome/Default/Web Applications/`
- **PWA apps (Brave)**: `~/Applications/Brave Browser Apps.localized/`
- **PWA apps (Chrome)**: `~/Applications/Chrome Apps.localized/`

## Key Insight

Chromium assigns a permanent extension ID per origin (e.g., `localhost:5173`). This ID persists across uninstall/reinstall. The icon cache in `Manifest Resources/{ID}/Icons/` is populated ONCE and never refreshed from the manifest. The only fix is to overwrite the files directly on disk.
