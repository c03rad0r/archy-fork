import { shallowMount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import NostrTabSigner from '@/views/NostrTabSigner.vue'

describe('NostrTabSigner visibility', () => {
  beforeEach(() => {
    localStorage.clear()
    window.history.replaceState({}, '', '/nostr-signer')
  })

  afterEach(() => vi.restoreAllMocks())

  function parentMessage(data: Record<string, unknown>) {
    const event = new MessageEvent('message', { data, origin: window.location.origin })
    Object.defineProperty(event, 'source', { value: window.parent })
    window.dispatchEvent(event)
  }

  it('does not reveal the full-screen frame for a silent remembered request', async () => {
    localStorage.setItem('archipelago_app_identity_archipelago-source', JSON.stringify({
      id: 'identity-a',
      name: 'Alice',
      nostr_pubkey: 'abc123',
    }))
    const postMessage = vi.spyOn(window.parent, 'postMessage')
    const wrapper = shallowMount(NostrTabSigner)
    expect(document.documentElement.classList.contains('nostr-signer-route')).toBe(true)
    expect(document.body.classList.contains('nostr-signer-route')).toBe(true)

    parentMessage({
      type: 'archipelago:signer-init',
      appId: 'archipelago-source',
      appName: 'GitWorkshop',
    })
    postMessage.mockClear()

    parentMessage({ type: 'nostr-request', id: 1, method: 'getRelays', params: {} })
    await Promise.resolve()

    expect(postMessage).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'archipelago:signer-show' }),
      expect.anything(),
    )

    wrapper.unmount()
    expect(document.documentElement.classList.contains('nostr-signer-route')).toBe(false)
    expect(document.body.classList.contains('nostr-signer-route')).toBe(false)
  })
})
