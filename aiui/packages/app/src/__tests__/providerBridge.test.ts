import { afterEach, describe, expect, it, vi } from 'vitest'
import { archyBridge } from '@/services/archyBridge'
const originalParent = window.parent
const origin = 'https://node.example'
afterEach(() => { archyBridge.destroy(); Object.defineProperty(window, 'parent', { value: originalParent, configurable: true }); vi.restoreAllMocks() })
describe('trusted provider setup bridge', () => {
  it('accepts configuration only from the embedding parent, rejects siblings and other origins', () => {
    const parent = { postMessage: vi.fn() }
    Object.defineProperty(window, 'parent', { value: parent, configurable: true })
    archyBridge.init(origin)
    const listener = vi.fn(); const unsubscribe = archyBridge.onProviderConfigured(listener)
    const send = (source: unknown, from: string, provider = 'openai') => window.dispatchEvent(new MessageEvent('message', { source: source as Window, origin: from, data: { type: 'ai:provider-configured', provider, model: 'test-model' } }))
    send({}, origin); send(parent, 'https://evil.example'); send(parent, origin, 'arbitrary')
    expect(listener).not.toHaveBeenCalled()
    send(parent, origin)
    expect(listener).toHaveBeenCalledExactlyOnceWith({ provider: 'openai', model: 'test-model' })
    archyBridge.requestAISetup()
    expect(parent.postMessage).toHaveBeenLastCalledWith({ type: 'ai:setup-request' }, origin)
    unsubscribe()
  })
  it('replays the selection when the composer mounts after the handshake', () => {
    const parent = { postMessage: vi.fn() }; Object.defineProperty(window, 'parent', { value: parent, configurable: true })
    archyBridge.init(origin)
    window.dispatchEvent(new MessageEvent('message', { source: parent as unknown as Window, origin, data: { type: 'ai:provider-configured', provider: 'local' } }))
    const listener = vi.fn(); const unsubscribe = archyBridge.onProviderConfigured(listener)
    expect(listener).toHaveBeenCalledExactlyOnceWith({ provider: 'local', model: '' }); unsubscribe()
  })
})
