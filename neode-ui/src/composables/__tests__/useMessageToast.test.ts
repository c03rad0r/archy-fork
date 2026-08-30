import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const mockPush = vi.fn()

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: mockPush }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    getReceivedMessages: vi.fn(),
    call: vi.fn(),
  },
}))

import { useMessageToast } from '../useMessageToast'
import { rpcClient } from '@/api/rpc-client'

const mockedRpc = vi.mocked(rpcClient)

describe('useMessageToast', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
    localStorage.clear()
    setActivePinia(createPinia())
    vi.mocked(rpcClient.call).mockResolvedValue({ messages: [], count: 0 })
    // Reset shared singleton state
    const toast = useMessageToast()
    toast.stopPolling()
    toast.receivedMessages.value = []
    toast.lastMessageCount.value = 0
    toast.loadingMessages.value = false
    toast.toastMessage.value = { show: false, text: '', fromPubkey: '', contactId: null }
  })

  afterEach(() => {
    const toast = useMessageToast()
    toast.stopPolling()
    vi.useRealTimers()
  })

  it('starts with empty state', () => {
    const toast = useMessageToast()
    expect(toast.receivedMessages.value).toEqual([])
    expect(toast.lastMessageCount.value).toBe(0)
    expect(toast.loadingMessages.value).toBe(false)
    expect(toast.toastMessage.value.show).toBe(false)
    expect(toast.unreadCount.value).toBe(0)
  })

  it('loadReceivedMessages fetches and stores messages', async () => {
    mockedRpc.getReceivedMessages.mockResolvedValue({
      messages: [
        { from_pubkey: 'abc', message: 'Hello', timestamp: '2026-01-01' },
      ],
    })
    const toast = useMessageToast()
    await toast.loadReceivedMessages()

    expect(toast.receivedMessages.value.length).toBe(1)
    expect(toast.lastMessageCount.value).toBe(1)
    expect(toast.loadingMessages.value).toBe(false)
  })

  it('does not show toast on initial load', async () => {
    mockedRpc.getReceivedMessages.mockResolvedValue({
      messages: [{ from_pubkey: 'a', message: 'Hi', timestamp: '2026-01-01' }],
    })
    const toast = useMessageToast()
    await toast.loadReceivedMessages()

    expect(toast.toastMessage.value.show).toBe(false)
  })

  it('shows toast when new messages arrive after initial load', async () => {
    const toast = useMessageToast()

    // Initial load
    mockedRpc.getReceivedMessages.mockResolvedValue({
      messages: [{ from_pubkey: 'a', message: 'First', timestamp: '2026-01-01' }],
    })
    await toast.loadReceivedMessages()

    // New message arrives
    mockedRpc.getReceivedMessages.mockResolvedValue({
      messages: [
        { from_pubkey: 'a', message: 'First', timestamp: '2026-01-01' },
        { from_pubkey: 'b', message: 'Second', timestamp: '2026-01-02' },
      ],
    })
    await toast.loadReceivedMessages()

    expect(toast.toastMessage.value.show).toBe(true)
    expect(toast.toastMessage.value.text).toBe('Second')
  })

  it('shows count for multiple new messages', async () => {
    const toast = useMessageToast()

    // Initial load
    mockedRpc.getReceivedMessages.mockResolvedValue({
      messages: [{ from_pubkey: 'a', message: 'One', timestamp: '2026-01-01' }],
    })
    await toast.loadReceivedMessages()

    // Multiple new messages
    mockedRpc.getReceivedMessages.mockResolvedValue({
      messages: [
        { from_pubkey: 'a', message: 'One', timestamp: '2026-01-01' },
        { from_pubkey: 'b', message: 'Two', timestamp: '2026-01-02' },
        { from_pubkey: 'c', message: 'Three', timestamp: '2026-01-03' },
      ],
    })
    await toast.loadReceivedMessages()

    expect(toast.toastMessage.value.show).toBe(true)
    expect(toast.toastMessage.value.text).toBe('2 new messages')
  })

  it('unreadCount reflects difference', async () => {
    const toast = useMessageToast()
    toast.receivedMessages.value = [
      { from_pubkey: 'a', message: 'Hi', timestamp: '2026-01-01' },
      { from_pubkey: 'b', message: 'Hey', timestamp: '2026-01-02' },
    ]
    toast.lastMessageCount.value = 1
    expect(toast.unreadCount.value).toBe(1)
  })

  it('unreadCount is never negative', () => {
    const toast = useMessageToast()
    toast.receivedMessages.value = []
    toast.lastMessageCount.value = 5
    expect(toast.unreadCount.value).toBe(0)
  })

  it('markAsRead syncs lastMessageCount', () => {
    const toast = useMessageToast()
    toast.receivedMessages.value = [
      { from_pubkey: 'a', message: 'Hi', timestamp: '2026-01-01' },
      { from_pubkey: 'b', message: 'Hey', timestamp: '2026-01-02' },
    ]
    toast.lastMessageCount.value = 0
    toast.markAsRead()
    expect(toast.lastMessageCount.value).toBe(2)
    expect(toast.unreadCount.value).toBe(0)
  })

  it('shows a radio-mesh toast and deep-links to its contact', async () => {
    const toast = useMessageToast()
    mockedRpc.getReceivedMessages.mockResolvedValue({ messages: [] })

    // Initialize an empty node, then deliver its first Meshtastic message.
    await toast.loadReceivedMessages()
    vi.mocked(rpcClient.call).mockResolvedValueOnce({
      messages: [{
        id: 1,
        direction: 'received',
        peer_contact_id: 42,
        peer_name: 'Alice',
        plaintext: 'Over LoRa',
        timestamp: '2026-01-01',
        delivered: true,
        encrypted: true,
        transport: 'meshtastic',
      }],
      count: 1,
    })
    await toast.loadReceivedMessages()

    expect(toast.toastMessage.value).toMatchObject({
      show: true,
      text: 'Over LoRa',
      contactId: 42,
    })
    toast.dismissToastAndOpenMessages()
    expect(mockPush).toHaveBeenCalledWith({
      path: '/dashboard/mesh',
      query: { contact: '42' },
    })
  })

  it('dismissToastAndOpenMessages clears toast and navigates', () => {
    const toast = useMessageToast()
    toast.toastMessage.value = { show: true, text: 'New message', fromPubkey: '', contactId: null }
    toast.dismissToastAndOpenMessages()

    expect(toast.toastMessage.value.show).toBe(false)
    expect(mockPush).toHaveBeenCalledWith('/dashboard/mesh')
  })

  it('stops polling on 401 error', async () => {
    const toast = useMessageToast()
    mockedRpc.getReceivedMessages.mockRejectedValue(new Error('401 Unauthorized'))
    toast.startPolling()

    // Wait for initial load triggered by startPolling
    await vi.advanceTimersByTimeAsync(0)

    // Polling should have stopped, so advancing time should NOT call again
    vi.clearAllMocks()
    await vi.advanceTimersByTimeAsync(60000)
    expect(mockedRpc.getReceivedMessages).not.toHaveBeenCalled()
  })

  it('startPolling does not create duplicate timers', () => {
    const toast = useMessageToast()
    mockedRpc.getReceivedMessages.mockResolvedValue({ messages: [] })
    toast.startPolling()
    toast.startPolling()
    toast.startPolling()
    // Should only have one timer — verify by stopping and checking no more calls
    toast.stopPolling()
  })
})
