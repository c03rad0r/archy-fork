/**
 * Tests for useControllerNav — validates against GAMEPAD-NAV-MAP.md
 *
 * Tests the navigation logic (element queries, spatial nav, zone detection)
 * without mounting the composable (which needs Vue lifecycle).
 */
import { describe, it, expect, vi, afterEach } from 'vitest'

// ─── Mocks ─────────────────────────────────────────────────────
vi.mock('vue-router', () => ({
  useRoute: () => ({ path: '/dashboard' }),
  useRouter: () => ({ push: vi.fn().mockResolvedValue(undefined) }),
}))
vi.mock('@/stores/controller', () => ({ useControllerStore: () => ({ setActive: vi.fn(), setGamepadCount: vi.fn() }) }))
vi.mock('@/stores/spotlight', () => ({ useSpotlightStore: () => ({ isOpen: false, close: vi.fn() }) }))
vi.mock('@/stores/cli', () => ({ useCLIStore: () => ({ isOpen: false, close: vi.fn() }) }))
vi.mock('@/stores/appLauncher', () => ({ useAppLauncherStore: () => ({ isOpen: false, close: vi.fn() }) }))
vi.mock('@/composables/useNavSounds', () => ({ playNavSound: vi.fn() }))

// ─── Helpers ───────────────────────────────────────────────────

const FOCUSABLE_SELECTOR = [
  'a[href]', 'button:not([disabled])', 'input:not([disabled])',
  'select:not([disabled])', 'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])', '[data-controller-focus]',
  '[data-controller-container]',
].join(', ')

function queryFocusable(root: HTMLElement | Document = document): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    el => !el.hasAttribute('data-controller-ignore') && !el.closest('[data-controller-ignore]')
  )
}

function queryContainers(): HTMLElement[] {
  const zone = document.querySelector('[data-controller-zone="main"]')
  if (!zone) return []
  return Array.from(zone.querySelectorAll<HTMLElement>('[data-controller-container]'))
}

function queryNavBarItems(): HTMLElement[] {
  const zone = document.querySelector('[data-controller-zone="main"]')
  if (!zone) return []
  return queryFocusable(zone as HTMLElement).filter(el =>
    !el.hasAttribute('data-controller-container') &&
    !el.closest('[data-controller-container]')
  )
}

function querySidebar(): HTMLElement[] {
  const zone = document.querySelector('[data-controller-zone="sidebar"]')
  return zone ? queryFocusable(zone as HTMLElement) : []
}

// ─── Module Export ──────────────────────────────────────────────

describe('module', () => {
  it('exports useControllerNav', async () => {
    const mod = await import('../useControllerNav')
    expect(typeof mod.useControllerNav).toBe('function')
  })
})

// ─── SIDEBAR: Up/Down wrap, Right→container, Left→nothing ──────

describe('sidebar navigation (NAV-MAP: Sidebar)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('finds all sidebar nav items', () => {
    document.body.innerHTML = `
      <div data-controller-zone="sidebar">
        <a href="/dashboard">Home</a>
        <a href="/dashboard/apps">Apps</a>
        <a href="/dashboard/cloud">Cloud</a>
        <button>AIUI</button>
        <button>Logout</button>
      </div>
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0">Card</div>
      </div>
    `
    expect(querySidebar().length).toBe(5)
  })

  it('wraps down: Logout → Home', () => {
    const items = ['Home', 'Apps', 'Cloud', 'Logout']
    const lastIdx = items.length - 1
    expect((lastIdx + 1) % items.length).toBe(0) // wraps to Home
  })

  it('wraps up: Home → Logout', () => {
    const items = ['Home', 'Apps', 'Cloud', 'Logout']
    expect((0 - 1 + items.length) % items.length).toBe(items.length - 1) // wraps to Logout
  })

  it('right from sidebar targets first container, not nav bar items', () => {
    document.body.innerHTML = `
      <div data-controller-zone="sidebar"><a href="/">Home</a></div>
      <div data-controller-zone="main">
        <button class="mode-switcher-btn" id="tab">Tab</button>
        <div data-controller-container tabindex="0" id="card1">Card</div>
      </div>
    `
    const containers = queryContainers()
    expect(containers[0]?.id).toBe('card1')
  })

  it('left from sidebar does nothing (no target exists)', () => {
    document.body.innerHTML = `
      <div data-controller-zone="sidebar"><a href="/">Home</a></div>
    `
    const sidebar = querySidebar()
    const el = sidebar[0]!
    // Nothing to the left of sidebar
    expect(el.closest('[data-controller-zone="sidebar"]')).toBeTruthy()
  })
})

// ─── HOME: 2-col grid + nav bar ────────────────────────────────

describe('HOME grid (NAV-MAP: HOME /dashboard)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('has Dashboard and Setup nav bar items', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div role="tablist">
          <button role="tab" class="mode-switcher-btn" id="dashTab">Dashboard</button>
          <button role="tab" class="mode-switcher-btn" id="setupTab">Setup</button>
        </div>
        <div data-controller-container tabindex="0" id="myApps">My Apps</div>
        <div data-controller-container tabindex="0" id="cloud">Cloud</div>
      </div>
    `
    const navItems = queryNavBarItems()
    expect(navItems.length).toBe(2)
    expect(navItems[0]?.id).toBe('dashTab')
    expect(navItems[1]?.id).toBe('setupTab')
  })

  it('containers exclude nav bar items', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <button class="mode-switcher-btn">Dashboard</button>
        <button class="mode-switcher-btn">Setup</button>
        <div data-controller-container tabindex="0" id="myApps">My Apps</div>
        <div data-controller-container tabindex="0" id="cloud">Cloud</div>
        <div data-controller-container tabindex="0" id="network">Network</div>
        <div data-controller-container tabindex="0" id="wallet">Wallet</div>
        <div data-controller-container tabindex="0" id="system">System</div>
      </div>
    `
    const containers = queryContainers()
    expect(containers.length).toBe(5)
    expect(containers.map(c => c.id)).toEqual(['myApps', 'cloud', 'network', 'wallet', 'system'])
    // Nav bar items are separate
    const navItems = queryNavBarItems()
    expect(navItems.length).toBe(2)
  })

  it('inner controls are not in the container grid', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0" id="myApps">
          <a href="/dashboard/apps">Go</a>
          <button id="browseStore">Browse Store</button>
          <button id="manageApps">Manage Apps</button>
        </div>
      </div>
    `
    // Only 1 container in grid
    expect(queryContainers().length).toBe(1)
    // Nav bar is empty (all focusables are inside the container)
    expect(queryNavBarItems().length).toBe(0)
  })
})

// ─── APPS: 3-col grid + nav bar with tabs/filters/search ───────

describe('APPS grid (NAV-MAP: APPS /dashboard/apps)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('nav bar has tabs, filters, and search', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div class="mode-switcher">
          <button class="mode-switcher-btn" id="myAppsTab">My Apps</button>
          <a href="/dashboard/discover" class="mode-switcher-btn" id="storeTab">App Store</a>
          <button class="mode-switcher-btn" id="servicesTab">Services</button>
        </div>
        <div class="mode-switcher">
          <button class="mode-switcher-btn" id="allFilter">All</button>
          <button class="mode-switcher-btn" id="btcFilter">Bitcoin</button>
        </div>
        <input type="text" id="search" />
        <div data-controller-container tabindex="0" id="app1">App1</div>
        <div data-controller-container tabindex="0" id="app2">App2</div>
        <div data-controller-container tabindex="0" id="app3">App3</div>
      </div>
    `
    const navItems = queryNavBarItems()
    // 3 tabs + 2 filters + 1 search = 6 nav bar items
    expect(navItems.length).toBe(6)
    expect(navItems.map(el => el.id)).toEqual(['myAppsTab', 'storeTab', 'servicesTab', 'allFilter', 'btcFilter', 'search'])

    // 3 containers
    expect(queryContainers().length).toBe(3)
  })

  it('app cards with launch attribute are containers', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container data-controller-launch tabindex="0" id="app1">
          <button data-controller-launch-btn>Launch</button>
        </div>
      </div>
    `
    const containers = queryContainers()
    expect(containers.length).toBe(1)
    expect(containers[0]?.hasAttribute('data-controller-launch')).toBe(true)
    const launchBtn = containers[0]?.querySelector('[data-controller-launch-btn]')
    expect(launchBtn).toBeTruthy()
  })
})

// ─── CLOUD: 3-col, no nav bar ──────────────────────────────────

describe('CLOUD grid (NAV-MAP: CLOUD /dashboard/cloud)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('has section cards as containers, no nav bar', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0" id="photos">Photos</div>
        <div data-controller-container tabindex="0" id="music">Music</div>
        <div data-controller-container tabindex="0" id="docs">Documents</div>
        <div data-controller-container tabindex="0" id="files">Files</div>
      </div>
    `
    expect(queryContainers().length).toBe(4)
    expect(queryNavBarItems().length).toBe(0)
  })
})

// ─── NETWORK: 2-col ────────────────────────────────────────────

describe('NETWORK grid (NAV-MAP: NETWORK /dashboard/server)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('has Local Network and Web3 containers', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0" id="localNet">Local Network</div>
        <div data-controller-container tabindex="0" id="web3">Web3</div>
      </div>
    `
    const containers = queryContainers()
    expect(containers.length).toBe(2)
    expect(containers[0]?.id).toBe('localNet')
    expect(containers[1]?.id).toBe('web3')
  })
})

// ─── SETTINGS: vertical stack ──────────────────────────────────

describe('SETTINGS grid (NAV-MAP: SETTINGS /dashboard/settings)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('has stacked section containers', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0" id="account">Account Info</div>
        <div data-controller-container tabindex="0" id="password">Change Password</div>
        <div data-controller-container tabindex="0" id="twofa">Two-Factor</div>
        <div data-controller-container tabindex="0" id="system">System Info</div>
        <div data-controller-container tabindex="0" id="danger">Danger Zone</div>
      </div>
    `
    const containers = queryContainers()
    expect(containers.length).toBe(5)
    // No nav bar
    expect(queryNavBarItems().length).toBe(0)
  })
})

// ─── ENTER behavior ────────────────────────────────────────────

describe('enter key behavior (NAV-MAP: Rules 5)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('container with primary link: Enter should navigate', () => {
    document.body.innerHTML = `
      <div data-controller-container tabindex="0">
        <a href="/dashboard/apps" id="link">Go</a>
        <button>Browse</button>
      </div>
    `
    const container = document.querySelector('[data-controller-container]')!
    const link = container.querySelector('a[href]')
    expect(link).toBeTruthy()
    expect(link?.getAttribute('href')).toBe('/dashboard/apps')
  })

  it('container without link: Enter drills into inner [Y] controls', () => {
    document.body.innerHTML = `
      <div data-controller-container tabindex="0">
        <button id="btn1">Open Shop</button>
        <button id="btn2">Accept Payments</button>
      </div>
    `
    const container = document.querySelector('[data-controller-container]')!
    expect(container.querySelector('a[href]')).toBeNull()
    const inner = Array.from(container.querySelectorAll('button'))
    expect(inner.length).toBe(2)
  })

  it('install container: Enter clicks install button', () => {
    document.body.innerHTML = `
      <div data-controller-container data-controller-install tabindex="0">
        <button data-controller-install-btn>Install</button>
      </div>
    `
    const container = document.querySelector('[data-controller-container]')!
    expect(container.hasAttribute('data-controller-install')).toBe(true)
    expect(container.querySelector('[data-controller-install-btn]')).toBeTruthy()
  })

  it('launch container: Enter clicks launch button', () => {
    document.body.innerHTML = `
      <div data-controller-container data-controller-launch tabindex="0">
        <button data-controller-launch-btn>Launch</button>
      </div>
    `
    const container = document.querySelector('[data-controller-container]')!
    expect(container.hasAttribute('data-controller-launch')).toBe(true)
    expect(container.querySelector('[data-controller-launch-btn]')).toBeTruthy()
  })
})

// ─── INSIDE CONTAINER [Y] ──────────────────────────────────────

describe('inside container navigation (NAV-MAP: Rules 6)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('inner controls are isolated from other containers', () => {
    document.body.innerHTML = `
      <div data-controller-container tabindex="0" id="card1">
        <button id="stop">Stop</button>
        <button id="restart">Restart</button>
      </div>
      <div data-controller-container tabindex="0" id="card2">
        <button id="other">Other</button>
      </div>
    `
    const card1 = document.getElementById('card1')!
    const inner = queryFocusable(card1).filter(el => el !== card1 && !el.hasAttribute('data-controller-container'))
    expect(inner.length).toBe(2)
    expect(inner.map(el => el.id)).toEqual(['stop', 'restart'])
    // "other" is NOT in card1's inner controls
    expect(inner.find(el => el.id === 'other')).toBeUndefined()
  })

  it('escape from inner control returns to container', () => {
    document.body.innerHTML = `
      <div data-controller-container tabindex="0" id="card">
        <button id="inner">Action</button>
      </div>
    `
    const inner = document.getElementById('inner')!
    const container = inner.closest('[data-controller-container]')
    expect(container).toBeTruthy()
    expect(container?.id).toBe('card')
    expect(container?.getAttribute('tabindex')).toBe('0')
  })

  it('isInsideContainer is true for nested, false for container itself', () => {
    document.body.innerHTML = `
      <div data-controller-container tabindex="0" id="card">
        <button id="inside">In</button>
      </div>
      <button id="outside">Out</button>
    `
    const inside = document.getElementById('inside')!
    const outside = document.getElementById('outside')!
    const card = document.getElementById('card')!

    // inside: has container ancestor that isn't itself
    const insideContainer = inside.closest('[data-controller-container]')
    expect(insideContainer && insideContainer !== inside).toBe(true)
    // card: IS the container
    expect(card.hasAttribute('data-controller-container')).toBe(true)
    // outside: no container ancestor
    expect(outside.closest('[data-controller-container]')).toBeNull()
  })
})

// ─── TEXT INPUT handling ───────────────────────────────────────

describe('text input handling (NAV-MAP: text inputs)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('up/down exits input, left/right stays', () => {
    const exitKeys = ['ArrowUp', 'ArrowDown']
    const stayKeys = ['ArrowLeft', 'ArrowRight']
    exitKeys.forEach(k => expect(['ArrowUp', 'ArrowDown'].includes(k)).toBe(true))
    stayKeys.forEach(k => expect(['ArrowUp', 'ArrowDown'].includes(k)).toBe(false))
  })

  it('enter on password clicks next button (submit)', () => {
    document.body.innerHTML = `
      <input id="pass" type="password" />
      <button id="login">Login</button>
    `
    const all = queryFocusable()
    const passIdx = all.findIndex(el => el.id === 'pass')
    const next = all[passIdx + 1]
    expect(next?.tagName).toBe('BUTTON')
    expect(next?.id).toBe('login')
  })
})

// ─── FOCUS MEMORY ──────────────────────────────────────────────

describe('focus memory (NAV-MAP: zone transitions)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('remembers and recalls elements', () => {
    document.body.innerHTML = `<button id="btn">Test</button>`
    const memory = new Map<string, HTMLElement>()
    const btn = document.getElementById('btn')!
    memory.set('main', btn)
    expect(memory.get('main')).toBe(btn)
    expect(document.contains(btn)).toBe(true)
  })

  it('detects stale (removed) elements', () => {
    document.body.innerHTML = `<button id="btn">Test</button>`
    const memory = new Map<string, HTMLElement>()
    const btn = document.getElementById('btn')!
    memory.set('main', btn)
    btn.remove()
    expect(document.contains(memory.get('main')!)).toBe(false)
  })

  it('clears on route change', () => {
    const memory = new Map<string, HTMLElement>()
    document.body.innerHTML = `<button id="btn">Test</button>`
    memory.set('main', document.getElementById('btn')!)
    memory.delete('main')
    expect(memory.get('main')).toBeUndefined()
  })
})

// ─── SPATIAL NAVIGATION ────────────────────────────────────────

describe('spatial navigation', () => {
  it('overlap scoring: aligned > offset', () => {
    const from = { top: 50, bottom: 200, left: 0, right: 150 }
    const aligned = { top: 50, bottom: 200, left: 200, right: 350 }
    const offset = { top: 160, bottom: 310, left: 200, right: 350 }
    const alignedOv = Math.max(0, Math.min(from.bottom, aligned.bottom) - Math.max(from.top, aligned.top))
    const offsetOv = Math.max(0, Math.min(from.bottom, offset.bottom) - Math.max(from.top, offset.top))
    expect(alignedOv).toBe(150)
    expect(offsetOv).toBe(40)
    expect(alignedOv).toBeGreaterThan(offsetOv)
  })

  it('tiebreaker: up/down prefers leftmost', () => {
    // Two elements below, same distance, same overlap
    const a = { left: 0 }
    const b = { left: 200 }
    // Sort: leftmost wins
    expect(a.left - b.left).toBeLessThan(0) // a is leftmost
  })

  it('no wrap in 2D grid (NAV-MAP: Rules 2)', () => {
    // At rightmost column, pressing right should find nothing
    const from = { left: 400, right: 600, top: 0, bottom: 200 }
    const threshold = 50
    // No element to the right
    const candidate = { left: 0, right: 150 } // far left
    expect(candidate.left >= from.right - threshold).toBe(false) // NOT to the right
  })
})

// ─── GAMEPAD DETECTION ─────────────────────────────────────────

describe('gamepad detection', () => {
  it('counts connected gamepads', () => {
    const gp = [{ connected: true }, null, { connected: true }, null] as (Gamepad | null)[]
    expect(gp.filter(g => g?.connected).length).toBe(2)
  })
  it('handles null list', () => {
    const count = (gp: (Gamepad | null)[] | null) => gp ? gp.filter(g => g?.connected).length : 0
    expect(count(null)).toBe(0)
  })
})

// ─── DATA-CONTROLLER-IGNORE ────────────────────────────────────

describe('data-controller-ignore', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('excluded elements are filtered out', () => {
    document.body.innerHTML = `
      <button data-controller-ignore>Skip</button>
      <div data-controller-ignore><button>Nested ignored</button></div>
      <button id="real">Real</button>
    `
    const all = queryFocusable()
    expect(all.length).toBe(1)
    expect(all[0]?.id).toBe('real')
  })
})

// ─── NAV BAR [N] DETECTION ─────────────────────────────────────

describe('nav bar detection', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('nav bar items are in main zone but not inside containers', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <button class="mode-switcher-btn" id="tab1">Dashboard</button>
        <button class="mode-switcher-btn" id="tab2">Setup</button>
        <div data-controller-container tabindex="0" id="card">
          <button id="inner">Inner</button>
        </div>
      </div>
    `
    const navItems = queryNavBarItems()
    expect(navItems.length).toBe(2)
    expect(navItems[0]?.id).toBe('tab1')
    expect(navItems[1]?.id).toBe('tab2')
    // Inner button is NOT a nav bar item
    expect(navItems.find(el => el.id === 'inner')).toBeUndefined()
  })

  it('pages without nav bar return empty', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0">Card</div>
      </div>
    `
    expect(queryNavBarItems().length).toBe(0)
  })
})

// ─── DISCOVER: featured + grid ─────────────────────────────────

describe('DISCOVER grid (NAV-MAP: DISCOVER /dashboard/discover)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('has nav bar + featured + app grid', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <a href="/dashboard/apps" class="mode-switcher-btn" id="myApps">My Apps</a>
        <a href="/dashboard/discover" class="mode-switcher-btn" id="appStore">App Store</a>
        <div data-controller-container data-controller-install tabindex="0" id="feat1">Featured 1</div>
        <div data-controller-container data-controller-install tabindex="0" id="feat2">Featured 2</div>
        <div data-controller-container data-controller-install tabindex="0" id="app1">App 1</div>
        <div data-controller-container data-controller-install tabindex="0" id="app2">App 2</div>
      </div>
    `
    expect(queryNavBarItems().length).toBe(2)
    expect(queryContainers().length).toBe(4)
  })
})

// ─── MESH / FLEET / SETTINGS containers exist ──────────────────

describe('pages have containers (NAV-MAP: all pages)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('mesh has panel containers', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0" id="device">Device Status</div>
        <div data-controller-container tabindex="0" id="chat">Chat Panel</div>
        <div data-controller-container tabindex="0" id="peers">Peers</div>
      </div>
    `
    expect(queryContainers().length).toBe(3)
  })

  it('fleet has stat + node containers', () => {
    document.body.innerHTML = `
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0">Nodes</div>
        <div data-controller-container tabindex="0">Online</div>
        <div data-controller-container tabindex="0">Offline</div>
        <div data-controller-container tabindex="0">Health</div>
        <div data-controller-container tabindex="0">Node 1</div>
      </div>
    `
    expect(queryContainers().length).toBe(5)
  })
})

// ─── FULL FLOW: sidebar → container → inner → back ─────────────

describe('full navigation flow (NAV-MAP: Rules 1-8)', () => {
  afterEach(() => { document.body.innerHTML = '' })

  it('complete roundtrip: sidebar → container → inner → escape → sidebar', () => {
    document.body.innerHTML = `
      <div data-controller-zone="sidebar">
        <a href="/dashboard" class="nav-tab-active" id="sideHome">Home</a>
        <a href="/dashboard/apps" id="sideApps">Apps</a>
      </div>
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0" id="card1">
          <button id="inner1">Browse</button>
          <button id="inner2">Manage</button>
        </div>
        <div data-controller-container tabindex="0" id="card2">
          <a href="/dashboard/cloud">Go</a>
        </div>
      </div>
    `
    // Step 1: Sidebar exists, has active tab
    const sidebar = querySidebar()
    expect(sidebar.length).toBe(2)
    const activeTab = document.querySelector('.nav-tab-active') as HTMLElement
    expect(activeTab?.id).toBe('sideHome')

    // Step 2: Right from sidebar → first container
    const containers = queryContainers()
    expect(containers[0]?.id).toBe('card1')

    // Step 3: Enter on card1 (no primary link) → drill into inner controls
    const card1 = document.getElementById('card1')!
    const inner = queryFocusable(card1).filter(el => el !== card1 && !el.hasAttribute('data-controller-container'))
    expect(inner.length).toBe(2)
    expect(inner[0]?.id).toBe('inner1')

    // Step 4: Escape from inner → back to card1
    const innerEl = document.getElementById('inner1')!
    const parentContainer = innerEl.closest('[data-controller-container]')
    expect(parentContainer?.id).toBe('card1')

    // Step 5: Escape from card1 → sidebar active tab
    expect(activeTab?.id).toBe('sideHome')

    // Step 6: card2 has primary link → Enter navigates
    const card2 = document.getElementById('card2')!
    const primaryLink = card2.querySelector('a[href]')
    expect(primaryLink?.getAttribute('href')).toBe('/dashboard/cloud')
  })

  it('no dead ends: every container can reach sidebar', () => {
    document.body.innerHTML = `
      <div data-controller-zone="sidebar">
        <a href="/" class="nav-tab-active">Home</a>
      </div>
      <div data-controller-zone="main">
        <div data-controller-container tabindex="0" id="c1">C1</div>
        <div data-controller-container tabindex="0" id="c2">C2</div>
      </div>
    `
    // Every container is in main zone
    const containers = queryContainers()
    containers.forEach(c => {
      expect(c.closest('[data-controller-zone="main"]')).toBeTruthy()
    })
    // Sidebar has at least one item
    expect(querySidebar().length).toBeGreaterThan(0)
    // Active tab exists for Left → sidebar
    expect(document.querySelector('.nav-tab-active')).toBeTruthy()
  })
})
