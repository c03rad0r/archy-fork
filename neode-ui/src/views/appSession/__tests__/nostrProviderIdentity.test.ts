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
    selectIdentity: () => Promise<unknown>
    getSelectedIdentity: () => { nostr_pubkey: string } | null
    onIdentitySelected: (
      callback: (identity: { nostr_pubkey: string }) => void,
    ) => () => void
  }
}

describe('nostr-provider identity selection', () => {
  let providerWindow: ProviderWindow

  beforeEach(() => {
    providerWindow = window as ProviderWindow
    delete providerWindow.__archipelagoNostr
    delete providerWindow.nostr
    delete providerWindow.archipelagoNostr
    delete providerWindow.ArchipelagoSurface
    document.documentElement.innerHTML = '<head><title>IndeedHub</title></head><body></body>'
    window.history.replaceState({}, '', '/app/indeedhub/')
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
    Reflect.deleteProperty(document, 'readyState')
    delete providerWindow.__archipelagoNostr
    delete providerWindow.nostr
    delete providerWindow.archipelagoNostr
    delete providerWindow.ArchipelagoSurface
  })

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
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: 'abc123' } },
      origin: signerOrigin,
    })
    Object.defineProperty(selected, 'source', { value: frame.contentWindow })
    window.dispatchEvent(selected)
    await Promise.resolve()

    await expect(publicKey).resolves.toBe('abc123')
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
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: 'fast-choice' } },
      origin: signerOrigin,
    })
    Object.defineProperty(selected, 'source', { value: frame.contentWindow })
    window.dispatchEvent(selected)
    postMessage.mockClear()

    await expect(providerWindow.nostr!.getPublicKey()).resolves.toBe('fast-choice')
    expect(postMessage).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'nostr-request', method: 'getPublicKey' }),
      signerOrigin,
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
      data: { type: 'nostr-response', id: request.id, result: 'recreated-key' },
      origin: signerOrigin,
    })
    Object.defineProperty(response, 'source', { value: replacement.contentWindow })
    window.dispatchEvent(response)
    await expect(publicKey).resolves.toBe('recreated-key')
  })

  it('delivers an eager identity to an app listener that mounts afterward', () => {
    const { frame, signerOrigin } = loadProvider(false)
    const selected = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: 'late-listener' } },
      origin: signerOrigin,
    })
    Object.defineProperty(selected, 'source', { value: frame.contentWindow })
    window.dispatchEvent(selected)

    const listener = vi.fn()
    const unsubscribe = providerWindow.archipelagoNostr!.onIdentitySelected(listener)

    expect(listener).toHaveBeenCalledOnce()
    expect(listener).toHaveBeenCalledWith({ nostr_pubkey: 'late-listener' })
    expect(providerWindow.archipelagoNostr!.getSelectedIdentity())
      .toEqual({ nostr_pubkey: 'late-listener' })

    unsubscribe()
    const changed = new MessageEvent('message', {
      data: { type: 'archipelago:signer-identity', identity: { nostr_pubkey: 'after-unsubscribe' } },
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
    const { postMessage, signerOrigin } = loadProvider(false)

    const identity = new MessageEvent('message', {
      data: { type: 'archipelago:identity', nostr_pubkey: 'indeedhub-key' },
      origin: window.location.origin,
    })
    Object.defineProperty(identity, 'source', { value: window })
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
        params: { event: expect.objectContaining({ kind: 27235, pubkey: 'indeedhub-key' }) },
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
        json: () => Promise.resolve({ accessToken: 'real-token', refreshToken: 'refresh' }),
      })
    vi.stubGlobal('fetch', fetchMock)
    const raf = vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 1)
    const { frame, postMessage, signerOrigin } = loadProvider(false)

    const identity = new MessageEvent('message', {
      data: { type: 'archipelago:identity', nostr_pubkey: 'indeedhub-key' },
      origin: window.location.origin,
    })
    Object.defineProperty(identity, 'source', { value: window })
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
      data: { type: 'nostr-response', id: signRequest.id, result: { id: 'signed-event' } },
      origin: signerOrigin,
    })
    Object.defineProperty(signed, 'source', { value: frame.contentWindow })
    window.dispatchEvent(signed)
    await vi.advanceTimersByTimeAsync(0)

    expect(sessionStorage.getItem('nostr_token')).toBe('real-token')
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
})
