# Three-Mode UI System: Easy / Pro / Chat

## Overview

Archipelago's UI will support three switchable modes, each targeting a different user experience level:

| Mode | Label in UI | Target User | What They See |
|------|-------------|-------------|---------------|
| **Pro** | Pro | Power users, developers, node operators | Current full interface — all services, configs, technical details |
| **Easy** | Easy | Complete beginners, non-technical users | Goal-based interface — "Open a Shop", "Store My Photos" |
| **Chat** | Chat | Everyone (future) | Conversational AI interface powered by AIUI |

### Key Principles

1. **Pro mode is preserved** — the current interface stays exactly as-is and continues to be improved
2. **Same URLs** — modes don't change route paths. `/dashboard` shows different content based on mode
3. **Cross-surfacing** — Easy mode goals are searchable from Spotlight (Cmd+K) and suggested in Pro mode
4. **Persistent preference** — mode choice saved to localStorage + backend UIData

---

## How Modes Work

### Architecture: Conditional Rendering

Rather than separate route trees (`/easy/home`, `/pro/home`), the mode controls **what renders within existing routes**:

```
Dashboard.vue (shared shell)
  ├── Sidebar → nav items change per mode
  ├── ModeSwitcher → always visible in sidebar
  └── <RouterView>
        └── Home.vue (dispatcher)
              ├── <GamerHome /> (Pro mode)
              ├── <EasyHome />  (Easy mode)
              └── <ChatHome />  (Chat mode)
```

This means:
- Auth guards, WebSocket, stores — all shared
- URLs never change — bookmarks work regardless of mode
- Both modes use the same component library (glass-card, glass-button, etc.)

### Navigation Per Mode

**Pro Mode** (current, 7 items):
```
Home → My Apps → App Store → Cloud → Network → Web5 → Settings
```

**Easy Mode** (simplified, 3 items):
```
Home → My Services → Settings
```

**Chat Mode** (4 items):
```
Home → Chat → My Apps → Settings
```

---

## Easy Mode: Goal-Based Interface

### The Problem

Current interface says: "Here are 20+ services you can install. Figure out which ones you need, install them, configure them to talk to each other."

Easy mode says: **"What do you want to do?"**

### Goal Cards (Easy Mode Home)

When in Easy mode, the Home screen shows goal cards instead of the current 4 technical overview cards:

```
┌─────────────────────┐  ┌─────────────────────┐
│  🏪 Open a Shop     │  │  ⚡ Accept Payments  │
│                     │  │                     │
│  Set up your own    │  │  Receive Bitcoin &  │
│  Bitcoin-powered    │  │  Lightning payments │
│  online store       │  │                     │
│                     │  │  ~30 min • Beginner │
│  ~45 min • Beginner │  │  ▸ Start            │
│  ▸ Start            │  └─────────────────────┘
└─────────────────────┘
┌─────────────────────┐  ┌─────────────────────┐
│  📸 Store My Photos │  │  📁 Store My Files  │
│                     │  │                     │
│  Private photo      │  │  Personal cloud     │
│  backup & gallery   │  │  storage & sync     │
│                     │  │                     │
│  ~15 min • Beginner │  │  ~20 min • Beginner │
│  ▸ Start            │  │  ▸ Start            │
└─────────────────────┘  └─────────────────────┘
┌─────────────────────┐  ┌─────────────────────┐
│  ⚡ Lightning Node   │  │  🔑 Create Identity │
│                     │  │                     │
│  Run your own       │  │  Sovereign DID &    │
│  Lightning Network  │  │  Nostr identity     │
│  routing node       │  │                     │
│                     │  │  ~5 min • Beginner  │
│  ~40 min • Beginner │  │  ▸ Start            │
│  ▸ Start            │  └─────────────────────┘
└─────────────────────┘
┌─────────────────────┐
│  💾 Back Up         │
│                     │
│  Encrypted backup   │
│  of your entire     │
│  node               │
│                     │
│  ~10 min • Beginner │
│  ▸ Start            │
└─────────────────────┘
```

### Goal Workflow Wizard

Clicking a goal opens a **multi-step wizard** at `/dashboard/goals/:goalId`:

```
┌──────────────────────────────────────────────────────┐
│  ← Back to Goals                                     │
│                                                      │
│  Open a Shop                                         │
│  Set up your own Bitcoin-powered online store         │
│                                                      │
│  Step 2 of 4                                         │
│  ═══════════════════════▓▓▓░░░░░░░░░░░░              │
│                                                      │
│  ┌─────────────────────────────────────────────────┐ │
│  │ ✅ Step 1: Install Bitcoin Node                 │ │
│  │    Bitcoin Core is running and syncing           │ │
│  └─────────────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────────────┐ │
│  │ ⏳ Step 2: Install Lightning Network            │ │
│  │    Installing LND...                   [45%]    │ │
│  │    ████████████████████░░░░░░░░░░░░░            │ │
│  └─────────────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────────────┐ │
│  │ ○ Step 3: Install BTCPay Server                 │ │
│  │    Waiting for Lightning to be ready             │ │
│  └─────────────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────────────┐ │
│  │ ○ Step 4: Set Up Your Store                     │ │
│  │    Configure your store name and settings        │ │
│  └─────────────────────────────────────────────────┘ │
│                                                      │
│  ℹ️ Bitcoin needs to sync before Lightning can       │
│     start. This takes 2-3 days on first run.         │
│                                                      │
└──────────────────────────────────────────────────────┘
```

**Smart features:**
- Steps already satisfied (app running from a previous goal) are auto-completed
- Dependency resolution: Bitcoin must be running before LND can start
- Real-time progress from WebSocket data patches
- `configure` steps open the app in the iframe launcher for the user to complete

### Goal Definitions

| Goal | What It Provisions | Estimated Time |
|------|-------------------|----------------|
| **Open a Shop** | Bitcoin Knots + LND + BTCPay Server | ~45 min |
| **Accept Payments** | Bitcoin Knots + LND | ~30 min |
| **Store My Photos** | Immich (photo management) | ~15 min |
| **Store My Files** | Nextcloud (cloud storage) | ~20 min |
| **Run a Lightning Node** | Bitcoin Knots + LND + channel setup | ~40 min |
| **Create My Identity** | Built-in DID + Nostr keypair | ~5 min |
| **Back Up Everything** | Built-in encrypted backup | ~10 min |

---

## Mode Switcher UI

### Desktop Sidebar

A compact three-segment toggle sits below the logo, above navigation:

```
┌──────────────────────┐
│ 🏝️ Archipelago      │
│     v0.1.0           │
│                      │
│ ┌──────┬──────┬────┐ │
│ │ Easy │ Pro  │Chat│ │  ← Mode switcher
│ └──────┴──────┴────┘ │
│                      │
│ ○ Home               │
│ ○ My Apps            │  ← Nav items change
│ ○ App Store          │     per mode
│ ○ ...                │
│                      │
│ ⚙ Settings           │
│ ↪ Logout             │
│ ● Online             │
└──────────────────────┘
```

### Settings Page

Full-width selection cards in a new "Interface Mode" section:

```
┌──────────────────────────────────────────────────────┐
│  Interface Mode                                       │
│  Choose how you want to interact with your node.      │
│                                                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │             │  │   ██████   │  │             │  │
│  │  Easy Mode  │  │  Pro Mode  │  │  Chat Mode  │  │
│  │             │  │  (Active)  │  │  (Soon)     │  │
│  │  Goal-based │  │  Full      │  │  AI chat    │  │
│  │  guided     │  │  control   │  │  interface  │  │
│  │  setup      │  │  of all    │  │             │  │
│  │             │  │  services  │  │             │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  │
└──────────────────────────────────────────────────────┘
```

Uses the existing `.path-option-card` / `.path-option-card--selected` pattern from OnboardingPath.vue.

### Mobile

Mode switcher is in Settings only (bottom tab bar has limited space).

---

## Cross-Surfacing: Goals Everywhere

### Spotlight Search (Cmd+K)

Goals are added to the help tree and appear in search results regardless of mode:

```
┌──────────────────────────────────────┐
│ 🔍 shop                              │
│                                       │
│ Quick Start Goals                     │
│   🚀 Open a Shop                     │
│   🚀 Accept Payments                 │
│                                       │
│ Navigate                              │
│   → App Store                         │
│                                       │
│ Actions                               │
│   → Install an App                    │
└──────────────────────────────────────┘
```

### Pro Mode Home

A "Quick Start Goals" section appears at the bottom of Pro mode's Home, giving power users easy access to the guided workflows:

```
┌──────────────────────────────────────────────────────┐
│  Quick Start Goals                                    │
│  Not sure where to start? Try a guided setup.         │
│                                                       │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐  │
│  │ Open a Shop  │ │ Accept       │ │ Store Photos │  │
│  │              │ │ Payments     │ │              │  │
│  └──────────────┘ └──────────────┘ └──────────────┘  │
└──────────────────────────────────────────────────────┘
```

---

## Chat Mode (Placeholder)

For now, Chat mode shows a placeholder with a disabled input:

```
┌──────────────────────────────────────────────────────┐
│                                                       │
│              💬  AI Assistant                          │
│                                                       │
│     Conversational interface coming soon.              │
│     Talk to your node, ask questions, and              │
│     manage everything through natural language.        │
│                                                       │
│     ┌──────────────────────────────────────┐          │
│     │ What would you like to do?           │          │
│     └──────────────────────────────────────┘          │
│                                                       │
│           AIUI integration in development              │
│                                                       │
└──────────────────────────────────────────────────────┘
```

When AIUI is integrated, this becomes the conversational interface where users can say things like "Set up a Lightning node" and the system guides them through it via chat.

---

## Data Model

### UIMode Type

```typescript
type UIMode = 'gamer' | 'easy' | 'chat'
```

Stored in:
- `localStorage` as `archipelago-ui-mode` (immediate, works offline)
- `UIData.mode` on the backend (synced via WebSocket, persists across devices)

### Goal Types

```typescript
interface GoalDefinition {
  id: string                    // 'open-a-shop'
  title: string                 // 'Open a Shop'
  subtitle: string              // 'Accept Bitcoin payments with your own store'
  icon: string                  // Icon identifier
  category: string              // 'commerce', 'payments', 'storage', etc.
  requiredApps: string[]        // ['bitcoin-core', 'lnd', 'btcpay-server']
  steps: GoalStep[]             // Sequential steps
  estimatedTime: string         // '~45 minutes'
  difficulty: 'beginner' | 'intermediate'
}

interface GoalStep {
  id: string
  title: string                 // 'Install Bitcoin Node'
  description: string
  appId?: string                // Which app this step provisions
  action: 'install' | 'configure' | 'verify' | 'info'
  isAutomatic: boolean          // Can system do this without user input?
}
```

---

## Implementation Order

| Phase | What | Files Changed | Visible Effect |
|-------|------|--------------|----------------|
| 1 | Data layer | types, stores, data files | None (foundation) |
| 2 | Mode switching | Dashboard, Settings, Router | Mode toggle appears, nav changes |
| 3 | Easy mode views | Home refactor, EasyHome, GoalDetail | Easy mode is functional |
| 4 | Chat + polish | Chat placeholder, Spotlight goals, Pro goals section | Complete system |

Each phase deploys independently. Phase 1 is invisible. Phase 2 adds the switcher. Phase 3 makes Easy mode work. Phase 4 polishes everything.

---

## File Inventory

### New Files (10)
```
src/types/goals.ts          — Goal type definitions
src/data/goals.ts           — Goal catalog (7 goals)
src/stores/uiMode.ts        — UI mode Pinia store
src/stores/goals.ts         — Goal progress tracking
src/components/ModeSwitcher.vue  — Mode toggle widget
src/components/GamerHome.vue     — Extracted current Home content
src/components/EasyHome.vue      — Easy mode goal cards
src/components/ChatHome.vue      — Chat mode home wrapper
src/views/GoalDetail.vue         — Goal workflow wizard
src/views/Chat.vue               — Chat placeholder
```

### Modified Files (11)
```
src/types/api.ts            — Add UIMode type + mode field to UIData
src/router/index.ts         — Add goals/:goalId and chat routes
src/views/Dashboard.vue     — Computed nav items, ModeSwitcher in sidebar
src/views/Home.vue          — Mode dispatcher (GamerHome/EasyHome/ChatHome)
src/views/Settings.vue      — Interface Mode selection section
src/data/helpTree.ts        — Goals in Spotlight search
src/style.css               — Mode switcher, goal card, wizard CSS
src/stores/app.ts           — Sync mode from backend
src/api/rpc-client.ts       — setUIMode() RPC method
src/components/SpotlightSearch.vue — Visual indicator for goal items
mock-backend.js             — ui.set-mode handler
```
