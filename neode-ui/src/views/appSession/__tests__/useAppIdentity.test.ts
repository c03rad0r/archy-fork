import { ref } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { rpcClient } from '@/api/rpc-client'
import { useAppIdentity, type SelectedIdentity } from '../useAppIdentity'

vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))

const alice: SelectedIdentity = {
  id: 'alice-id',
  name: 'Alice',
  did: 'did:key:alice',
  pubkey: 'identity-key',
  nostr_pubkey: 'nostr-key',
}

describe('useAppIdentity explicit identity selection', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.mocked(rpcClient.call).mockResolvedValue({ signature: 'proof' })
  })

  it('reuses a stored identity normally but reopens the picker for login', async () => {
    localStorage.setItem('archipelago_app_identity_archipelago-source', JSON.stringify(alice))
    const postMessage = vi.fn()
    const frame = ref({ contentWindow: { postMessage } } as unknown as HTMLIFrameElement)
    const showPicker = ref(false)
    const identity = useAppIdentity(ref('archipelago-source'), frame, showPicker)

    identity.handleIdentityRequest()
    await vi.waitFor(() => expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'archipelago:identity', nostr_pubkey: 'nostr-key' }),
      '*',
    ))

    postMessage.mockClear()
    identity.handleIdentityRequest(true)
    expect(showPicker.value).toBe(true)
    expect(postMessage).not.toHaveBeenCalled()
  })

  it('notifies the requesting app when the chooser is cancelled', () => {
    const postMessage = vi.fn()
    const frame = ref({ contentWindow: { postMessage } } as unknown as HTMLIFrameElement)
    const showPicker = ref(true)
    const identity = useAppIdentity(ref('archipelago-source'), frame, showPicker)

    identity.cancelIdentitySelection()

    expect(showPicker.value).toBe(false)
    expect(postMessage).toHaveBeenCalledWith({ type: 'archipelago:identity-cancelled' }, '*')
  })
})
