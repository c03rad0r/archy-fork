import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const providerSource = readFileSync(
  resolve(process.cwd(), 'public/nostr-provider.js'),
  'utf8',
)

type ProviderWindow = Window & {
  __archipelagoNostr?: boolean
  ArchipelagoSurface?: {
    expectPageTransition: () => void
  }
  nostr?: { getPublicKey: () => Promise<string> }
  archipelagoNostr?: {
    clearSession: () => void
    selectIdentity: () => Promise<unknown>
    getSelectedIdentity: () => { nostr_pubkey: string } | null
    onIdentitySelected: (
      callback: (identity: { nostr_pubkey: string }) => void,
    ) => () => void
  }
}

describe('nostr-provider identity selection', () => {
  let providerWindow: ProviderWindow
  let listeners: Array<[string, EventListenerOrEventListenerObject, boolean | AddEventListenerOptions | undefined]>

  beforeEach(() => {
    vi.useFakeTimers()
    listeners = []
    const add = window.addEventListener.bind(window)
    vi.spyOn(window, 'addEventListener').mockImplementation((type, listener, options) => {
      if (listener) listeners.push([type, listener as EventListenerOrEventListenerObject, options])
      add(type, listener, options)
    })
    sessionStorage.clear()
    providerWindow = window as ProviderWindow
    delete providerWindow.__archipelagoNostr
    delete providerWindow.nostr
    delete providerWindow.archipelagoNostr
    delete providerWindow.ArchipelagoSurface
    document.documentElement.innerHTML = '<head><title>IndeedHub</title></head><body></body>'
    window.history.replaceState({}, '', '/app/indeedhub/')
  })

  afterEach(() => {
    for (const [type, listener, options] of listeners) window.removeEventListener(type, listener, options)
    vi.clearAllTimers()
    vi.unstubAllGlobals()
    vi.useRealTimers()
    vi.restoreAllMocks()
    Reflect.deleteProperty(document, 'readyState')
    delete providerWindow.__archipelagoNostr
    delete providerWindow.nostr
    delete providerWindow.archipelagoNostr
    delete providerWindow.ArchipelagoSurface
  })

  function token(pubkey: string, exp = Math.floor(Date.now() / 1000) + 3600) {
    return `e30.${btoa(JSON.stringify({ sub: pubkey, exp })).replace(/=+$/, '')}.signature`
  }

  function choose(frame: HTMLIFrameElement, origin: string, pubkey: string, display_name?: string) {
    const event = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: pubkey, display_name } }, origin,
    })
    Object.defineProperty(event, 'source', { value: frame.contentWindow })
    window.dispatchEvent(event)
  }

  function loadProvider(userActivated: boolean) {
    Object.defineProperty(navigator, 'userActivation', {
      configurable: true,
      value: { isActive: userActivated },
    })
    window.eval(providerSource)
    window.dispatchEvent(new Event('load'))
    const frame = document.querySelector<HTMLIFrameElement>('#archipelago-nostr-signer')!
    const postMessage = vi.spyOn(frame.contentWindow!, 'postMessage')
    const signerOriginUrl = new URL(window.location.href)
    signerOriginUrl.port = ''
    const signerOrigin = signerOriginUrl.origin
    const ready = new MessageEvent('message', {
      data: { type: 'archipelago:signer-ready' },
      origin: signerOrigin,
    })
    Object.defineProperty(ready, 'source', { value: frame.contentWindow })
    window.dispatchEvent(ready)
    postMessage.mockClear()
    return { frame, postMessage, signerOrigin }
  }

  it('reopens the chooser for a user-triggered NIP-07 login', async () => {
    const { frame, postMessage, signerOrigin } = loadProvider(true)
    const publicKey = providerWindow.nostr!.getPublicKey()

    expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'archipelago:signer-select-identity', force: true }),
      signerOrigin,
    )

    const selected = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: '6ca13d52ca70c883e0f0bb101e425a89e8624de51db2d2392593af6a84118090' } },
      origin: signerOrigin,
    })
    Object.defineProperty(selected, 'source', { value: frame.contentWindow })
    window.dispatchEvent(selected)
    await Promise.resolve()

    await expect(publicKey).resolves.toBe('6ca13d52ca70c883e0f0bb101e425a89e8624de51db2d2392593af6a84118090')
    expect(postMessage).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'nostr-request', method: 'getPublicKey' }),
      signerOrigin,
    )
  })

  it('uses the remembered identity for background account restoration', () => {
    const { postMessage, signerOrigin } = loadProvider(false)
    void providerWindow.nostr!.getPublicKey()

    expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'nostr-request', method: 'getPublicKey' }),
      signerOrigin,
    )
    expect(postMessage).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'archipelago:signer-select-identity' }),
      expect.anything(),
    )
  })

  it('keeps an eager picker choice for the app login that follows', async () => {
    const { frame, postMessage, signerOrigin } = loadProvider(false)
    const selected = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: 'd930382168af7367013ad9723e7ebc093918f788abcb32ce93db245031edff0c' } },
      origin: signerOrigin,
    })
    Object.defineProperty(selected, 'source', { value: frame.contentWindow })
    window.dispatchEvent(selected)
    postMessage.mockClear()

    await expect(providerWindow.nostr!.getPublicKey()).resolves.toBe('d930382168af7367013ad9723e7ebc093918f788abcb32ce93db245031edff0c')
    expect(postMessage).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'nostr-request', method: 'getPublicKey' }),
      signerOrigin,
    )
  })

  it('does not reopen after a selected identity when activation survives account restoration', async () => {
    const { frame, postMessage, signerOrigin } = loadProvider(true)
    const selected = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: '4af900489429183b065f3cd1e46c59666642b54bed7c2a5a0c0566970992536e' } },
      origin: signerOrigin,
    })
    Object.defineProperty(selected, 'source', { value: frame.contentWindow })
    window.dispatchEvent(selected)
    await expect(providerWindow.nostr!.getPublicKey()).resolves.toBe('4af900489429183b065f3cd1e46c59666642b54bed7c2a5a0c0566970992536e')
    postMessage.mockClear()
    const next = providerWindow.nostr!.getPublicKey()
    expect(postMessage).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'archipelago:signer-select-identity' }), expect.anything(),
    )
    const request = postMessage.mock.calls[0]![0] as { id: number }
    const response = new MessageEvent('message', {
      data: { type: 'nostr-response', id: request.id, result: '4af900489429183b065f3cd1e46c59666642b54bed7c2a5a0c0566970992536e' }, origin: signerOrigin,
    })
    Object.defineProperty(response, 'source', { value: frame.contentWindow })
    window.dispatchEvent(response)
    await expect(next).resolves.toBe('4af900489429183b065f3cd1e46c59666642b54bed7c2a5a0c0566970992536e')
    // Explicit account switching remains available after a successful login.
    void providerWindow.archipelagoNostr!.selectIdentity()
    expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'archipelago:signer-select-identity', force: true }), signerOrigin,
    )
  })

  it('does not let an unbound token suppress an explicit login choice', () => {
    sessionStorage.setItem('nostr_token', 'test-session-hint')
    const { postMessage, signerOrigin } = loadProvider(true)
    void providerWindow.nostr!.getPublicKey()
    expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'archipelago:signer-select-identity' }), signerOrigin,
    )
  })

  it('parks the hidden broker off-screen and reuses it for the next request', async () => {
    const surface = {
      expectPageTransition: vi.fn(),
    }
    providerWindow.ArchipelagoSurface = surface
    const { frame, postMessage, signerOrigin } = loadProvider(false)

    const show = new MessageEvent('message', {
      data: { type: 'archipelago:signer-show' },
      origin: signerOrigin,
    })
    Object.defineProperty(show, 'source', { value: frame.contentWindow })
    window.dispatchEvent(show)
    expect(frame.style.display).toBe('block')

    const hide = new MessageEvent('message', {
      data: { type: 'archipelago:signer-hide' },
      origin: signerOrigin,
    })
    Object.defineProperty(hide, 'source', { value: frame.contentWindow })
    window.dispatchEvent(hide)

    expect(frame.style.display).toBe('block')
    expect(frame.style.width).toBe('1px')
    expect(frame.style.height).toBe('1px')
    expect(frame.style.opacity).toBe('0')
    expect(frame.style.pointerEvents).toBe('none')
    expect(frame.style.transform).toContain('-10000px')
    expect(document.querySelector('#archipelago-nostr-signer')).toBe(frame)

    const publicKey = providerWindow.nostr!.getPublicKey()
    const replacement = document.querySelector<HTMLIFrameElement>('#archipelago-nostr-signer')!
    expect(replacement).toBe(frame)
    const ready = new MessageEvent('message', {
      data: { type: 'archipelago:signer-ready' },
      origin: signerOrigin,
    })
    Object.defineProperty(ready, 'source', { value: replacement.contentWindow })
    window.dispatchEvent(ready)

    const request = postMessage.mock.calls
      .map(call => call[0] as { type: string; id?: number })
      .find(message => message.type === 'nostr-request')!
    expect(request).toBeDefined()
    const response = new MessageEvent('message', {
      data: { type: 'nostr-response', id: request.id, result: 'f25efa08a1e6551d15d41622efedc40faceacc4f87ec766221613c14aa07cffb' },
      origin: signerOrigin,
    })
    Object.defineProperty(response, 'source', { value: replacement.contentWindow })
    window.dispatchEvent(response)
    await expect(publicKey).resolves.toBe('f25efa08a1e6551d15d41622efedc40faceacc4f87ec766221613c14aa07cffb')
  })

  it('delivers an eager identity to an app listener that mounts afterward', () => {
    const { frame, signerOrigin } = loadProvider(false)
    const selected = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: 'c23f5e74ca130353248c66f8fdeff29bcd77515f19536c5f82c51da223f9b410' } },
      origin: signerOrigin,
    })
    Object.defineProperty(selected, 'source', { value: frame.contentWindow })
    window.dispatchEvent(selected)

    const listener = vi.fn()
    const unsubscribe = providerWindow.archipelagoNostr!.onIdentitySelected(listener)

    expect(listener).toHaveBeenCalledOnce()
    expect(listener).toHaveBeenCalledWith({ nostr_pubkey: 'c23f5e74ca130353248c66f8fdeff29bcd77515f19536c5f82c51da223f9b410' })
    expect(providerWindow.archipelagoNostr!.getSelectedIdentity())
      .toEqual({ nostr_pubkey: 'c23f5e74ca130353248c66f8fdeff29bcd77515f19536c5f82c51da223f9b410' })

    unsubscribe()
    const changed = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: '7dbe01f62123a375cdfd5c92c78408db35005175e99fed1aa05d74b91df13f36' } },
      origin: signerOrigin,
    })
    Object.defineProperty(changed, 'source', { value: frame.contentWindow })
    window.dispatchEvent(changed)
    expect(listener).toHaveBeenCalledOnce()
  })

  it('turns an automatic IndeeHub identity into a NIP-98 signing request', async () => {
    vi.useFakeTimers()
    const fetchMock = vi.fn().mockResolvedValue({ ok: true })
    vi.stubGlobal('fetch', fetchMock)
    const { frame, postMessage, signerOrigin } = loadProvider(false)

    const identity = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: '0f431dd8b163f611777915a6479a81f48f1d3f2e2133e6916c91290c9b352047' } },
      origin: signerOrigin,
    })
    Object.defineProperty(identity, 'source', { value: frame.contentWindow })
    window.dispatchEvent(identity)
    await vi.advanceTimersByTimeAsync(1500)

    expect(fetchMock).toHaveBeenCalledWith(
      `${window.location.origin}/api/nostr-auth/health`,
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    )
    expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'nostr-request',
        method: 'signEvent',
        params: { event: expect.objectContaining({ kind: 27235, pubkey: '0f431dd8b163f611777915a6479a81f48f1d3f2e2133e6916c91290c9b352047' }) },
      }),
      signerOrigin,
    )
  })

  it('waits for the signer success surface to hide before reloading after NIP-98', async () => {
    vi.useFakeTimers()
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({ ok: true })
      .mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ accessToken: token('0f431dd8b163f611777915a6479a81f48f1d3f2e2133e6916c91290c9b352047'), refreshToken: 'refresh' }),
      })
    vi.stubGlobal('fetch', fetchMock)
    const raf = vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 1)
    const { frame, postMessage, signerOrigin } = loadProvider(false)

    const identity = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: '0f431dd8b163f611777915a6479a81f48f1d3f2e2133e6916c91290c9b352047' } },
      origin: signerOrigin,
    })
    Object.defineProperty(identity, 'source', { value: frame.contentWindow })
    window.dispatchEvent(identity)
    await vi.advanceTimersByTimeAsync(1500)

    const signRequest = postMessage.mock.calls
      .map(call => call[0] as { type: string; id?: number })
      .find(message => message.type === 'nostr-request' && message.id != null)!
    const show = new MessageEvent('message', {
      data: { type: 'archipelago:signer-show' },
      origin: signerOrigin,
    })
    Object.defineProperty(show, 'source', { value: frame.contentWindow })
    window.dispatchEvent(show)

    const signed = new MessageEvent('message', {
      data: { type: 'nostr-response', id: signRequest.id, result: { id: 'signed-event', pubkey: '0f431dd8b163f611777915a6479a81f48f1d3f2e2133e6916c91290c9b352047' } },
      origin: signerOrigin,
    })
    Object.defineProperty(signed, 'source', { value: frame.contentWindow })
    window.dispatchEvent(signed)
    await vi.advanceTimersByTimeAsync(0)

    expect(sessionStorage.getItem('nostr_token')).toBe(token('0f431dd8b163f611777915a6479a81f48f1d3f2e2133e6916c91290c9b352047'))
    expect(raf).not.toHaveBeenCalled()

    const hide = new MessageEvent('message', {
      data: { type: 'archipelago:signer-hide' },
      origin: signerOrigin,
    })
    Object.defineProperty(hide, 'source', { value: frame.contentWindow })
    window.dispatchEvent(hide)
    await Promise.resolve()

    expect(raf).toHaveBeenCalledOnce()
  })

  it('queues a request until signer-init when the signer iframe wins the load race', () => {
    Object.defineProperty(navigator, 'userActivation', {
      configurable: true,
      value: { isActive: false },
    })
    Object.defineProperty(document, 'readyState', {
      configurable: true,
      value: 'loading',
    })
    window.eval(providerSource)
    const frame = document.querySelector<HTMLIFrameElement>('#archipelago-nostr-signer')!
    const postMessage = vi.spyOn(frame.contentWindow!, 'postMessage')
    const signerOriginUrl = new URL(window.location.href)
    signerOriginUrl.port = ''
    const signerOrigin = signerOriginUrl.origin
    const ready = new MessageEvent('message', {
      data: { type: 'archipelago:signer-ready' },
      origin: signerOrigin,
    })
    Object.defineProperty(ready, 'source', { value: frame.contentWindow })
    window.dispatchEvent(ready)

    void providerWindow.nostr!.getPublicKey()
    expect(postMessage).not.toHaveBeenCalled()

    window.dispatchEvent(new Event('load'))
    expect(postMessage.mock.calls.map(call => (call[0] as { type: string }).type))
      .toEqual(['archipelago:signer-init', 'nostr-request'])
  })
  it.each(['expired', 'wrong-key', 'malformed'])('replaces a %s token without deleting stored accounts', async kind => {
    const original = kind === 'expired' ? token('7be403e3cc82262248913b70f596a31b93f656e0da8ce5d1bf757c63ef7a8db4', 1) : kind === 'wrong-key' ? token('d9298a10d1b0735837dc4bd85dac641b0f3cef27a47e5d53a54f2f3f5b2fcffa') : 'invalid'
    sessionStorage.setItem('nostr_token', original)
    sessionStorage.setItem('refresh_token', 'old-refresh')
    localStorage.setItem('saved-account-fixture', 'private-key-and-profile-fixture')
    const fetchMock = vi.fn().mockResolvedValue({ ok: true })
    vi.stubGlobal('fetch', fetchMock)
    const { frame, signerOrigin, postMessage } = loadProvider(false)
    choose(frame, signerOrigin, '7be403e3cc82262248913b70f596a31b93f656e0da8ce5d1bf757c63ef7a8db4')
    expect(sessionStorage.getItem('nostr_token')).toBeNull()
    expect(sessionStorage.getItem('refresh_token')).toBeNull()
    expect(localStorage.getItem('saved-account-fixture')).toBe('private-key-and-profile-fixture')
    await vi.advanceTimersByTimeAsync(1500)
    expect(fetchMock).toHaveBeenCalledOnce()
    expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({ method: 'signEvent' }), signerOrigin)
    localStorage.removeItem('saved-account-fixture')
  })

  it('reuses only an unexpired token matching the verified selected identity', async () => {
    const saved = token('7be403e3cc82262248913b70f596a31b93f656e0da8ce5d1bf757c63ef7a8db4')
    sessionStorage.setItem('nostr_token', saved)
    sessionStorage.setItem('nostr_pubkey', 'stale-key')
    sessionStorage.setItem('refresh_token', 'stale-refresh')
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    const { frame, signerOrigin } = loadProvider(false)
    choose(frame, signerOrigin, '7be403e3cc82262248913b70f596a31b93f656e0da8ce5d1bf757c63ef7a8db4', 'My native profile')
    await vi.advanceTimersByTimeAsync(1600)
    expect(fetchMock).not.toHaveBeenCalled()
    expect(sessionStorage.getItem('nostr_token')).toBe(saved)
    expect(sessionStorage.getItem('nostr_pubkey')).toBe('7be403e3cc82262248913b70f596a31b93f656e0da8ce5d1bf757c63ef7a8db4')
    expect(sessionStorage.getItem('refresh_token')).toBeNull()
    expect(providerWindow.archipelagoNostr!.getSelectedIdentity()).toEqual({ nostr_pubkey: '7be403e3cc82262248913b70f596a31b93f656e0da8ce5d1bf757c63ef7a8db4', display_name: 'My native profile' })
  })

  it('ignores self-forged identity messages and signer messages from another origin', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    const { frame } = loadProvider(false)
    choose(frame, 'https://untrusted.example', 'ccdd35168ab474fa5764a526cfb83621351e23682c5075b2e18d56bddf96aa30')
    const event = new MessageEvent('message', { data: { type: 'archipelago:identity', nostr_pubkey: 'ccdd35168ab474fa5764a526cfb83621351e23682c5075b2e18d56bddf96aa30' }, origin: location.origin })
    Object.defineProperty(event, 'source', { value: window })
    window.dispatchEvent(event)
    await vi.advanceTimersByTimeAsync(1600)
    expect(providerWindow.archipelagoNostr!.getSelectedIdentity()).toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it.each(['switch', 'logout'])('does not store an in-flight old session after %s', async action => {
    let complete!: (response: unknown) => void
    const fetchMock = vi.fn().mockResolvedValueOnce({ ok: true }).mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
    vi.stubGlobal('fetch', fetchMock)
    const { frame, signerOrigin, postMessage } = loadProvider(false)
    choose(frame, signerOrigin, 'cba06b5736faf67e54b07b561eae94395e774c517a7d910a54369e1263ccfbd4')
    await vi.advanceTimersByTimeAsync(1500)
    const request = postMessage.mock.calls.map(c => c[0]).find(v => v.method === 'signEvent')
    const signed = new MessageEvent('message', { data: { type: 'nostr-response', id: request.id, result: { pubkey: 'cba06b5736faf67e54b07b561eae94395e774c517a7d910a54369e1263ccfbd4' } }, origin: signerOrigin })
    Object.defineProperty(signed, 'source', { value: frame.contentWindow })
    window.dispatchEvent(signed)
    await vi.advanceTimersByTimeAsync(0)
    if (action === 'switch') choose(frame, signerOrigin, '11507a0e2f5e69d5dfa40a62a1bd7b6ee57e6bcd85c67c9b8431b36fff21c437')
    else providerWindow.archipelagoNostr!.clearSession()
    complete({ ok: true, json: async () => ({ accessToken: token('cba06b5736faf67e54b07b561eae94395e774c517a7d910a54369e1263ccfbd4'), refreshToken: 'old-refresh' }) })
    await vi.advanceTimersByTimeAsync(0)
    expect(sessionStorage.getItem('nostr_token')).toBeNull()
    expect(sessionStorage.getItem('refresh_token')).toBeNull()
  })

  it('announces a verified switch before clearing the old session and exposes only public metadata', () => {
    const oldKey = 'a'.repeat(64), nextKey = 'b'.repeat(64)
    sessionStorage.setItem('nostr_token', token(oldKey))
    const { frame, signerOrigin } = loadProvider(false)
    const changes: unknown[] = []
    window.addEventListener('archipelago:identity-changing', event => {
      changes.push({ detail: (event as CustomEvent).detail, token: sessionStorage.getItem('nostr_token'), selected: providerWindow.archipelagoNostr!.getSelectedIdentity() })
    })
    choose(frame, signerOrigin, nextKey, 'Chosen profile')
    expect(changes).toEqual([{ detail: { nostr_pubkey: nextKey, previous_nostr_pubkey: null }, token: token(oldKey), selected: { nostr_pubkey: nextKey, display_name: 'Chosen profile' } }])
    expect(sessionStorage.getItem('nostr_token')).toBeNull()
  })

  it('rejects a malformed native identity without exposing it or starting authentication', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    const { frame, signerOrigin } = loadProvider(false)
    choose(frame, signerOrigin, 'not-a-native-public-key')
    await vi.advanceTimersByTimeAsync(1600)
    expect(providerWindow.archipelagoNostr!.getSelectedIdentity()).toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('clears a now-expired token on repeated selection of the same identity', () => {
    const key = 'a'.repeat(64)
    const { frame, signerOrigin } = loadProvider(false)
    choose(frame, signerOrigin, key)
    sessionStorage.setItem('nostr_token', token(key, 1))
    const notify = vi.fn()
    window.addEventListener('archipelago:identity-changing', notify)
    choose(frame, signerOrigin, key)
    expect(notify).toHaveBeenCalledOnce()
    expect(sessionStorage.getItem('nostr_token')).toBeNull()
  })

  it('keeps logout quiet and permits a subsequent explicit identity choice', async () => {
    const key = 'a'.repeat(64)
    const fetchMock = vi.fn().mockResolvedValue({ ok: true })
    vi.stubGlobal('fetch', fetchMock)
    const { frame, signerOrigin, postMessage } = loadProvider(false)
    choose(frame, signerOrigin, key)
    postMessage.mockClear()
    providerWindow.archipelagoNostr!.clearSession()
    await vi.advanceTimersByTimeAsync(1600)
    expect(fetchMock).not.toHaveBeenCalled()
    expect(providerWindow.archipelagoNostr!.getSelectedIdentity()).toBeNull()
    expect(postMessage).toHaveBeenCalledWith(
      { type: 'archipelago:signer-clear-session' },
      signerOrigin,
    )
    const selection = providerWindow.archipelagoNostr!.selectIdentity()
    expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({ type: 'archipelago:signer-select-identity', force: true }), signerOrigin)
    choose(frame, signerOrigin, key)
    await expect(selection).resolves.toEqual({ nostr_pubkey: key })
    await vi.advanceTimersByTimeAsync(1500)
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  it('rejects a session response bound to another key after valid signing', async () => {
    const key = 'a'.repeat(64)
    const fetchMock = vi.fn().mockResolvedValueOnce({ ok: true }).mockResolvedValueOnce({ ok: true, json: async () => ({ accessToken: token('b'.repeat(64)) }) })
    vi.stubGlobal('fetch', fetchMock)
    const { frame, signerOrigin, postMessage } = loadProvider(false)
    choose(frame, signerOrigin, key)
    await vi.advanceTimersByTimeAsync(1500)
    const request = postMessage.mock.calls.map(c => c[0]).find(v => v.method === 'signEvent')
    const signed = new MessageEvent('message', { data: { type: 'nostr-response', id: request.id, result: { pubkey: key } }, origin: signerOrigin })
    Object.defineProperty(signed, 'source', { value: frame.contentWindow })
    window.dispatchEvent(signed)
    await vi.advanceTimersByTimeAsync(0)
    expect(sessionStorage.getItem('nostr_token')).toBeNull()
  })

  it('does not reset an in-flight auth when the same identity is announced again', async () => {
    const key = 'a'.repeat(64)
    let complete!: (response: unknown) => void
    const fetchMock = vi.fn().mockResolvedValueOnce({ ok: true }).mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
    vi.stubGlobal('fetch', fetchMock)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 1)
    const { frame, signerOrigin, postMessage } = loadProvider(false)
    choose(frame, signerOrigin, key)
    await vi.advanceTimersByTimeAsync(1500)
    const request = postMessage.mock.calls.map(c => c[0]).find(v => v.method === 'signEvent')
    const signed = new MessageEvent('message', { data: { type: 'nostr-response', id: request.id, result: { pubkey: key } }, origin: signerOrigin })
    Object.defineProperty(signed, 'source', { value: frame.contentWindow })
    window.dispatchEvent(signed)
    await vi.advanceTimersByTimeAsync(0)
    const notify = vi.fn()
    window.addEventListener('archipelago:identity-changing', notify)
    choose(frame, signerOrigin, key)
    expect(notify).not.toHaveBeenCalled()
    complete({ ok: true, json: async () => ({ accessToken: token(key) }) })
    await vi.advanceTimersByTimeAsync(0)
    expect(sessionStorage.getItem('nostr_token')).toBe(token(key))
  })

})
