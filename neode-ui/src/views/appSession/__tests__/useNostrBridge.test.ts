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
