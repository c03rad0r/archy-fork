import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useWeb5BadgeStore } from '../web5Badge'

// Mock rpcClient
vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

import { rpcClient } from '@/api/rpc-client'

describe('useWeb5BadgeStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('starts with zero pending requests', () => {
    const store = useWeb5BadgeStore()
    expect(store.pendingRequestCount).toBe(0)
  })

  it('refresh updates count from API', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({
      requests: [{ id: '1' }, { id: '2' }, { id: '3' }],
    })

    const store = useWeb5BadgeStore()
    await store.refresh()

    expect(store.pendingRequestCount).toBe(3)
    expect(rpcClient.call).toHaveBeenCalledWith({ method: 'network.list-requests' })
  })

  it('refresh handles empty requests', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({ requests: [] })

    const store = useWeb5BadgeStore()
    await store.refresh()

    expect(store.pendingRequestCount).toBe(0)
  })

  it('refresh handles null requests gracefully', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({ requests: null })

    const store = useWeb5BadgeStore()
    await store.refresh()

    expect(store.pendingRequestCount).toBe(0)
  })

  it('refresh handles API error gracefully', async () => {
    vi.mocked(rpcClient.call).mockRejectedValueOnce(new Error('Network error'))

    const store = useWeb5BadgeStore()
    store.pendingRequestCount = 5 // pre-existing value
    await store.refresh()

    // Should not throw, count stays at pre-existing value (error swallowed)
    expect(store.pendingRequestCount).toBe(5)
  })

  it('refresh updates count on subsequent calls', async () => {
    vi.mocked(rpcClient.call)
      .mockResolvedValueOnce({ requests: [{ id: '1' }] })
      .mockResolvedValueOnce({ requests: [{ id: '1' }, { id: '2' }] })

    const store = useWeb5BadgeStore()
    await store.refresh()
    expect(store.pendingRequestCount).toBe(1)

    await store.refresh()
    expect(store.pendingRequestCount).toBe(2)
  })
})
