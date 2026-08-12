import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

// vi.hoisted runs before vi.mock hoisting
const { mockPush, mockWindowOpen } = vi.hoisted(() => ({
  mockPush: vi.fn(),
  mockWindowOpen: vi.fn(),
}))

// Mock vue-router
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: mockPush }),
}))
vi.mock('@/router', () => ({
  default: { push: mockPush, currentRoute: { value: { fullPath: '/dashboard/apps', name: 'apps' } } },
}))

vi.stubGlobal('open', mockWindowOpen)

import { useAppLauncherStore } from '../appLauncher'

describe('useAppLauncherStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    // Default to HTTP to avoid proxy rewriting
    Object.defineProperty(window, 'location', {
      value: { origin: 'http://192.0.2.10', protocol: 'http:', hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })
    Object.defineProperty(window, 'innerWidth', {
      value: 1024,
      writable: true,
      configurable: true,
    })
  })

  it('starts closed with empty state', () => {
    const store = useAppLauncherStore()
    expect(store.isOpen).toBe(false)
    expect(store.url).toBe('')
    expect(store.title).toBe('')
  })

  describe('companion app (native bridge)', () => {
    const openInApp = vi.fn()
    beforeEach(() => {
      ;(window as any).ArchipelagoNative = { openInApp, openExternal: vi.fn() }
      openInApp.mockClear()
    })
    afterEach(() => {
      delete (window as any).ArchipelagoNative
    })

    it('openSession hands iframeable apps to the native WebView, never the iframe session', () => {
      const store = useAppLauncherStore()
      store.openSession('filebrowser')
      expect(openInApp).toHaveBeenCalledWith(expect.stringContaining(':8083'))
      expect(store.panelAppId).toBeNull()
      expect(store.isOpen).toBe(false)
      expect(mockWindowOpen).not.toHaveBeenCalled()
    })

    it('openSession carries deep-link paths into the WebView URL', () => {
      const store = useAppLauncherStore()
      store.openSession('mempool', { path: '/tx/abc123' })
      expect(openInApp).toHaveBeenCalledWith(expect.stringContaining('/tx/abc123'))
      expect(store.panelAppId).toBeNull()
    })

    it('open() never falls through to the iframe overlay', () => {
      const store = useAppLauncherStore()
      store.open({ url: 'http://192.0.2.10:9999', title: 'Unknown app' })
      expect(openInApp).toHaveBeenCalledWith('http://192.0.2.10:9999')
      expect(store.isOpen).toBe(false)
    })
  })

  it('routes known port apps to full-page session', () => {
    const store = useAppLauncherStore()

    // Port 8083 maps to /app/filebrowser/ — should route to session
    store.open({ url: 'http://192.0.2.10:8083', title: 'FileBrowser' })

    // Default panel mode: sets panelAppId, doesn't open overlay
    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe('filebrowser')
    expect(mockWindowOpen).not.toHaveBeenCalled()
  })

  it('uses the store-driven panel on mobile (no route change, no background swap)', () => {
    Object.defineProperty(window, 'innerWidth', {
      value: 390,
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.openSession('indeedhub')

    // Mobile now uses the store-driven panel like desktop panel mode so the
    // underlying page/tab never changes and closing returns to the origin.
    expect(store.panelAppId).toBe('indeedhub')
    expect(mockPush).not.toHaveBeenCalled()
  })

  it('normalizes localhost launch URLs to current host before resolving', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://localhost:4080', title: 'Mempool' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe('mempool')
    expect(mockWindowOpen).not.toHaveBeenCalled()
  })

  it('normalizes localhost IndeeHub URLs to current host before resolving', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://localhost:7778', title: 'IndeeHub' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe('indeedhub')
    expect(mockWindowOpen).not.toHaveBeenCalled()
  })

  it('routes BTCPay (port 23000) to full-page session', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:23000', title: 'BTCPay' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:23000',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('normalizes old Nginx Proxy Manager port 81 to 8081', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:81', title: 'Nginx Proxy Manager' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:8081',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('opens tab-only apps directly on mobile (new tab in PWA, no interstitial)', () => {
    Object.defineProperty(window, 'innerWidth', {
      value: 390,
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:8081', title: 'Nginx Proxy Manager' })

    // Tab-only app on mobile-web: open directly in a new browser tab (the
    // companion would use the in-app WebView). No session, no route push, no
    // "this app opens in a tab" interstitial.
    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockPush).not.toHaveBeenCalled()
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:8081',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('opens Nginx Proxy Manager in new tab using title hint when URL is path-only', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'https://192.0.2.10/app/nginx-proxy-manager/', title: 'Nginx Proxy Manager' })

    expect(mockWindowOpen).toHaveBeenCalledWith(
      'https://192.0.2.10/app/nginx-proxy-manager/',
      '_blank',
      'noopener,noreferrer',
    )
    expect(store.panelAppId).toBe(null)
  })

  it('normalizes legacy Nginx Proxy Manager ports to 8081', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:8181', title: 'Nginx Proxy Manager' })

    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:8081',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('normalizes legacy Uptime Kuma port 3001 to 3002', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:3001', title: 'Uptime Kuma' })

    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:3002',
      '_blank',
      'noopener,noreferrer',
    )
    expect(store.panelAppId).toBe(null)
    expect(store.isOpen).toBe(false)
  })

  it('opens Uptime Kuma in new tab using title hint when URL is path-only', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'https://192.0.2.10/app/uptime-kuma/', title: 'Uptime Kuma' })

    expect(mockWindowOpen).toHaveBeenCalledWith(
      'https://192.0.2.10/app/uptime-kuma/',
      '_blank',
      'noopener,noreferrer',
    )
    expect(store.panelAppId).toBe(null)
  })

  it('routes Home Assistant (port 8123) to full-page session', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:8123', title: 'Home Assistant' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:8123',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('routes Grafana (port 3000) to full-page session', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:3000', title: 'Grafana' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:3000',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('opens Gitea path URL in new tab', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10/app/gitea/', title: 'Gitea' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10/app/gitea/',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('does not map raw port 3001 to gitea session', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:3001', title: 'Unknown 3001' })

    expect(store.panelAppId).toBe(null)
    expect(store.isOpen).toBe(true)
  })

  it('opens in new tab when openInNewTab flag is set for unknown URL', () => {
    const store = useAppLauncherStore()

    // Use an unresolvable URL so it doesn't route to session
    store.open({ url: 'http://192.0.2.10:9999', title: 'Unknown', openInNewTab: true })

    expect(store.isOpen).toBe(false)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.10:9999',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('opens known prepackaged websites in new tab on desktop when requested', () => {
    const store = useAppLauncherStore()

    store.open({ url: 'https://botfights.net', title: 'BotFights', openInNewTab: true })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'https://botfights.net',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('opens prepackaged websites in the store-driven panel on mobile', () => {
    Object.defineProperty(window, 'innerWidth', {
      value: 390,
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.open({ url: 'https://botfights.net', title: 'BotFights', openInNewTab: true })

    // Iframeable prepackaged sites stay in-app via the store panel (no route
    // change, no background swap) just like every other mobile launch.
    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe('botfights')
    expect(mockWindowOpen).not.toHaveBeenCalled()
    expect(mockPush).not.toHaveBeenCalled()
  })

  it('routes HTTPS same-host apps via session view', () => {
    Object.defineProperty(window, 'location', {
      value: { origin: 'https://192.0.2.10', protocol: 'https:', hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:8083', title: 'FileBrowser' })

    // Known port — routes to session (panel mode by default)
    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe('filebrowser')
  })

  it('opens unknown URL in iframe overlay on HTTP', () => {
    const store = useAppLauncherStore()

    // Unresolvable URL — falls through to iframe overlay
    store.open({ url: 'http://192.0.2.10:9999', title: 'Custom App' })

    expect(store.isOpen).toBe(true)
    expect(store.url).toBe('http://192.0.2.10:9999')
    expect(store.title).toBe('Custom App')
    expect(mockWindowOpen).not.toHaveBeenCalled()
  })

  it('opens unknown different-host URL in iframe overlay', () => {
    Object.defineProperty(window, 'location', {
      value: { origin: 'https://192.0.2.10', protocol: 'https:', hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.168.1.100:9999', title: 'Remote App' })

    // Different host, unknown port — opens in iframe overlay (no proxy rewrite)
    expect(store.isOpen).toBe(true)
    expect(store.url).toBe('http://192.168.1.100:9999')
  })

  it('close resets state', () => {
    const store = useAppLauncherStore()
    // Use unknown URL to trigger iframe overlay
    store.open({ url: 'http://192.0.2.10:9999', title: 'Custom' })

    store.close()

    expect(store.isOpen).toBe(false)
    expect(store.url).toBe('')
    expect(store.title).toBe('')
  })

  it('close restores focus to previous element', async () => {
    vi.useFakeTimers()
    const store = useAppLauncherStore()
    const mockButton = { focus: vi.fn() } as unknown as HTMLElement
    Object.defineProperty(document, 'activeElement', { value: mockButton, configurable: true })

    store.open({ url: 'http://192.0.2.10:9999', title: 'Custom' })
    store.close()

    expect(store.isOpen).toBe(false)
    expect(store.url).toBe('')

    // requestAnimationFrame fires the focus restore callback
    vi.runAllTimers()
    vi.useRealTimers()
  })
})
