import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { rpcClient } from '@/api/rpc-client'
import { useNostrBridge } from '../useNostrBridge'

vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))

describe('useNostrBridge consent presentation', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.useFakeTimers()
    vi.mocked(rpcClient.call).mockResolvedValue({ id: 'signed-event' })
  })
  afterEach(() => vi.useRealTimers())

  it('keeps the contained identity loader visible through signing and completion', async () => {
    const source = { postMessage: vi.fn() } as unknown as Window
    const bridge = useNostrBridge(
      () => ({ id: 'identity-a', name: 'Alice', nostr_pubkey: 'pubkey-a' } as never),
      {
        appId: () => 'archipelago-source', appName: () => 'GitWorkshop',
        appUrl: () => 'https://node.test/app/archipelago-source/', frameWindow: () => source,
      },
    )
    const event = {
      data: { type: 'nostr-request', id: 'request-1', method: 'signEvent', params: { event: { kind: 1621, content: 'Fix it' } } },
      source, origin: 'https://node.test',
    } as MessageEvent

    const handling = bridge.handleNostrRequest(event)
    await Promise.resolve()
    expect(bridge.showConsent.value).toBe(true)
    expect(bridge.consentPhase.value).toBe('review')

    bridge.approveConsent(false)
    expect(bridge.consentPhase.value).toBe('signing')
    expect(bridge.showConsent.value).toBe(true)
    await handling
    expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ type: 'nostr-response', id: 'request-1' }), 'https://node.test')

    await vi.advanceTimersByTimeAsync(350)
    expect(bridge.consentPhase.value).toBe('success')
    await vi.advanceTimersByTimeAsync(325)
    expect(bridge.showConsent.value).toBe(false)
  })
})

describe('useNostrBridge concurrent requests', () => {
  beforeEach(() => {
    localStorage.clear(); vi.useFakeTimers(); vi.mocked(rpcClient.call).mockReset()
    vi.mocked(rpcClient.call).mockResolvedValue({ id: 'signed-event' })
  })
  afterEach(() => vi.useRealTimers())
  function setup() {
    const source = { postMessage: vi.fn() } as unknown as Window
    let identity = 'identity-a'
    const bridge = useNostrBridge(() => ({ id: identity, name: identity } as never), {
      appId: () => 'indeehub', appName: () => 'IndeeHub',
      appUrl: () => 'https://node.test/app/indeehub/', frameWindow: () => source,
    })
    const request = (id: string) => bridge.handleNostrRequest({ source, origin: 'https://node.test', data: {
      type: 'nostr-request', id, method: 'signEvent', params: { event: { kind: 27235, content: id } },
    } } as MessageEvent)
    return { source, bridge, request, setIdentity: (value: string) => { identity = value } }
  }
  it('queues a second request without replacing the first prompt or dropping either response', async () => {
    const { source, bridge, request } = setup()
    const first = request('first'), second = request('second')
    expect(bridge.consentRequest.value?.content).toBe('first')
    bridge.approveConsent(false)
    await first
    expect(bridge.consentRequest.value?.content).toBe('first')
    await vi.advanceTimersByTimeAsync(675)
    expect(bridge.consentRequest.value?.content).toBe('second')
    bridge.approveConsent(false)
    await second
    expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ id: 'first', result: { id: 'signed-event' } }), 'https://node.test')
    expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ id: 'second', result: { id: 'signed-event' } }), 'https://node.test')
    await vi.runAllTimersAsync()
    expect(bridge.showConsent.value).toBe(false)
  })
  it('continues after a denied request without leaving the first promise pending', async () => {
    const { source, bridge, request } = setup()
    const first = request('denied'), second = request('allowed')
    bridge.denyConsent()
    await first
    await Promise.resolve(); await Promise.resolve()
    expect(bridge.consentRequest.value?.content).toBe('allowed')
    bridge.approveConsent(false); await second
    expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ id: 'denied', error: expect.any(String) }), 'https://node.test')
    expect(rpcClient.call).toHaveBeenCalledTimes(1)
    await vi.runAllTimersAsync()
  })
  it('waits for error dismissal before presenting the next request', async () => {
    const { bridge, request } = setup()
    vi.mocked(rpcClient.call).mockRejectedValueOnce(new Error('Temporary signer failure'))
    const first = request('failed'), second = request('next')
    bridge.approveConsent(false); await first
    expect(bridge.consentPhase.value).toBe('error')
    expect(bridge.consentRequest.value?.content).toBe('failed')
    bridge.denyConsent()
    await Promise.resolve(); await Promise.resolve()
    expect(bridge.consentRequest.value?.content).toBe('next')
    bridge.approveConsent(false); await second
    await vi.runAllTimersAsync()
  })
  it('cancels active and queued prompts on disposal without signing', async () => {
    const { source, bridge, request } = setup()
    const first = request('first'), second = request('second')
    bridge.dispose()
    await Promise.all([first, second, request('after-close')])
    expect(rpcClient.call).not.toHaveBeenCalled()
    for (const id of ['first', 'second', 'after-close']) {
      expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ id, error: expect.any(String) }), 'https://node.test')
    }
    expect(bridge.showConsent.value).toBe(false)
  })
  it('rejects an identity change while consent is pending', async () => {
    const { source, bridge, request, setIdentity } = setup()
    const pending = request('identity-change')
    setIdentity('identity-b'); bridge.approveConsent(false); await pending
    expect(rpcClient.call).not.toHaveBeenCalled()
    expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ error: expect.stringContaining('identity changed') }), 'https://node.test')
    bridge.dispose()
  })
  it('bounds the queue and responds to excess requests instead of silently hanging', async () => {
    const { source, bridge, request } = setup()
    const pending = Array.from({ length: 20 }, (_, i) => request(`request-${i}`))
    expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ error: expect.stringContaining('Too many') }), 'https://node.test')
    bridge.dispose(); await Promise.all(pending)
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('rejects a queued request if the selected identity changed before its turn', async () => {
    const { source, bridge, request, setIdentity } = setup()
    const first = request('first'), second = request('stale-queued')
    bridge.approveConsent(false); await first
    setIdentity('identity-b')
    await vi.runAllTimersAsync(); await second
    expect(rpcClient.call).toHaveBeenCalledTimes(1)
    expect(source.postMessage).toHaveBeenCalledWith(expect.objectContaining({ id: 'stale-queued', error: expect.stringContaining('identity changed') }), 'https://node.test')
  })
  it('allows a retained app session to sign again after its previous prompts were cancelled', async () => {
    const { bridge, request } = setup()
    const cancelled = request('cancelled')
    bridge.cancelPending(); await cancelled
    await Promise.resolve(); await Promise.resolve()
    const reopened = request('reopened')
    expect(bridge.consentRequest.value?.content).toBe('reopened')
    bridge.approveConsent(false); await reopened
    expect(rpcClient.call).toHaveBeenCalledTimes(1)
    await vi.runAllTimersAsync()
  })

})
