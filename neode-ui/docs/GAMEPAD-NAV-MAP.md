# Gamepad Navigation Map

Every arrow key, every position, every page.

`[C]` = Container (red tile, D-pad grid)
`[N]` = Nav bar item (secondary, reached via Up from top row)
`[Y]` = Inner control (entered via Enter on container, exited via Escape)
`[S]` = Sidebar item

---

## Sidebar (all pages)

Vertical list. Up/Down wrap. Right enters page. Left does nothing.

| Position   | Up         | Down       | Right          | Left    |
|------------|------------|------------|----------------|---------|
| Home       | Logout     | Apps       | First [C]      | nothing |
| Apps       | Home       | Cloud      | First [C]      | nothing |
| Cloud      | Apps       | Mesh       | First [C]      | nothing |
| Mesh       | Cloud      | Network    | First [C]      | nothing |
| Network    | Mesh       | Web5       | First [C]      | nothing |
| Web5       | Network    | Fleet      | First [C]      | nothing |
| Fleet      | Web5       | Settings   | First [C]      | nothing |
| Settings   | Fleet      | AIUI       | First [C]      | nothing |
| AIUI       | Settings   | Logout     | First [C]      | nothing |
| Logout     | AIUI       | Home       | First [C]      | nothing |

---

## HOME `/dashboard`

### Nav bar `[N]`

```
[N] Dashboard    [N] Setup
```

### Grid `[C]`

```
Row 1:  [C] My Apps       [C] Cloud
Row 2:  [C] Network       [C] Wallet
Row 3:  [C] System
Row 4:  [C] Quick Start   (full-width, if visible)
```

| Position      | Up           | Down         | Left       | Right    | Enter                     |
|---------------|--------------|--------------|------------|----------|---------------------------|
| [N] Dashboard | nothing      | My Apps      | nothing    | Setup    | Switch tab                |
| [N] Setup     | nothing      | My Apps      | Dashboard  | nothing  | Switch tab                |
| My Apps       | [N] bar      | Network      | Sidebar    | Cloud    | /dashboard/apps           |
| Cloud         | [N] bar      | Wallet       | My Apps    | nothing  | /dashboard/cloud          |
| Network       | My Apps      | System       | Sidebar    | Wallet   | /dashboard/server         |
| Wallet        | Cloud        | nothing      | Network    | nothing  | /dashboard/web5           |
| System        | Network      | Quick Start  | Sidebar    | nothing  | /dashboard/settings       |
| Quick Start   | System       | nothing      | Sidebar    | nothing  | Drill into [Y]            |

### Quick Start `[Y]` inner controls

```
[Y] Open a Shop    [Y] Accept Payments    [Y] File Browser
```

| Position         | Left             | Right            | Escape         |
|------------------|------------------|------------------|----------------|
| Open a Shop      | nothing          | Accept Payments  | Back to [C]    |
| Accept Payments  | Open a Shop      | File Browser     | Back to [C]    |
| File Browser     | Accept Payments  | nothing          | Back to [C]    |

---

## APPS `/dashboard/apps`

### Nav bar `[N]`

```
[N] My Apps  [N] App Store  [N] Services  |  [N] All  [N] Bitcoin  [N] Social (etc)  |  [N] Search
```

Three groups: page tabs, category filters (dynamic), search input.

| Position       | Up      | Down    | Left           | Right          | Enter              |
|----------------|---------|---------|----------------|----------------|--------------------|
| [N] My Apps    | nothing | App1    | nothing        | App Store      | Switch tab         |
| [N] App Store  | nothing | App1    | My Apps        | Services       | /dashboard/discover|
| [N] Services   | nothing | App1    | App Store      | All filter     | Switch tab         |
| [N] All        | nothing | App1    | Services       | Bitcoin (etc)  | Filter             |
| [N] Search     | nothing | App1    | last filter    | nothing        | Type text          |

### Grid `[C]` (3-col)

```
Row 1:  [C] App1    [C] App2    [C] App3
Row 2:  [C] App4    [C] App5    [C] App6
(etc)
```

| Position       | Up               | Down      | Left      | Right    | Enter           |
|----------------|------------------|-----------|-----------|----------|-----------------|
| App1 (row 1)   | [N] bar My Apps  | App4      | Sidebar   | App2     | Launch app      |
| App2 (row 1)   | [N] bar My Apps  | App5      | App1      | App3     | Launch app      |
| App3 (row 1)   | [N] bar My Apps  | App6      | App2      | nothing  | Launch app      |
| App4 (row 2)   | App1             | App7      | Sidebar   | App5     | Launch app      |
| App5 (row 2)   | App2             | App8      | App4      | App6     | Launch app      |
| App6 (row 2)   | App3             | App9      | App5      | nothing  | Launch app      |
| (etc)          | above            | below     | left/side | right    | Launch app      |

### App `[Y]` inner controls (if no launch action)

```
[Y] Stop    [Y] Restart    [Y] Uninstall
```

Escape exits back to [C] app card.

---

## CLOUD `/dashboard/cloud`

No nav bar.

### Grid `[C]` (3-col)

```
Row 1:  [C] Photos      [C] Music       [C] Documents
Row 2:  [C] Files        [C] Peer1       [C] Peer2 (etc)
```

| Position    | Up         | Down      | Left      | Right      | Enter             |
|-------------|------------|-----------|-----------|------------|-------------------|
| Photos      | nothing    | Files     | Sidebar   | Music      | Open section      |
| Music       | nothing    | Peer1     | Photos    | Documents  | Open section      |
| Documents   | nothing    | Peer2     | Music     | nothing    | Open section      |
| Files       | Photos     | nothing   | Sidebar   | Peer1      | Open section      |
| Peer1       | Music      | nothing   | Files     | Peer2      | Open peer files   |
| Peer2       | Documents  | nothing   | Peer1     | nothing    | Open peer files   |

---

## NETWORK `/dashboard/server`

No nav bar.

### Grid `[C]`

```
Row 1:  [C] Quick Actions (full-width, contains Restart/Check Tor/Auto-Sync/Logs)
Row 2:  [C] Local Network       [C] Web3
Row 3:  [C] Network Interfaces  [C] Tor Services
```

| Position             | Up              | Down                | Left          | Right             | Enter            |
|----------------------|-----------------|---------------------|---------------|-------------------|------------------|
| Quick Actions        | nothing         | Local Network       | Sidebar       | nothing           | Drill into [Y]   |
| Local Network        | Quick Actions   | Network Interfaces  | Sidebar       | Web3              | Drill into [Y]   |
| Web3                 | Quick Actions   | Tor Services        | Local Network | nothing           | Drill into [Y]   |
| Network Interfaces   | Local Network   | nothing             | Sidebar       | Tor Services      | Drill into [Y]   |
| Tor Services         | Web3            | nothing             | Net Interfaces| nothing           | Drill into [Y]   |

---

## WEB5 `/dashboard/web5`

No nav bar. Containers from child components stacked vertically + side-by-side.

### Grid `[C]`

```
Row 1:  [C] Action1  [C] Action2  [C] Action3  [C] Action4  [C] Action5  [C] Action6
Row 2:  [C] Wallet              [C] Domains
Row 3:  [C] Nostr Relays        [C] Node Visibility
Row 4:  [C] Connected Nodes
```

Standard spatial grid nav. Left from leftmost = Sidebar. Enter = drill into [Y] controls.

---

## DISCOVER `/dashboard/discover`

### Nav bar `[N]`

```
[N] My Apps  [N] App Store  [N] Services  |  [N] Discover  [N] Categories...  |  [N] Search
```

Down from nav bar → first container. Nav bar remembers last-focused tab — Up from cards returns to it.

### Grid `[C]`

```
Featured (2-col):  [C] Featured1    [C] Featured2
All Apps (3-col):  [C] App1         [C] App2         [C] App3
                   [C] App4         [C] App5         [C] App6
                   (etc)
```

Cards use same style as My Apps: `glass-card transition-all hover:-translate-y-1`.

| Position     | Up             | Down     | Left      | Right      | Enter              |
|--------------|----------------|----------|-----------|------------|--------------------|
| [N] tabs     | nothing        | Featured1| left tab  | right tab  | Switch/filter      |
| Featured1    | remembered [N] | App1     | Sidebar   | Featured2  | View details       |
| App1         | Featured1      | App4     | Sidebar   | App2       | Install / details  |
| (etc)        | above          | below    | left/side | right      | Install / details  |

---

## MESH `/dashboard/mesh`

### Grid `[C]`

```
Left column:   [C] Device Status    [C] Actions    [C] Peers List
Right column:  [C] Chat Panel       [C] Tools (Bitcoin/Dead Man/Map)
```

| Position        | Up            | Down      | Left      | Right       | Enter                          |
|-----------------|---------------|-----------|-----------|-------------|--------------------------------|
| Device Status   | nothing       | Actions   | Sidebar   | Chat Panel  | Drill into [Y]                 |
| Actions         | Device Status | Peers     | Sidebar   | Chat Panel  | Drill into [Y] buttons         |
| Peers List      | Actions       | nothing   | Sidebar   | Chat Panel  | Drill into peer rows           |
| Chat Panel      | nothing       | Tools     | Device    | nothing     | Drill into [Y]                 |
| Tools           | Chat Panel    | nothing   | Peers     | nothing     | Drill into [Y]                 |

**Chat flow:** Select a peer/channel (Enter on peer row) → focus auto-jumps to message input → type → Enter sends.

---

## FLEET `/dashboard/fleet`

### Grid `[C]`

```
Row 1:  [C] Nodes    [C] Online    [C] Offline    [C] Health
Row 2:  [C] Node1    [C] Node2     [C] Node3      (etc)
```

Spatial grid nav. Enter = view node details.

---

## SETTINGS `/dashboard/settings`

**Mixed page:** Two containers ([C] Server Name, [C] Interface Mode) + linear buttons.
Up/Down steps through elements. Right navigates paired items on the same row. Left → sidebar.
Enter on containers → drill in. Enter on buttons → activate. Escape → exit container / sidebar.

`[C]` = Container  `[B]` = Button  `[I]` = Input  `[T]` = Toggle

### Account Section (glass-card)

```
 1. [C] Server Name              → Enter: edit name, Enter: save, Escape: cancel
    [B] What's New               → right of Server Name
 2. [B] Copy DID
 3. [B] Copy Onion Address
 4. [B] Change Password          → opens modal
 5. [B] Enable 2FA / Disable 2FA → opens modal
 6. [B] Logout
```

### System Section

```
 7. [C] Interface Mode           → Enter: drill in, Left/Right between Easy/Gamer/Chat, Enter: select, Escape: exit
    [B] Language buttons          → below Interface Mode
 8. [B] Login with Claude        → opens modal
 9. [T] Enable All (AI data)     + per-category [T] toggles
10. [B] Manage Updates
11. [I] Webhook URL
12. [I] Webhook Secret
13. [T] Container Crash  [T] Update Available
14. [T] Disk Space Warning  [T] Backup Complete
15. [B] Save Configuration  [B] Send Test
16. [T] Enable Beta Telemetry
17. [B] Create Backup
18. [B] Export Channel Backup
19. [B] Network Diagnostics      → navigates to /dashboard/server
20. [B] Reboot                   → opens confirm modal
21. [B] Factory Reset            → opens confirm modal
```

| Position                  | Up          | Down        | Left          | Right          | Enter              |
|---------------------------|-------------|-------------|---------------|----------------|--------------------|
| 1. Server Name            | nothing     | Copy DID    | Sidebar       | What's New     | Edit name          |
| 1b. What's New            | nothing     | Copy DID    | Server Name   | nothing        | Show release notes |
| 2. Copy DID               | Server Name | Copy Onion  | Sidebar       | nothing        | Copy to clipboard  |
| 3. Copy Onion             | Copy DID    | Change PW   | Sidebar       | nothing        | Copy to clipboard  |
| 4. Change Password        | Copy Onion  | Enable 2FA  | Sidebar       | nothing        | Open modal         |
| 5. Enable 2FA             | Change PW   | Logout      | Sidebar       | nothing        | Open modal         |
| 6. Logout                 | Enable 2FA  | Language    | Sidebar       | nothing        | Logout             |
| 7. Language               | Logout      | Claude Login| Sidebar       | nothing        | Select language    |
| 8. Login with Claude      | Language    | AI toggles  | Sidebar       | nothing        | Open modal         |
| 9. AI toggles (each row)  | above       | below       | Sidebar       | next toggle    | Toggle on/off      |
| 10. Manage Updates        | AI toggles  | Webhook URL | Sidebar       | nothing        | Open updates       |
| 11. Webhook URL           | Updates     | Secret      | Sidebar       | nothing        | Edit field         |
| 12. Secret                | Webhook URL | Crash toggle| Sidebar       | nothing        | Edit field         |
| 13a. Container Crash      | Secret      | Disk Space  | Sidebar       | Update Avail   | Toggle on/off      |
| 13b. Update Available     | Secret      | Backup Done | Container Crash| nothing       | Toggle on/off      |
| 14a. Disk Space Warning   | Crash       | Save Config | Sidebar       | Backup Done    | Toggle on/off      |
| 14b. Backup Complete      | Update Avail| Send Test   | Disk Space    | nothing        | Toggle on/off      |
| 15a. Save Configuration   | Disk Space  | Telemetry   | Sidebar       | Send Test      | Save               |
| 15b. Send Test            | Backup Done | Telemetry   | Save Config   | nothing        | Send test webhook  |
| 16. Telemetry             | Save/Test   | Create Bkup | Sidebar       | nothing        | Toggle on/off      |
| 17. Create Backup         | Telemetry   | Export Chan | Sidebar       | nothing        | Open modal         |
| 18. Export Channel        | Create Bkup | Net Diag    | Sidebar       | nothing        | Export             |
| 19. Network Diagnostics   | Export Chan | Reboot      | Sidebar       | nothing        | → /dashboard/server|
| 20. Reboot                | Net Diag    | Factory Rst | Sidebar       | nothing        | Open confirm       |
| 21. Factory Reset         | Reboot      | nothing     | Sidebar       | nothing        | Open confirm       |

---

## LOGIN `/login`

No sidebar, no grid. Three modes on the same route.
`[B]` = Button  `[I]` = Input field  `[L]` = Link

### Set Password (first visit after onboarding)

Auto-focus: `[I] Password`

```
[I] Password
[I] Confirm Password
[B] Set Password
[L] Replay Intro    [L] Restart Onboarding
```

| Position              | Up                  | Down                | Left              | Right               | Enter              |
|-----------------------|---------------------|---------------------|-------------------|---------------------|--------------------|
| [I] Password          | nothing             | [I] Confirm         | nothing           | nothing             | Type / Down        |
| [I] Confirm           | [I] Password        | [B] Set Password    | nothing           | nothing             | Type / Down        |
| [B] Set Password      | [I] Confirm         | [L] Replay Intro    | nothing           | nothing             | Submit             |
| [L] Replay Intro      | [B] Set Password    | nothing             | nothing           | [L] Restart         | Replay intro       |
| [L] Restart           | [B] Set Password    | nothing             | [L] Replay Intro  | nothing             | Restart onboarding |

### Normal Login

Auto-focus: `[I] Password`

```
[I] Password
[B] Login
[L] Replay Intro    [L] Restart Onboarding
```

| Position              | Up               | Down             | Left              | Right               | Enter         |
|-----------------------|------------------|------------------|-------------------|---------------------|---------------|
| [I] Password          | nothing          | [B] Login        | nothing           | nothing             | Type / Down   |
| [B] Login             | [I] Password     | [L] Replay Intro | nothing           | nothing             | Submit        |
| [L] Replay Intro      | [B] Login        | nothing          | nothing           | [L] Restart         | Replay intro  |
| [L] Restart           | [B] Login        | nothing          | [L] Replay Intro  | nothing             | Restart       |

### TOTP Verification (after password accepted)

Auto-focus: `[I] TOTP Code`

```
[I] TOTP Code
[B] Verify
[L] Use Backup Code
```

| Position              | Up               | Down             | Left    | Right   | Enter              |
|-----------------------|------------------|------------------|---------|---------|--------------------|
| [I] TOTP Code         | nothing          | [B] Verify       | nothing | nothing | Type / Down        |
| [B] Verify            | [I] TOTP Code    | [L] Backup Code  | nothing | nothing | Submit             |
| [L] Use Backup Code   | [B] Verify       | nothing          | nothing | nothing | Toggle backup mode |

---

## ONBOARDING `/onboarding/*`

No sidebar, no grid. Sequential wizard screens.
`[B]` = Button  `[I]` = Input field  `[C]` = Selectable card  `[L]` = Link

**Global onboarding rules:**
- No sidebar or nav bar on any onboarding screen.
- First interactive element auto-focused on each screen (inputs when present, otherwise primary button).
- B button (Escape) = go back to previous onboarding step (where applicable).
- D-pad Up/Down **always** moves between focusable elements — inputs are never trapping. Up/Down exits a focused input to the adjacent element.
- Enter on an input = submit if it's the last field, otherwise move to next field.
- Enter activates the focused element.

---

### INTRO `/onboarding/intro`

Default focus: `[B] Unlock`

```
[B] Unlock your sovereignty
[L] Restore from backup
```

| Position          | Up              | Down            | Left    | Right   | Enter                       |
|-------------------|-----------------|-----------------|---------|---------|------------------------------|
| [B] Unlock        | nothing         | [L] Restore     | nothing | nothing | → /onboarding/path           |
| [L] Restore       | [B] Unlock      | nothing         | nothing | nothing | Show restore panel           |

#### Restore Panel `[Y]` (shown after activating Restore link)

```
[I] File picker
[I] Passphrase
[B] Cancel    [B] Restore
```

| Position          | Up              | Down            | Left       | Right      | Enter              | Escape         |
|-------------------|-----------------|-----------------|------------|------------|--------------------|----------------|
| [I] File picker   | nothing         | [I] Passphrase  | nothing    | nothing    | Open file dialog   | Close panel    |
| [I] Passphrase    | [I] File picker | [B] Cancel      | nothing    | nothing    | Type / Down        | Close panel    |
| [B] Cancel        | [I] Passphrase  | nothing         | nothing    | [B] Restore| Close panel        | Close panel    |
| [B] Restore       | [I] Passphrase  | nothing         | [B] Cancel | nothing    | Submit restore     | Close panel    |

---

### PATH `/onboarding/path`

Default focus: `[C] Fresh Start`

```
[C] Fresh Start    [C] Restore (disabled)    [C] Connect (disabled)
[B] Continue
```

| Position            | Up              | Down          | Left              | Right             | Enter                  |
|---------------------|-----------------|---------------|-------------------|-------------------|------------------------|
| [C] Fresh Start     | nothing         | [B] Continue  | nothing           | [C] Restore       | Select option          |
| [C] Restore         | nothing         | [B] Continue  | [C] Fresh Start   | [C] Connect       | nothing (disabled)     |
| [C] Connect         | nothing         | [B] Continue  | [C] Restore       | nothing           | nothing (disabled)     |
| [B] Continue        | [C] Fresh Start | nothing       | nothing           | nothing           | → /login (complete)    |

---

### OPTIONS `/onboarding/options`

Default focus: `[C] Sovereignty`

```
Row 1:  [C] Sovereignty     [C] Commerce       [C] Projects
Row 2:  [C] Transmitter     [C] Hoster         [C] AI
[B] Continue
```

| Position            | Up               | Down             | Left             | Right            | Enter              |
|---------------------|------------------|------------------|------------------|------------------|--------------------|
| [C] Sovereignty     | nothing          | [C] Transmitter  | nothing          | [C] Commerce     | nothing (display)  |
| [C] Commerce        | nothing          | [C] Hoster       | [C] Sovereignty  | [C] Projects     | nothing (display)  |
| [C] Projects        | nothing          | [C] AI           | [C] Commerce     | nothing          | nothing (display)  |
| [C] Transmitter     | [C] Sovereignty  | [B] Continue     | nothing          | [C] Hoster       | nothing (display)  |
| [C] Hoster          | [C] Commerce     | [B] Continue     | [C] Transmitter  | [C] AI           | nothing (display)  |
| [C] AI              | [C] Projects     | [B] Continue     | [C] Hoster       | nothing          | nothing (display)  |
| [B] Continue        | [C] Transmitter  | nothing          | nothing          | nothing          | → /onboarding/did  |

---

### DID `/onboarding/did`

**Loading state:** No interactive elements. Auto-advances when generation completes.

**After generation:**

Default focus: `[B] Continue`

```
[B] Copy DID
[B] Copy Nostr (if available)
[B] Continue
```

| Position            | Up               | Down             | Left    | Right   | Enter                       |
|---------------------|------------------|------------------|---------|---------|-----------------------------|
| [B] Copy DID        | nothing          | [B] Copy Nostr   | nothing | nothing | Copy to clipboard           |
| [B] Copy Nostr      | [B] Copy DID     | [B] Continue     | nothing | nothing | Copy to clipboard           |
| [B] Continue        | [B] Copy Nostr   | nothing          | nothing | nothing | → /onboarding/identity      |

If no Nostr ID: `[B] Copy DID` → Down → `[B] Continue` directly.

---

### IDENTITY `/onboarding/identity`

Auto-focus: `[I] Name`

```
[I] Identity Name
[C] Personal    [C] Business    [C] Anonymous
[B] Continue
```

| Position            | Up               | Down             | Left            | Right           | Enter                       |
|---------------------|------------------|------------------|-----------------|-----------------|-----------------------------|
| [I] Name            | nothing          | [C] Personal     | nothing         | nothing         | Type / Down                 |
| [C] Personal        | [I] Name         | [B] Continue     | nothing         | [C] Business    | Select purpose              |
| [C] Business        | [I] Name         | [B] Continue     | [C] Personal    | [C] Anonymous   | Select purpose              |
| [C] Anonymous       | [I] Name         | [B] Continue     | [C] Business    | nothing         | Select purpose              |
| [B] Continue        | [C] Personal     | nothing          | nothing         | nothing         | → /onboarding/backup        |

---

### BACKUP `/onboarding/backup`

Auto-focus: `[I] Passphrase`

```
[I] Passphrase
[B] Download Backup
[B] Continue (disabled until downloaded)
```

| Position            | Up               | Down             | Left    | Right   | Enter                       |
|---------------------|------------------|------------------|---------|---------|-----------------------------|
| [I] Passphrase      | nothing          | [B] Download     | nothing | nothing | Type / Down                 |
| [B] Download        | [I] Passphrase   | [B] Continue     | nothing | nothing | Create & download backup    |
| [B] Continue        | [B] Download     | nothing          | nothing | nothing | → /onboarding/verify        |

`[B] Continue` disabled (skip focus) until backup downloaded.

---

### VERIFY `/onboarding/verify`

**Phase 1 — Signing:**

Default focus: `[B] Sign Challenge`

```
[B] Sign Challenge
```

| Position             | Up      | Down    | Left    | Right   | Enter                  |
|----------------------|---------|---------|---------|---------|------------------------|
| [B] Sign Challenge   | nothing | nothing | nothing | nothing | Sign crypto challenge  |

**Phase 2 — After verification:**

Default focus: `[B] Finish`

```
[B] Finish
```

| Position    | Up      | Down    | Left    | Right   | Enter                       |
|-------------|---------|---------|---------|---------|------------------------------|
| [B] Finish  | nothing | nothing | nothing | nothing | → /onboarding/done           |

---

### DONE `/onboarding/done`

Default focus: `[B] Set Password`

```
[C] Identity    [C] Backup    [C] Ready
[B] Set Password
```

| Position            | Up           | Down             | Left          | Right         | Enter                |
|---------------------|--------------|------------------|---------------|---------------|----------------------|
| [C] Identity        | nothing      | [B] Set Password | nothing       | [C] Backup    | nothing (display)    |
| [C] Backup          | nothing      | [B] Set Password | [C] Identity  | [C] Ready     | nothing (display)    |
| [C] Ready           | nothing      | [B] Set Password | [C] Backup    | nothing       | nothing (display)    |
| [B] Set Password    | [C] Identity | nothing          | nothing       | nothing       | → /login             |

---

## Onboarding & Login Rules

1. No sidebar or nav bar — linear wizard flow.
2. First interactive element auto-focused (input fields when present, otherwise primary button).
3. D-pad Up/Down **always** moves between focusable elements — inputs are never trapping. You can always D-pad out of a focused field.
4. Left/Right for horizontal card rows only.
5. Disabled elements are skipped in focus order.
6. B button (Escape) navigates back one onboarding step.
7. Enter on input: submits if last field, otherwise advances to next field.
8. No wrap — edges are dead stops.
9. No dead ends — every screen has a forward action.

---

## Rules

1. Sidebar: Up/Down wrap. Right → first [C]. Left → nothing.
2. Grid: arrows move between [C] spatially. No wrap at edges.
3. Left from leftmost [C] → Sidebar active tab.
4. Up from top-row [C] → [N] nav bar (if page has one), else nothing.
5. Enter on [C]: has link → navigate. No link → drill into [Y].
6. Inside [Y]: arrows move between inner controls. Escape → back to [C].
7. Escape from [C] → Sidebar.
8. No dead ends.

---

## Implementation Notes (for future sessions)

### Key files
- **Navigation logic**: `neode-ui/src/composables/useControllerNav.ts`
- **Controller store**: `neode-ui/src/stores/controller.ts`
- **Nav sounds**: `neode-ui/src/composables/useNavSounds.ts`
- **Focus styles**: `neode-ui/src/style.css` (lines ~53-142, search `focus-visible`)

### Data attributes
| Attribute | Purpose |
|-----------|---------|
| `data-controller-zone="main"` | Main content area (`<main>` in Dashboard.vue) |
| `data-controller-zone="sidebar"` | Sidebar nav |
| `data-controller-container` + `tabindex="0"` | Focusable card tile — gamepad can land on it, Enter drills in |
| `data-controller-install` | Container has Install button (Enter prioritizes it) |
| `data-controller-launch` | Container has Launch button (Enter prioritizes it) |
| `data-controller-install-btn` | The actual Install button inside a container |
| `data-controller-launch-btn` | The actual Launch button inside a container |
| `data-controller-ignore` | Skip element and descendants from gamepad nav |
| `tabindex="-1"` | Remove from gamepad focus order (used on ToggleSwitch) |

### Focus memory keys
| Key | Purpose | Cleared on |
|-----|---------|------------|
| `sidebar` | Last sidebar item focused | never (persists) |
| `main` | Last container/element in main zone | route change |
| `navBar` | Last nav bar tab (for Up return from containers) | route change |

### Navigation handler order (handleKeyDown)
1. **Text inputs** — special handling (Enter submits, Up/Down exits field)
2. **Escape** — close overlays → exit inner controls → exit to sidebar → back on detail pages
3. **Enter** — container actions (install/launch/link/inner) → regular click
4. **Sidebar** — Up/Down wrap, Right → main (containers or first focusable)
5. **Inside container** — arrows move between inner controls, can't leave via arrows
6. **Nav bar items** — Left/Right between tabs, Down/Up to nearest focusable (containers + buttons)
7. **Main zone** — spatial nav through containers + standalone focusables, fallbacks for edges

### Mixed pages (containers + standalone buttons, e.g. Settings)
- `isNavBarItem()` returns false on container-free pages (lets main zone handler do linear nav)
- Both nav bar handler and main zone handler search containers + standalone focusables together
- This prevents "jumping" where Down skips standalone buttons to reach the next container
- The filter `el.hasAttribute('data-controller-container') || !el.closest('[data-controller-container]')` excludes inner buttons

### Container-free pages (e.g. Settings if all containers removed)
- Sidebar → Right: checks `zone.querySelector('[data-controller-container]')` — if none found, focuses first focusable immediately (no 1s poll delay)
- `isNavBarItem()` returns false (prevents nav bar handler from catching everything)
- Main zone handler's spatial nav through all focusables handles Up/Down/Left/Right

### ToggleSwitch component
- Has `tabindex="-1"` and `data-controller-ignore` — invisible to gamepad nav
- Parent button handles the toggle click, so the switch doesn't need its own focus
- Without this, nav gets stuck bouncing between parent button and toggle switch

### Focus glow styles (Chromium gotchas)
- `box-shadow: 0 0 0 Npx` (spread-based ring) does NOT follow `border-radius` on composited layers (`translateZ(0)`)
- `outline` doesn't follow `border-radius` in Chrome < 94
- Safe approach: use blurred `box-shadow` (`0 0 6px 2px`) or `border-color` change for focus rings
- All `[data-controller-container]` have `outline: none !important` to kill browser defaults
- Cards use `glass-card transition-all hover:-translate-y-1` for consistent hover/focus lift

### Mesh chat auto-focus
- `openChat()`, `openChannelChat()`, `openArchChannel()` all call `nextTick(() => chatInputEl.value?.focus())`
- Message input has `@keydown.enter.exact.prevent="handleSendMessage"` — Enter sends immediately
- Ref: `chatInputEl` on the `<input>` element in Mesh.vue
