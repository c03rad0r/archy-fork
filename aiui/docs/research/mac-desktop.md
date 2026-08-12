# Mac Desktop App Research — AIUI

## Overview

Two approaches for shipping AIUI as a Mac desktop app: Tauri v2 (Rust-based, system WebView) vs Electron (Chromium-based).

## Tauri v2 (Recommended)

Released stable October 2024. Uses OS-native WebView (WKWebView on macOS). The Vue 3 + Vite frontend runs inside the WebView unchanged. JS calls into Rust via typed IPC bridge.

**Binary Size:** 2–8 MB installer (no bundled runtime)
**Memory Usage:** ~30–40 MB idle
**Startup Time:** < 500ms

### Menu Bar App Pattern (Raycast-style)

Fully supported via `tauri-plugin-positioner` + tray + window APIs. Frameless popover window anchored to tray icon with `decorations: false`, `skip_taskbar: true`. Community examples exist (`ahkohd/tauri-macos-menubar-app-example` v2-popover branch).

### Global Hotkey

Built-in via `@tauri-apps/plugin-global-shortcut`. Register accelerators (e.g., `CmdOrCtrl+Space`) that fire even when background/minimized. First-class plugin.

### System Tray

First-class support. `AppHandle::tray()` with native menus and click event handling from Rust or frontend.

### Auto-Update

`@tauri-apps/plugin-updater` — signed updates required (Ed25519 keypair). Host a static JSON endpoint with version metadata and signed artifact URLs.

### macOS Code Signing / Notarization

Automated via Tauri CLI environment variables (`APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`). Notarization adds ~2–5 min per build.

### Build Pipeline

- Prerequisites: Rust toolchain + Xcode CLI tools
- First build: 5–15 min (Cargo compiles Rust deps)
- Incremental builds: Fast with caching
- Config: `tauri.conf.json` + `Cargo.toml`
- Complexity: Medium-High (Rust requirement is the barrier)

### Mobile Support

Tauri v2 has **first-class iOS/Android support** in the same codebase (WKWebView on iOS, Android System WebView on Android). HMR extends to physical devices. This is a genuine differentiator — Electron is desktop-only.

## Electron

Mature since 2013. Bundles full Chromium + Node.js runtime. Used by VS Code, Slack, Discord, Obsidian.

**Binary Size:** 80–150 MB installer
**Memory Usage:** 200–350 MB idle
**Startup Time:** 1–2s

### Menu Bar App

Well-established via `menubar` npm package. Creates BrowserWindow positioned below tray icon, manages show/hide on tray click. Very mature.

### Global Hotkey

`globalShortcut` module in Electron core. System-wide even when hidden.

### System Tray

`Tray` class in Electron core with context menus and click events.

### Auto-Update

`electron-updater` (S3/GitHub Releases) or `update.electronjs.org` (free for open-source).

### macOS Code Signing / Notarization

Via `@electron/osx-sign` + `@electron/notarize`, integrated into `electron-builder` / Electron Forge.

### Build Pipeline

- Prerequisites: Node.js only — no additional runtimes
- Build tools: `electron-vite` for Vue 3 + Vite integration
- Build times: 2–5 min (no Rust compilation) + 2–5 min notarization
- Complexity: Medium (main/renderer process split requires understanding)

## Comparison

| Dimension | Tauri v2 | Electron |
|---|---|---|
| Installer size | 2–8 MB | 80–150 MB |
| Idle RAM | 30–40 MB | 200–350 MB |
| Startup time | < 500ms | 1–2s |
| Menu bar app | Supported | Supported |
| Global hotkey | Built-in plugin | Built-in API |
| System tray | Built-in | Built-in |
| Auto-update | Built-in (signed) | electron-updater |
| New language | Rust | None (JS/TS) |
| iOS/Android | Yes (same codebase) | No |
| WebView | WKWebView (varies by OS) | Chromium (pinned, consistent) |
| Ecosystem maturity | Growing fast | Very mature |
| Security model | Capability-based, opt-in | Opt-out, manual discipline |
| Debug tools | Safari Web Inspector | Chrome DevTools |

## Recommendation

**Tauri v2 is the stronger choice for AIUI:**

1. **Memory advantage is decisive.** Users running local LLMs or managing API streaming need resources for the AI workload, not the shell. 30 MB vs 300 MB matters.
2. **Menu bar pattern fits naturally** for a chat/AI assistant (Raycast-style quick invoke).
3. **iOS/Android support** from the same codebase aligns with AIUI's multi-surface vision.
4. **Capability-based security** is appropriate for handling API keys and sensitive chat data.
5. **Binary size matters** — 5 MB download vs 120 MB affects distribution trust.

## Concrete Next Steps

1. Scaffold Tauri v2 project: `npm create tauri-app@latest` with Vite template
2. Point dev server to existing `packages/app` Vite config
3. Implement tray icon + menu bar popover window
4. Register global hotkey (e.g., `Cmd+Shift+Space`) to invoke chat
5. Write Rust commands for: file I/O, tray management, updater config
6. Set up macOS code signing + notarization pipeline
7. Distribute via Homebrew cask or direct download
8. Evaluate Tauri mobile targets for iOS/Android convergence
