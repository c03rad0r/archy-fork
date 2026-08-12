# iOS App Research — AIUI

## Overview

Three approaches for shipping AIUI (Vue 3 + Vite SPA) as an iOS app.

## Approach 1: Capacitor (Recommended)

Capacitor wraps the Vite build output (`dist/`) in a native iOS Xcode project. The web app runs inside WKWebView with a JavaScript bridge to native device APIs.

```bash
pnpm add @capacitor/core @capacitor/cli @capacitor/ios
npx cap init && npx cap add ios
pnpm build && npx cap sync
npx cap open ios  # opens Xcode
```

**Pros:**
- Near-zero code changes to existing Vue 3 app — one codebase for web + iOS + Android
- Large, mature plugin ecosystem (camera, biometrics, push, geolocation, haptics)
- Hot reload during dev via `npx cap run ios --livereload`
- OTA live updates possible via Capgo, bypassing App Store review for JS changes
- `@capacitor/push-notifications` wraps APNs natively

**Cons:**
- Service workers do NOT work in WKWebView on iOS (capacitor:// protocol breaks SW registration)
- Performance ceiling is WebKit JS engine (not V8)
- Each iOS SDK bump requires Capacitor + plugin updates

**Push Notifications:** Full support via `@capacitor/push-notifications` (APNs). Production-grade.

**Offline:** Entire app bundle ships inside .ipa — available offline. Dynamic data must use `@capacitor/preferences` or local SQLite. Workbox/SW caching does not work.

**Performance:** Modern WKWebView uses Nitro JS engine (same as Safari). For a chat UI like AIUI, indistinguishable from Safari. GPU-accelerated CSS transforms work well.

## Approach 2: Custom WKWebView Swift Wrapper

Write a native Swift/SwiftUI app embedding WKWebView. Use `WKScriptMessageHandler` for JS↔Swift communication.

**Pros:**
- Maximum native control — own the shell, native navigation, gestures
- Can implement App Clips, Share Extensions, Widgets alongside web content
- Full access to all iOS APIs at the native layer

**Cons:**
- Requires Swift knowledge — adds second language + build system
- JS↔Swift bridge must be hand-written for every integration
- No structured plugin community; each integration is bespoke
- More setup friction vs Capacitor

**Push/Offline/Performance:** Same as Capacitor (all use WKWebView). More manual setup.

## Approach 3: React Native WebView

Create a React Native app with `react-native-webview` rendering the Vite build output.

**Pros:**
- RN has deep native API access and large ecosystem
- Surrounding shell can be fully native

**Cons:**
- Two separate tech stacks (Vue + RN) — highest maintenance burden
- No code sharing between Vue app and RN shell
- Performance often worse (full RN runtime + WebView engine)
- RN's own breaking changes cadence adds risk

**Verdict:** Only justified if an existing RN app is already in production.

## App Store Risk: Guideline 4.2

Apple's Guideline 4.2 (Minimum Functionality) is the primary risk for all webview-based apps. Apps that pass share these traits:
- Native tab bar or navigation (not web-based menus)
- At least one native API integration (push, biometrics, camera, Apple Pay)
- Offline functionality beyond what a browser bookmark offers
- UI formatted for iOS, not a desktop website in a phone frame

For AIUI: the chat interface, push notifications, and offline message history constitute sufficient native functionality.

## Service Workers in WKWebView

**SWs do not run inside WKWebView** — this is a fundamental WebKit limitation, not framework-specific. The correct offline strategy for all three approaches: ship assets in app bundle + implement dynamic caching via native storage APIs.

## Deep Linking

All three support iOS Universal Links via AASA file + Associated Domains capability:
- **Capacitor:** `@capacitor/app` `appUrlOpen` event → Vue Router
- **Custom WKWebView:** `AppDelegate.application(_:continue:...)` → JS evaluation
- **RN:** React Navigation linking config → WebView `postMessage`

## Comparison

| Dimension | Capacitor | Custom WKWebView | RN WebView |
|---|---|---|---|
| Vue code reuse | 100% | 100% | 100% |
| Native shell effort | Low | High | Very high |
| Push notifications | First-class | Manual APNs | Via RN layer |
| App Store risk | Moderate* | Moderate* | Moderate* |
| Performance | Good | Good | Adequate |
| Maintenance burden | Low-moderate | High | Very high |
| Team fit (web-first) | Best | Poor | Poor |

*All face identical Guideline 4.2 scrutiny — framework choice is irrelevant to reviewers.

## Concrete Next Steps

1. Add `@capacitor/core`, `@capacitor/cli`, `@capacitor/ios` to `packages/app`
2. Set Vite `base: './'` for the Capacitor build config
3. Disable PWA service worker for native builds (partially done already)
4. Add `@capacitor/push-notifications` for APNs
5. Implement native splash screen and app icon
6. Test on iOS Simulator via `npx cap run ios`
7. Set up Apple Developer account + code signing
8. Submit TestFlight build for internal testing
