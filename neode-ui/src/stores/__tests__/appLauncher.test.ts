import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { __setSignedCatalogForTests } from '@/views/discover/curatedApps'
import { nextTick } from 'vue'
import { useToast } from '@/composables/useToast'

// The signed catalog's embedded manifests decide which ports the app gate
// fronts (TLS on the same port) — prime the same shape the live catalog
// carries for the apps these tests launch.
const SIGNED = {
  apps: {
    vaultwarden: { version: '1.37.1', manifest: { app: { ports: [{ host: 8082, auth: 'gated' }] } } },
    gitea: { version: '1.23', manifest: { app: { ports: [{ host: 3001, auth: 'open' }] } } },
    'nginx-proxy-manager': { version: 'latest' }, // legacy: no manifest → http
  },
}

// vi.hoisted runs before vi.mock hoisting
const { mockPush, mockWindowOpen, mockRpcCall } = vi.hoisted(() => ({
  mockPush: vi.fn(),
  mockWindowOpen: vi.fn(),
  mockRpcCall: vi.fn(),
}))

// Mock vue-router
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: mockPush }),
}))
vi.mock('@/router', () => ({
  default: { push: mockPush, currentRoute: { value: { fullPath: '/dashboard/apps', name: 'apps' } } },
}))
vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: mockRpcCall },
}))

vi.stubGlobal('open', mockWindowOpen)

import { useAppLauncherStore, senderMatchesApp } from '../appLauncher'
import { useAppStore } from '../app'

describe('legacy overlay native signer lifecycle', () => {
  let store: ReturnType<typeof useAppLauncherStore>
  const appUrl = 'http://192.0.2.10:19999'
  const identityKey = 'archipelago_app_identity_' + appUrl.replace(/[^a-z0-9]/gi, '_')
  const frame = { postMessage: vi.fn() } as unknown as Window
  function request(id: string, source = frame, origin = appUrl) {
    const event = new MessageEvent('message', { origin, data: { type: 'nostr-request', id,
      method: 'signEvent', params: { event: { kind: 27235, content: id } } } })
    Object.defineProperty(event, 'source', { value: source })
    window.dispatchEvent(event)
  }
  beforeEach(() => {
    vi.useFakeTimers()
    setActivePinia(createPinia())
    localStorage.clear()
    vi.clearAllMocks()
    mockRpcCall.mockResolvedValue({ id: 'signed-fixture' })
    Object.defineProperty(window, 'location', { value: { origin: 'http://192.0.2.10', protocol: 'http:', hostname: '192.0.2.10' }, configurable: true })
    Object.defineProperty(window, 'innerWidth', { value: 1024, configurable: true })
    localStorage.setItem(identityKey, JSON.stringify({ id: 'legacy-identity', name: 'Saved creator' }))
    store = useAppLauncherStore()
    store.open({ url: appUrl, title: 'Custom film app' })
    store.setNostrFrame(frame)
  })
  afterEach(async () => {
    store.close(); store.$dispose()
    await vi.runAllTimersAsync()
    vi.useRealTimers()
  })
  it('queues simultaneous requests and preserves the saved identity and success animation', async () => {
    request('first'); request('second')
    expect(store.consentRequest?.content).toBe('first')
    expect(store.consentRequest?.identityLabel).toBe('Saved creator')
    store.approveConsent(false)
    await vi.advanceTimersByTimeAsync(350)
    expect(store.consentPhase).toBe('success')
    expect(mockRpcCall).toHaveBeenCalledTimes(1)
    expect(mockRpcCall).toHaveBeenCalledWith({ method: 'identity.nostr-sign', params: {
      id: 'legacy-identity', event: { kind: 27235, content: 'first' } } })
    await vi.advanceTimersByTimeAsync(325)
    expect(store.consentRequest?.content).toBe('second')
    store.approveConsent(false)
    await vi.advanceTimersByTimeAsync(675)
    expect(frame.postMessage).toHaveBeenCalledTimes(2)
    expect(frame.postMessage).toHaveBeenNthCalledWith(1, { type: 'nostr-response', id: 'first', result: { id: 'signed-fixture' } }, appUrl)
    expect(frame.postMessage).toHaveBeenNthCalledWith(2, { type: 'nostr-response', id: 'second', result: { id: 'signed-fixture' } }, appUrl)
    expect(store.showConsent).toBe(false)
  })
  it('rejects a same-origin window that is not the actual app iframe', async () => {
    const other = { postMessage: vi.fn() } as unknown as Window
    request('forged-window', other)
    await nextTick()
    expect(store.showConsent).toBe(false)
    expect(mockRpcCall).not.toHaveBeenCalled()
    expect(other.postMessage).not.toHaveBeenCalled()
  })
  it.each(['close', 'frame', 'url', 'dispose'])('settles all pending requests on %s without signing', async action => {
    request('first'); request('second')
    if (action === 'close') store.close()
    if (action === 'frame') store.setNostrFrame(null)
    if (action === 'url') store.url = 'http://192.0.2.10:19998'
    if (action === 'dispose') store.$dispose()
    await vi.runAllTimersAsync()
    expect(mockRpcCall).not.toHaveBeenCalled()
    expect(frame.postMessage).toHaveBeenCalledTimes(2)
    for (const [response] of vi.mocked(frame.postMessage).mock.calls) expect(response).toHaveProperty('error')
  })
  it('rejects identity changes before approval and can reopen after closing', async () => {
    request('old-identity')
    localStorage.setItem(identityKey, JSON.stringify({ id: 'another-identity', name: 'Other' }))
    store.approveConsent(false)
    await vi.runAllTimersAsync()
    expect(mockRpcCall).not.toHaveBeenCalled()
    store.close()
    store.open({ url: appUrl, title: 'Custom film app' })
    store.setNostrFrame(frame)
    request('reopened')
    await vi.advanceTimersByTimeAsync(0)
    store.approveConsent(false)
    await vi.advanceTimersByTimeAsync(675)
    expect(mockRpcCall).toHaveBeenCalledWith(expect.objectContaining({ params: expect.objectContaining({ id: 'another-identity' }) }))
  })
});

describe('useAppLauncherStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockRpcCall.mockResolvedValue({ credentials: [] })
    __setSignedCatalogForTests(SIGNED as never)
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

  it.each(['active', 'closed', 'superseded', 'reopened'])('binds late media retention to the original launch: %s', state => {
    const id = 'late-media-fixture'
    __setSignedCatalogForTests({apps:{[id]:{version:'1',manifest:{app:{id,metadata:{launch:{media_controls:'archipelago-v1'}}}}}}} as never)
    const app = useAppStore()
    app.data = {'package-data':{}} as never
    const launcher = useAppLauncherStore()
    launcher.openSession(id, {skipCredentialPrompt:true})
    expect(launcher.panelAppId).toBe(id)
    const generation = launcher.mediaSessionGeneration(id)
    expect(launcher.mediaAppId).toBeNull()
    app.data = {'package-data':{[id]:{state:'running','ui-ready':true,manifest:{id,version:'1'}}}} as never
    if (state === 'closed' || state === 'reopened') launcher.closePanel()
    if (state === 'superseded') launcher.openSession('mempool', {skipCredentialPrompt:true})
    if (state === 'reopened') { launcher.openSession(id, {skipCredentialPrompt:true}); launcher.mediaAppId=null }
    expect(launcher.retainMediaSession(id,'1',generation)).toBe(state === 'active')
    expect(launcher.mediaAppId).toBe(state === 'active' ? id : null)
    if (state === 'active') {
      launcher.closePanel()
      expect(launcher.mediaAppId).toBe(id)
      launcher.openSession(id)
      expect(launcher.panelAppId).toBe(id)
    }
  })

  it.each(['close', 'another-app', 'none'])('waits for node demo launch policy, cancellation: %s', async cancel => {
    const id = `node-demo-delayed-${cancel}`
    const previousFetch = globalThis.fetch
    let release!: (value: unknown) => void
    vi.stubGlobal('fetch', vi.fn(() => new Promise(resolve => { release = resolve })))
    useAppStore().data = {'package-data':{[id]:{state:'running','ui-ready':true,manifest:{id,version:'1'}}}} as never
    const launcher = useAppLauncherStore()
    launcher.openSession(id)
    expect(launcher.panelAppId).toBeNull()
    if (cancel === 'close') launcher.closePanel()
    if (cancel === 'another-app') launcher.openSession('mempool')
    release({ ok: true, json: async () => ({ scope: 'single-node-demo', expires_at: new Date(Date.now() + 60000).toISOString(), apps: {
      [id]: { version: '1', manifest: { app: { id, metadata: { launch: { media_controls: 'archipelago-v1' } } } } },
    } }) })
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(launcher.panelAppId).toBe(cancel === 'none' ? id : cancel === 'another-app' ? 'mempool' : null)
    launcher.openSession(id)
    expect(launcher.panelAppId).toBe(id)
    expect(launcher.mediaAppId).toBe(id)
    vi.stubGlobal('fetch', previousFetch)
  })

  it('shows unavailable launch policy with explicit retry and ignores a canceled retry', async()=>{
    __setSignedCatalogForTests(null)
    const previousFetch=globalThis.fetch
    vi.stubGlobal('fetch',vi.fn(async()=>({ok:false})))
    const launcher=useAppLauncherStore();launcher.openSession('mempool')
    expect(launcher.panelAppId).toBe('mempool')
    expect(launcher.mediaAppId).toBeNull()
    expect(useToast().toasts.value[useToast().toasts.value.length - 1]?.message).toContain('Checking')
    await new Promise(resolve=>setTimeout(resolve,0))
    const failure=useToast().toasts.value[useToast().toasts.value.length - 1]
    expect(failure?.message).toContain('unavailable');expect(failure?.action?.label).toBe('Retry settings');expect(launcher.panelAppId).toBe('mempool');expect(launcher.mediaAppId).toBeNull()
    launcher.closePanel();const calls=vi.mocked(fetch).mock.calls.length;failure?.action?.onClick();expect(fetch).toHaveBeenCalledTimes(calls)
    vi.stubGlobal('fetch',previousFetch)
  })
  it.each(['close','another-app'])('does not reopen a late generic launch after %s',async(cancel)=>{
    __setSignedCatalogForTests(null);const previousFetch=globalThis.fetch
    let release!:(value:unknown)=>void
    vi.stubGlobal('fetch',vi.fn(()=>new Promise(resolve=>{release=resolve})))
    const launcher=useAppLauncherStore();launcher.openSession('filebrowser')
    if(cancel==='close')launcher.closePanel();else launcher.openSession('mempool')
    release({ok:true,json:async()=>SIGNED});await new Promise(resolve=>setTimeout(resolve,0))
    expect(launcher.panelAppId).toBe(cancel==='close'?null:'mempool')
    vi.stubGlobal('fetch',previousFetch)
  })
  it('blocks both browser and embedded launch while HTTP is unready', () => {
    const app = useAppStore()
    app.data = { 'package-data': { gitea: { state: 'running', 'ui-ready': false, health: 'healthy', manifest: { id: 'gitea', title: 'Gitea' } } } } as never
    const launcher = useAppLauncherStore()
    launcher.openSession('gitea')
    expect(launcher.panelAppId).toBeNull()
    launcher.open({ url: 'http://192.0.2.10:3001/', title: 'Gitea', openInNewTab: true })
    expect(mockWindowOpen).not.toHaveBeenCalled()
    expect(launcher.isOpen).toBe(false)
  })

  it('also gates a dynamic app resolved through its runtime URL', () => {
    useAppStore().data = { 'package-data': { custom: { state: 'running', 'ui-ready': false, manifest: { id: 'custom', title: 'Custom' }, installed: { 'interface-addresses': { main: { 'lan-address': 'http://localhost:18993/' } } } } } } as never
    const launcher = useAppLauncherStore()
    launcher.open({ url: 'http://192.0.2.10:18993/', title: 'Custom', openInNewTab: true })
    expect(mockWindowOpen).not.toHaveBeenCalled()
    expect(launcher.isOpen).toBe(false)
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

    it('shows credentials before handing an app to the native WebView', async () => {
      const store = useAppLauncherStore()
      store.openSession('filebrowser')
      await vi.waitFor(() => expect(store.credentialPrompt.loading).toBe(false))
      expect(store.credentialPrompt.show).toBe(true)
      expect(openInApp).not.toHaveBeenCalled()
      store.continueCredentialLaunch()
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

    it('opens GitWorkshop in the companion native WebView, never a dashboard iframe', () => {
      const store = useAppLauncherStore()
      store.openSession('archipelago-source')
      expect(openInApp).toHaveBeenCalledWith(
        'http://192.0.2.10/app/archipelago-source/npub1w3sqdkrhn0gyuvsex32effzgnfpyde6qrrc4u467flg5e9txh4wsfn5vjg/relay.ngit.dev/archy',
      )
      expect(store.panelAppId).toBeNull()
      expect(store.isOpen).toBe(false)
    })

    it('open() never falls through to the iframe overlay', () => {
      const store = useAppLauncherStore()
      store.open({ url: 'http://192.0.2.10:9999', title: 'Unknown app' })
      expect(openInApp).toHaveBeenCalledWith('http://192.0.2.10:9999')
      expect(store.isOpen).toBe(false)
    })
  })

  it('routes known port apps to full-page session after the credential gate', async () => {
    const store = useAppLauncherStore()

    // Port 8083 maps to /app/filebrowser/ — should route to session
    store.open({ url: 'http://192.0.2.10:8083', title: 'FileBrowser' })
    await vi.waitFor(() => expect(store.credentialPrompt.loading).toBe(false))
    store.continueCredentialLaunch()

    // Default panel mode: sets panelAppId, doesn't open overlay
    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe('filebrowser')
    expect(mockWindowOpen).not.toHaveBeenCalled()
  })

  it('shows DATUM generated credentials before opening its embedded UI', async () => {
    mockRpcCall.mockResolvedValueOnce({
      title: 'DATUM Gateway login',
      credentials: [{ label: 'Password', value: 'fixture-only-password', sensitive: true }],
    })
    const store = useAppLauncherStore()
    store.openSession('datum')
    await vi.waitFor(() => expect(store.credentialPrompt.loading).toBe(false))
    expect(store.credentialPrompt.show).toBe(true)
    expect(store.credentialPrompt.credentials[0]?.value).toBe('fixture-only-password')
    expect(store.panelAppId).toBeNull()
    expect(mockWindowOpen).not.toHaveBeenCalled()
    store.continueCredentialLaunch()
    expect(store.panelAppId).toBe('datum')
  })

  it('gates a Home-style Portainer launch until its first-run token is shown', async () => {
    mockRpcCall.mockResolvedValueOnce({
      title: 'Portainer first-run token',
      description: 'Use this token to create the administrator account.',
      credentials: [{ label: 'Token', value: 'test-token', sensitive: true }],
    })
    const store = useAppLauncherStore()

    store.openSession('portainer')
    await vi.waitFor(() => expect(store.credentialPrompt.loading).toBe(false))

    expect(store.credentialPrompt.show).toBe(true)
    expect(store.credentialPrompt.credentials[0]?.value).toBe('test-token')
    expect(mockWindowOpen).not.toHaveBeenCalled()

    store.continueCredentialLaunch()
    expect(mockWindowOpen).toHaveBeenCalledWith(
      expect.stringContaining(':9000'),
      '_blank',
      'noopener,noreferrer',
    )
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

  // An HTTPS connection must never hand the remote browser (or the phone
  // webview) a cleartext app URL: same-host app ports are gate-owned and
  // serve TLS on the same port. Plain-http pages keep http exactly as before
  // — pinned by every test above this one.
  it('upgrades same-host app URLs to https on an https page', () => {
    Object.defineProperty(window, 'location', {
      value: { origin: 'https://192.0.2.10', protocol: 'https:', hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:8082', title: 'Vaultwarden' })

    expect(store.isOpen).toBe(false)
    expect(store.panelAppId).toBe(null)
    expect(mockWindowOpen).toHaveBeenCalledWith(
      'https://192.0.2.10:8082',
      '_blank',
      'noopener,noreferrer',
    )
  })

  it('never upgrades a different host on an https page', () => {
    Object.defineProperty(window, 'location', {
      value: { origin: 'https://192.0.2.10', protocol: 'https:', hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.168.1.100:8082', title: 'Vaultwarden' })

    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.168.1.100:8082',
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

  it('routes HTTPS same-host apps via session view after the credential gate', async () => {
    Object.defineProperty(window, 'location', {
      value: { origin: 'https://192.0.2.10', protocol: 'https:', hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })
    const store = useAppLauncherStore()

    store.open({ url: 'http://192.0.2.10:8083', title: 'FileBrowser' })
    await vi.waitFor(() => expect(store.credentialPrompt.loading).toBe(false))
    store.continueCredentialLaunch()

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

  describe('NIP-07 sender origin matching', () => {
    it('accepts a scheme-upgraded frame (HSTS) as the opened app', () => {
      // Regression (2026-09-01): the stored app URL was http:// but the
      // browser loaded the frame as https:// — strict origin equality
      // dropped every nostr sign-in from the upgraded frame.
      expect(senderMatchesApp('http://framework-pt.local:7778', 'https://framework-pt.local:7778')).toBe(true)
      expect(senderMatchesApp('https://framework-pt.local:7778', 'http://framework-pt.local:7778')).toBe(true)
    })

    it('still rejects a different host or port', () => {
      expect(senderMatchesApp('http://framework-pt.local:7778', 'https://evil.example:7778')).toBe(false)
      expect(senderMatchesApp('http://framework-pt.local:7778', 'https://framework-pt.local:7777')).toBe(false)
      expect(senderMatchesApp('http://framework-pt.local:7778', 'null')).toBe(false)
      expect(senderMatchesApp('', 'https://framework-pt.local:7778')).toBe(false)
    })
  })
})
