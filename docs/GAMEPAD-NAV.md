# Gamepad / Controller Navigation Map

## Global Controls

| Button | Action |
|--------|--------|
| D-pad Up/Down | Navigate between items |
| D-pad Left | Go to sidebar (from any page) |
| D-pad Right | Enter main content from sidebar |
| Enter (A) | Activate / click focused element |
| Escape (B) | Go back one level (inner → container → sidebar → detail page back) |

## Navigation Layers

```
SIDEBAR ──Right──► CONTAINERS (or NAV BAR) ──Enter──► INNER CONTROLS
   ▲                      ▲                                  │
   └──Escape──────────────┘◄─────────Escape──────────────────┘
```

### Sidebar
- **Up/Down**: Move between sidebar items (wraps), auto-navigates links
- **Right**: Jump to main content (first container, or first button on container-free pages)
- **Left**: Nothing

### Nav Bar (mode-switcher tabs, category buttons)
- **Left/Right**: Move between tabs
- **Down**: Jump to first container below (remembers which tab for Up return)
- **Up**: Nothing (Escape to go to sidebar)
- **Left from leftmost**: Go to sidebar

### Container Grid (card tiles on most pages)
- **Arrows**: Spatial nav between containers
- **Enter**: Activate primary action (Install/Launch/navigate) or enter inner controls
- **Escape**: Go to sidebar
- **Left from leftmost**: Go to sidebar
- **Up from top row**: Return to remembered nav bar tab, or spatial to nearest nav item

### Inside Container (inner buttons after Enter)
- **Arrows**: Move between inner controls
- **Escape**: Exit back to the container tile

### Text Inputs
- **Up/Down**: Exit field, navigate spatially
- **Enter**: Submit (click adjacent button)
- **Left/Right**: Cursor movement (exit at edges)

### Container-Free Pages (Settings)
- **Right from sidebar**: Focus first button immediately (no 1s poll delay)
- **Up/Down**: Linear navigation through all buttons/toggles
- **Left**: Go to sidebar
- **Escape**: Go to sidebar

---

## Per-Page Mappings

### Home (`/dashboard`)
Container grid. Dashboard info cards.

### My Apps (`/dashboard/apps`)
| # | Element | Type |
|---|---------|------|
| Nav | My Apps / App Store / Services tabs | Nav bar (Left/Right) |
| 1–N | App cards (grid) | Containers — Enter to view details, inner Launch/Stop/Restart buttons |

### App Store / Discover (`/dashboard/discover`)
| # | Element | Type |
|---|---------|------|
| Nav | My Apps / App Store / Services tabs | Nav bar (Left/Right) |
| 1–2 | Sovereignty Stack featured cards | Containers (`glass-card transition-all hover:-translate-y-1`) |
| 3–N | All Applications grid cards | Containers — Enter for details, inner Install/Launch buttons |

### Network (`/dashboard/server`)
| # | Element | Type |
|---|---------|------|
| 1 | Quick Actions card | Single container — Enter to access Restart/Check Tor/View Logs buttons |
| 2 | Local Network card | Container |
| 3 | Web3 card | Container |
| 4 | Network Interfaces card | Container |
| 5 | Tor Services card | Container |

### Mesh (`/dashboard/mesh`)
| # | Element | Type |
|---|---------|------|
| 1 | Device status card | Container (left column) |
| 2 | Actions row (Enable/Broadcast/Off-Grid/Refresh) | Container |
| 3 | Peers list card | Container — Enter peer to open chat, inner peer items navigable |
| 4 | Chat panel | Container (right column) — message input + send |
| 5+ | Tool panels (Bitcoin/Dead Man/Map) | Containers |

**Chat flow**: Select peer (Enter) → focus auto-jumps to message input → type → Enter sends.

### Cloud (`/dashboard/cloud`)
Container grid. Folder/file cards.

### Settings (`/dashboard/settings`)
**Container-free page** — linear button navigation, no containers.

| # | Element | Section |
|---|---------|---------|
| 1 | Server Name input + save | Account Info |
| 2 | What's New button | Account Info |
| 3 | Copy DID button | Account Info |
| 4 | Copy Onion Address button | Account Info |
| 5 | Change Password button | Account → opens modal |
| 6 | Enable 2FA / Disable 2FA button | Account |
| 7 | Logout button | Account |
| 8 | Language selector buttons | Interface Mode |
| 9 | Login with Claude button | Claude Auth |
| 10 | Enable All / toggle per-category | AI Data Access |
| 11 | Manage Updates button | System Updates |
| 12 | Webhook URL input | Webhooks |
| 13 | Secret input | Webhooks |
| 14 | Container Crash / Update Available toggles | Webhooks |
| 15 | Disk Space Warning / Backup Complete toggles | Webhooks |
| 16 | Save Configuration / Send Test buttons | Webhooks |
| 17 | Enable Beta Telemetry button | Telemetry |
| 18 | Create Backup button | Backup |
| 19 | Export Channel Backup button | Backup |
| 20 | Network Diagnostics button | Danger Zone |
| 21 | Reboot button | Danger Zone → confirms with modal |
| 22 | Factory Reset button | Danger Zone → confirms with modal |

### Monitoring (`/dashboard/monitoring`)
Container grid. Stats/chart cards.

---

## Focus Memory

| Key | Remembers | Used When |
|-----|-----------|-----------|
| `sidebar` | Last sidebar item | Returning to sidebar via Escape/Left |
| `main` | Last focused container | Re-entering main zone |
| `navBar` | Last focused tab/button | Up from container returns to same tab |

All focus memory is cleared on route change.

## Data Attributes

| Attribute | Purpose |
|-----------|---------|
| `data-controller-zone="main"` | Main content area (on `<main>`) |
| `data-controller-zone="sidebar"` | Sidebar navigation |
| `data-controller-container` | Focusable card/tile (with `tabindex="0"`) |
| `data-controller-install` | Container has an Install button (Enter prioritizes it) |
| `data-controller-launch` | Container has a Launch button (Enter prioritizes it) |
| `data-controller-install-btn` | The actual Install button inside a container |
| `data-controller-launch-btn` | The actual Launch button inside a container |
| `data-controller-ignore` | Skip this element and descendants from navigation |
| `data-controller-focus` | Make non-standard element focusable |

## Implementation

- **File**: `neode-ui/src/composables/useControllerNav.ts`
- **Store**: `neode-ui/src/stores/controller.ts` (tracks active state + gamepad count)
- **Sounds**: `neode-ui/src/composables/useNavSounds.ts` (move/action/back)
- **Spatial nav**: `findNearestInDirection()` — filters by direction, scores by overlap + distance
