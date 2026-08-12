import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

import { useResourcesStore, SCHEMA_MARKER_KEY, CURRENT_SCHEMA_VERSION } from '../resources'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    logout: vi.fn(),
  },
}))

describe('resources store — clearAll (T-02-02 logout purge)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    sessionStorage.clear()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('drops every in-memory entry', async () => {
    const store = useResourcesStore()
    await store.refresh('k1', async () => 'a', { persist: true })
    await store.refresh('k2', async () => 'b', { persist: true })
    expect(store.entries.size).toBe(2)
    store.clearAll()
    expect(store.entries.size).toBe(0)
  })

  it('removes every resource:-prefixed sessionStorage key and leaves unrelated keys untouched', async () => {
    const store = useResourcesStore()
    await store.refresh('k1', async () => 'a', { persist: true })
    sessionStorage.setItem('unrelated-app-setting', 'keep-me')

    expect(sessionStorage.getItem('resource:k1')).not.toBeNull()
    expect(sessionStorage.getItem('unrelated-app-setting')).toBe('keep-me')

    store.clearAll()

    expect(sessionStorage.getItem('resource:k1')).toBeNull()
    expect(sessionStorage.getItem('unrelated-app-setting')).toBe('keep-me')
  })

  it('cancels pending invalidate timers so a scheduled revalidate never fires post-logout', async () => {
    const store = useResourcesStore()
    await store.refresh('k1', async () => 'a', { persist: true })
    const revalidate = vi.fn()
    store.subscribe('k1', revalidate)
    store.invalidate('k1') // schedules a debounced revalidate

    store.clearAll()

    // Advance past the debounce window — the cancelled timer must not fire.
    vi.advanceTimersByTime(2000)
    expect(revalidate).not.toHaveBeenCalled()
  })

  it('drops in-flight bookkeeping so a fetch resolving after clearAll does not repopulate memory or sessionStorage', async () => {
    const store = useResourcesStore()
    let resolveOld!: (v: string) => void
    const oldFetch = new Promise<string>((res) => { resolveOld = res })
    const p = store.refresh('k2', () => oldFetch, { persist: true })

    store.clearAll()
    // The key was dropped from `entries` by clearAll — a fresh lookup must
    // not see the old session's in-flight fetch as still pending for it.
    expect(store.entries.has('k2')).toBe(false)

    resolveOld('stale-session-data')
    await p

    // The old fetch resolved after clearAll's generation bump — its result
    // must be dropped, not written into a fresh entry or sessionStorage.
    expect(store.entries.has('k2')).toBe(false)
    const e = store.entry<string>('k2', true)
    expect(e.data).toBeNull()
    expect(sessionStorage.getItem('resource:k2')).toBeNull()
  })

  it('does not throw when sessionStorage.removeItem throws', async () => {
    const store = useResourcesStore()
    await store.refresh('k1', async () => 'a', { persist: true })
    const spy = vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => {
      throw new Error('sessionStorage unavailable')
    })
    expect(() => store.clearAll()).not.toThrow()
    spy.mockRestore()
  })

  it('does not throw when sessionStorage.key/length throw', async () => {
    const store = useResourcesStore()
    await store.refresh('k1', async () => 'a', { persist: true })
    const spy = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
      throw new Error('sessionStorage inaccessible')
    })
    expect(() => store.clearAll()).not.toThrow()
    spy.mockRestore()
  })
})

describe('auth store — logout purges the cache even when the RPC rejects', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    sessionStorage.clear()
    vi.resetModules()
  })

  it('clears the resources cache after a rejected rpcClient.logout()', async () => {
    const { rpcClient } = await import('@/api/rpc-client')
    vi.mocked(rpcClient.logout).mockRejectedValueOnce(new Error('network down'))

    const resources = useResourcesStore()
    await resources.refresh('some.cached.key', async () => 'sensitive-data', { persist: true })
    expect(resources.entries.size).toBe(1)

    const { useAuthStore } = await import('../auth')
    const auth = useAuthStore()
    await auth.logout()

    expect(resources.entries.size).toBe(0)
    expect(sessionStorage.getItem('resource:some.cached.key')).toBeNull()
  })

  it('clears the resources cache after a successful logout', async () => {
    const { rpcClient } = await import('@/api/rpc-client')
    vi.mocked(rpcClient.logout).mockResolvedValueOnce(undefined as never)

    const resources = useResourcesStore()
    await resources.refresh('another.key', async () => 'data', { persist: true })
    expect(resources.entries.size).toBe(1)

    const { useAuthStore } = await import('../auth')
    const auth = useAuthStore()
    await auth.logout()

    expect(resources.entries.size).toBe(0)
  })
})

describe('resources store — one-time schema migration purge (CR-01 follow-up, T-02-01)', () => {
  beforeEach(() => {
    sessionStorage.clear()
    localStorage.clear()
  })

  it('purges a legacy resource: snapshot written with no marker at all (pre-migration tab)', () => {
    // Simulates the exact incident scenario: a tab that was open before this
    // migration shipped, holding a snapshot the OLD bundle wrote under the
    // OLD (now-incorrect) persist decision — e.g. CR-01's web5.lnd-info.
    sessionStorage.setItem('resource:web5.lnd-info', JSON.stringify({
      data: { balance_sats: 50000, channel_balance_sats: 25000, synced_to_chain: true },
      fetchedAt: Date.now(),
    }))

    setActivePinia(createPinia())
    useResourcesStore() // store setup() runs the migration check synchronously

    expect(sessionStorage.getItem('resource:web5.lnd-info')).toBeNull()
  })

  it('purges every resource: snapshot when the marker is present but stale (persist contract changed since)', () => {
    sessionStorage.setItem(SCHEMA_MARKER_KEY, 'some-older-version-string')
    sessionStorage.setItem('resource:some-key', JSON.stringify({ data: 'stale-contract-data', fetchedAt: Date.now() }))

    setActivePinia(createPinia())
    useResourcesStore()

    expect(sessionStorage.getItem('resource:some-key')).toBeNull()
  })

  it('writes the current marker after purging, so this is a one-time cost per tab session', () => {
    sessionStorage.setItem('resource:legacy', JSON.stringify({ data: 'x', fetchedAt: Date.now() }))

    setActivePinia(createPinia())
    useResourcesStore()

    expect(sessionStorage.getItem(SCHEMA_MARKER_KEY)).toBe(CURRENT_SCHEMA_VERSION)
  })

  it('does NOT purge — a snapshot written under the CURRENT marker survives a later store init (one-time, not every-boot)', () => {
    // Marker already matches CURRENT_SCHEMA_VERSION, as if this same tab
    // already ran the migration once (e.g. an earlier navigation/reload).
    sessionStorage.setItem(SCHEMA_MARKER_KEY, CURRENT_SCHEMA_VERSION)
    const snapshot = JSON.stringify({ data: 'keep-me', fetchedAt: Date.now() })
    sessionStorage.setItem('resource:already-migrated-key', snapshot)

    // A second store "init" — e.g. a fresh Pinia instance, standing in for
    // another reload of the same tab (sessionStorage persists across it).
    setActivePinia(createPinia())
    useResourcesStore()

    // Byte-for-byte untouched — the migration was a no-op this time.
    expect(sessionStorage.getItem('resource:already-migrated-key')).toBe(snapshot)
  })

  it('a persist:false key never writes a sessionStorage snapshot in the first place', async () => {
    setActivePinia(createPinia())
    const store = useResourcesStore()
    await store.refresh('web5.lnd-info', async () => ({ balance_sats: 1, channel_balance_sats: 2 }), { persist: false })

    expect(sessionStorage.getItem('resource:web5.lnd-info')).toBeNull()
  })

  it('migration purge is strictly bounded to the resource: prefix — every other storage key (localStorage auth, onboarding seed scratch, PWA/splash flags, hand-rolled view caches) survives untouched', () => {
    // A representative sample of this app's real non-`resource:` storage
    // usage, seeded BEFORE a legacy snapshot forces the purge to actually
    // run — proving the purge cannot log anyone out or destroy unrelated
    // state (Dorian's blast-radius requirement).
    localStorage.setItem('neode-auth', 'true') // auth session flag (localStorage, not sessionStorage — untouched by construction)
    sessionStorage.setItem('archipelago_from_boot', '1')
    sessionStorage.setItem('archipelago_from_splash', '1')
    sessionStorage.setItem('_seed_words', JSON.stringify(['abandon', 'ability']))
    sessionStorage.setItem('_seed_challenge_indices', JSON.stringify([1, 2, 3]))
    sessionStorage.setItem('archipelago.web5.identities.v1', JSON.stringify({ v: 1, savedAt: Date.now(), identities: [] }))
    sessionStorage.setItem('archipelago.fleet.cache.v1', JSON.stringify({ v: 1 }))
    sessionStorage.setItem('unrelated-app-setting', 'keep-me')
    // The thing that actually triggers the purge this run:
    sessionStorage.setItem('resource:legacy-key', JSON.stringify({ data: 'x', fetchedAt: Date.now() }))

    setActivePinia(createPinia())
    useResourcesStore()

    // The purge fired (proves the test actually exercised the purge path).
    expect(sessionStorage.getItem('resource:legacy-key')).toBeNull()

    // Everything else survives byte-for-byte.
    expect(localStorage.getItem('neode-auth')).toBe('true')
    expect(sessionStorage.getItem('archipelago_from_boot')).toBe('1')
    expect(sessionStorage.getItem('archipelago_from_splash')).toBe('1')
    expect(sessionStorage.getItem('_seed_words')).toBe(JSON.stringify(['abandon', 'ability']))
    expect(sessionStorage.getItem('_seed_challenge_indices')).toBe(JSON.stringify([1, 2, 3]))
    expect(sessionStorage.getItem('archipelago.web5.identities.v1')).not.toBeNull()
    expect(sessionStorage.getItem('archipelago.fleet.cache.v1')).not.toBeNull()
    expect(sessionStorage.getItem('unrelated-app-setting')).toBe('keep-me')
  })

  it('the migration cannot race an in-flight fetch — it runs synchronously at store setup, before any entry()/refresh() call is possible, and a refresh immediately afterward behaves normally', async () => {
    sessionStorage.setItem('resource:legacy-key', JSON.stringify({ data: 'x', fetchedAt: Date.now() }))

    setActivePinia(createPinia())
    const store = useResourcesStore()

    // Nothing was, or could have been, in flight during the migration: the
    // store's internal bookkeeping is still pristine immediately after
    // construction (entries empty, no leaked in-flight promise).
    expect(store.entries.size).toBe(0)

    // A normal refresh right after store construction is unaffected by the
    // migration having just run — proves no corruption/wedged state.
    await store.refresh('fresh-key', async () => 'fresh-data', { persist: true })
    const e = store.entry<string>('fresh-key', true)
    expect(e.data).toBe('fresh-data')
    expect(e.loadState).toBe('ready')
    expect(sessionStorage.getItem('resource:fresh-key')).not.toBeNull()
  })
})
