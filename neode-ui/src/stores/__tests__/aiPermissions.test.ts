import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAIPermissionsStore, AI_PERMISSION_CATEGORIES } from '../aiPermissions'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))

const STORAGE_KEY = 'archipelago-ai-permissions'

describe('useAIPermissionsStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.clearAllMocks()
  })

  it('starts with empty permissions when no localStorage', () => {
    const store = useAIPermissionsStore()
    expect(store.enabled.size).toBe(0)
    expect(store.noneEnabled).toBe(true)
    expect(store.allEnabled).toBe(false)
  })

  it('loads valid categories from localStorage', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(['apps', 'system']))
    setActivePinia(createPinia())
    const store = useAIPermissionsStore()
    expect(store.isEnabled('apps')).toBe(true)
    expect(store.isEnabled('system')).toBe(true)
    expect(store.enabled.size).toBe(2)
  })

  it('filters invalid categories from localStorage', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(['apps', 'invalid-category', 'system']))
    setActivePinia(createPinia())
    const store = useAIPermissionsStore()
    expect(store.enabled.size).toBe(2)
    expect(store.isEnabled('apps')).toBe(true)
    expect(store.isEnabled('system')).toBe(true)
  })

  it('handles corrupt localStorage gracefully', () => {
    localStorage.setItem(STORAGE_KEY, 'not-valid-json{')
    setActivePinia(createPinia())
    const store = useAIPermissionsStore()
    expect(store.enabled.size).toBe(0)
  })

  it('toggle adds a category', () => {
    const store = useAIPermissionsStore()
    store.toggle('bitcoin')
    expect(store.isEnabled('bitcoin')).toBe(true)
  })

  it('toggle removes an enabled category', () => {
    const store = useAIPermissionsStore()
    store.toggle('bitcoin')
    store.toggle('bitcoin')
    expect(store.isEnabled('bitcoin')).toBe(false)
  })

  it('toggle persists to localStorage', () => {
    const store = useAIPermissionsStore()
    store.toggle('apps')
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]')
    expect(stored).toContain('apps')
  })

  it('enableAll enables all categories', () => {
    const store = useAIPermissionsStore()
    store.enableAll()
    expect(store.allEnabled).toBe(true)
    expect(store.enabled.size).toBe(AI_PERMISSION_CATEGORIES.length)
    for (const cat of AI_PERMISSION_CATEGORIES) {
      expect(store.isEnabled(cat.id)).toBe(true)
    }
  })

  it('disableAll disables all categories', () => {
    const store = useAIPermissionsStore()
    store.enableAll()
    store.disableAll()
    expect(store.noneEnabled).toBe(true)
    expect(store.enabled.size).toBe(0)
  })

  it('enabledCategories returns array of enabled IDs', () => {
    const store = useAIPermissionsStore()
    store.toggle('apps')
    store.toggle('network')
    expect(store.enabledCategories).toContain('apps')
    expect(store.enabledCategories).toContain('network')
    expect(store.enabledCategories.length).toBe(2)
  })

  it('AI_PERMISSION_CATEGORIES has 10 categories', () => {
    expect(AI_PERMISSION_CATEGORIES.length).toBe(10)
  })

  it('all categories have required fields', () => {
    for (const cat of AI_PERMISSION_CATEGORIES) {
      expect(cat.id).toBeTruthy()
      expect(cat.label).toBeTruthy()
      expect(cat.description).toBeTruthy()
      expect(cat.icon).toBeTruthy()
      expect(cat.group).toBeTruthy()
    }
  })

  describe('node-side persistence (grants are a property of the node, not a browser)', () => {
    it('MIGRATES local grants up when the node has none — never silently revokes them', async () => {
      // Everyone who granted permissions before this change has them only in
      // localStorage. An empty node must not wipe that on first hydrate.
      localStorage.setItem(STORAGE_KEY, JSON.stringify(['media', 'files']))
      const store = useAIPermissionsStore()
      vi.mocked(rpcClient.call).mockResolvedValueOnce({ granted: [] } as never)

      await store.hydrate()

      expect(store.isEnabled('media')).toBe(true)
      expect(store.isEnabled('files')).toBe(true)
      expect(vi.mocked(rpcClient.call).mock.calls.some(
        ([a]) => (a as { method?: string }).method === 'ai.permissions.set',
      )).toBe(true)
    })

    it('adopts the node grants on a browser that has none — the new-device case', async () => {
      const store = useAIPermissionsStore()
      vi.mocked(rpcClient.call).mockResolvedValueOnce({ granted: ['wallet'] } as never)

      await store.hydrate()

      expect(store.isEnabled('wallet')).toBe(true)
      // and it is cached locally so the next paint is instant
      expect(JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]')).toContain('wallet')
    })

    it('lets the node REVOKE a grant this browser still remembers', async () => {
      // The node is authoritative. A revocation made elsewhere must win, or
      // revoking would be impossible from any second device.
      localStorage.setItem(STORAGE_KEY, JSON.stringify(['media', 'wallet']))
      const store = useAIPermissionsStore()
      vi.mocked(rpcClient.call).mockResolvedValueOnce({ granted: ['media'] } as never)

      await store.hydrate()

      expect(store.isEnabled('media')).toBe(true)
      expect(store.isEnabled('wallet')).toBe(false)
    })

    it('keeps local grants when the node is unreachable rather than blanking them', async () => {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(['media']))
      const store = useAIPermissionsStore()
      vi.mocked(rpcClient.call).mockRejectedValueOnce(new Error('offline'))

      await store.hydrate()

      expect(store.isEnabled('media')).toBe(true)
      expect(store.hydrated).toBe(true)
    })

    it('ignores categories the node reports that this build does not know', async () => {
      const store = useAIPermissionsStore()
      vi.mocked(rpcClient.call).mockResolvedValueOnce({ granted: ['media', 'not-a-category'] } as never)

      await store.hydrate()

      expect(store.isEnabled('media')).toBe(true)
      expect(store.enabledCategories).not.toContain('not-a-category')
    })

    it('pushes every toggle to the node', async () => {
      const store = useAIPermissionsStore()
      vi.mocked(rpcClient.call).mockResolvedValue({ granted: [] } as never)

      store.toggle('media')
      await Promise.resolve()

      expect(vi.mocked(rpcClient.call).mock.calls.some(
        ([a]) => (a as { method?: string }).method === 'ai.permissions.set',
      )).toBe(true)
    })
  })
})
