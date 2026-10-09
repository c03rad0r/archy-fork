import { flushPromises, shallowMount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { reactive } from 'vue'
import NostrIdentityPicker from '@/components/NostrIdentityPicker.vue'
import NostrTabSigner from '@/views/NostrTabSigner.vue'

describe('NostrTabSigner visibility', () => {
  beforeEach(() => {
    localStorage.clear()
    window.history.replaceState({}, '', '/nostr-signer')
  })

  afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers() })

  function parentMessage(data: Record<string, unknown>) {
    const event = new MessageEvent('message', { data, origin: window.location.origin })
    Object.defineProperty(event, 'source', { value: window.parent })
    window.dispatchEvent(event)
  }

  it('sends cloneable public identity fields and hides the frame after picker selection', async () => {
    vi.useFakeTimers()
    const postMessage = vi.spyOn(window.parent, 'postMessage').mockImplementation(message => {
      structuredClone(message) // Browser postMessage rejects Vue reactive proxies.
    })
    const wrapper = shallowMount(NostrTabSigner)
    try {
      parentMessage({ type: 'archipelago:signer-init', appId: 'blossom', appName: 'Blossom' })
      const identity = reactive({ id: 'profile', name: 'Alice', did: 'did:example:alice', pubkey: 'public', nostr_pubkey: 'a'.repeat(64) })
      wrapper.findComponent({ name: 'NostrIdentityPicker' }).vm.$emit('select', identity)
      await Promise.resolve()
      expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({ type: 'archipelago:signer-identity', identity: expect.objectContaining({ id: 'profile' }) }), window.location.origin)
      await vi.advanceTimersByTimeAsync(450)
      expect(postMessage).toHaveBeenCalledWith({ type: 'archipelago:signer-hide' }, window.location.origin)
    } finally { wrapper.unmount(); vi.useRealTimers() }
  })

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

  it('forgets the remembered app selection when the app signs out', async () => {
    localStorage.setItem('archipelago_app_identity_indeedhub', JSON.stringify({
      id: 'identity-a', name: 'Alice', nostr_pubkey: 'a'.repeat(64),
    }))
    localStorage.setItem('unrelated-node-identity', 'preserved')
    const wrapper = shallowMount(NostrTabSigner)
    try {
      parentMessage({ type: 'archipelago:signer-init', appId: 'indeedhub', appName: 'IndeeHub' })
      parentMessage({ type: 'archipelago:signer-clear-session' })
      await flushPromises()

      expect(localStorage.getItem('archipelago_app_identity_indeedhub')).toBeNull()
      expect(localStorage.getItem('unrelated-node-identity')).toBe('preserved')
      expect(wrapper.findComponent(NostrIdentityPicker).props('show')).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('hands off a reactive picker identity as cloneable public fields and releases the overlay', async () => {
    vi.useFakeTimers()
    const sent: Array<Record<string, unknown>> = []
    // Real cross-frame postMessage performs structured cloning. A normal spy
    // would miss the browser-only DataCloneError that leaves a grey overlay.
    vi.spyOn(window.parent, 'postMessage').mockImplementation((message) => {
      sent.push(structuredClone(message))
    })
    const wrapper = shallowMount(NostrTabSigner)
    parentMessage({type: 'archipelago:signer-init', appId: 'indeedhub', appName: 'IndeeHub'})
    const selected = reactive({
      id: 'identity-a', name: 'Alice', did: 'did:key:test', pubkey: 'public-node-key',
      nostr_pubkey: 'a'.repeat(64), nostr_npub: 'npub-test', internal_metadata: 'must-stay-local',
    })
    expect(() => structuredClone(selected)).toThrow()
    wrapper.findComponent(NostrIdentityPicker).vm.$emit('select', selected)
    await flushPromises()
    const identityMessage = sent.find(message => message.type === 'archipelago:signer-identity')
    expect(identityMessage?.identity).toEqual({
      id: 'identity-a', name: 'Alice', did: 'did:key:test', pubkey: 'public-node-key',
      nostr_pubkey: 'a'.repeat(64), nostr_npub: 'npub-test',
    })
    await vi.advanceTimersByTimeAsync(400)
    expect(sent).toContainEqual({type: 'archipelago:signer-hide'})
    expect(wrapper.findComponent(NostrIdentityPicker).props('show')).toBe(false)
    wrapper.unmount()
  })

})
