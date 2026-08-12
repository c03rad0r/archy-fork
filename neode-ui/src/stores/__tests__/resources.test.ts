import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

import { useResourcesStore } from '../resources'
import { useCachedResource } from '@/composables/useCachedResource'

describe('resources store — stale-while-revalidate semantics', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    sessionStorage.clear()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('first fetch goes idle → loading → ready with data', async () => {
    const store = useResourcesStore()
    const e = store.entry<string>('k1', true)
    expect(e.loadState).toBe('idle')
    const p = store.refresh('k1', async () => 'hello', { persist: true })
    expect(e.loadState).toBe('loading')
    await p
    expect(e.loadState).toBe('ready')
    expect(e.data).toBe('hello')
    expect(e.fetchedAt).not.toBeNull()
  })

  it('sticky-ready: refresh never regresses ready → loading', async () => {
    const store = useResourcesStore()
    await store.refresh('k2', async () => 1, { persist: true })
    const e = store.entry<number>('k2', true)
    const p = store.refresh('k2', async () => 2, { persist: true })
    expect(e.loadState).toBe('refreshing')
    await p
    expect(e.loadState).toBe('ready')
    expect(e.data).toBe(2)
  })

  it('keeps last-known data on refresh error (ready + error set)', async () => {
    const store = useResourcesStore()
    await store.refresh('k3', async () => 'good', { persist: true })
    const e = store.entry<string>('k3', true)
    await store.refresh('k3', async () => {
      throw new Error('boom')
    }, { persist: true })
    expect(e.data).toBe('good')
    expect(e.loadState).toBe('ready')
    expect(e.error).toBe('boom')
  })

  it('errors with no prior data land in error state', async () => {
    const store = useResourcesStore()
    await store.refresh('k4', async () => {
      throw new Error('down')
    }, { persist: true })
    const e = store.entry('k4', true)
    expect(e.loadState).toBe('error')
    expect(e.data).toBeNull()
  })

  it('dedups concurrent refreshes for the same key', async () => {
    const store = useResourcesStore()
    const fetcher = vi.fn(async () => 'once')
    const p1 = store.refresh('k5', fetcher, { persist: true })
    const p2 = store.refresh('k5', fetcher, { persist: true })
    await Promise.all([p1, p2])
    expect(fetcher).toHaveBeenCalledTimes(1)
  })

  it('hydrates a new entry from the sessionStorage snapshot', async () => {
    const store = useResourcesStore()
    await store.refresh('k6', async () => ({ n: 42 }), { persist: true })
    // Fresh pinia = fresh memory cache, same sessionStorage.
    setActivePinia(createPinia())
    const store2 = useResourcesStore()
    const e = store2.entry<{ n: number }>('k6', true)
    expect(e.loadState).toBe('ready')
    expect(e.data).toEqual({ n: 42 })
  })

  it('optimistic update applies immediately and rollback restores', async () => {
    const store = useResourcesStore()
    await store.refresh('k7', async () => ['a'], { persist: true })
    const e = store.entry<string[]>('k7', true)
    const rollback = store.optimistic<string[]>('k7', (cur) => [...(cur ?? []), 'b'], true)
    expect(e.data).toEqual(['a', 'b'])
    rollback()
    expect(e.data).toEqual(['a'])
  })

  it('invalidate marks stale and debounce-runs subscribers', async () => {
    const store = useResourcesStore()
    await store.refresh('k8', async () => 1, { persist: true })
    const revalidate = vi.fn()
    store.subscribe('k8', revalidate)
    store.invalidate('k8')
    expect(store.entry('k8', true).fetchedAt).toBeNull()
    expect(revalidate).not.toHaveBeenCalled()
    vi.advanceTimersByTime(900)
    expect(revalidate).toHaveBeenCalledTimes(1)
  })
})

describe('useCachedResource composable', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    sessionStorage.clear()
  })

  it('fetches immediately when stale and exposes reactive views', async () => {
    const fetcher = vi.fn(async () => 'data')
    const r = useCachedResource<string>({ key: 'c1', fetcher, revalidateOnFocus: false, persist: true })
    await r.refresh()
    expect(fetcher).toHaveBeenCalled()
    expect(r.data.value).toBe('data')
    expect(r.loadState.value).toBe('ready')
    expect(r.isStale.value).toBe(false)
  })

  it('does not refetch within TTL (instant render from cache)', async () => {
    const fetcher = vi.fn(async () => 'v1')
    const r1 = useCachedResource<string>({ key: 'c2', fetcher, ttlMs: 60_000, revalidateOnFocus: false, persist: true })
    await r1.refresh()
    // Second component using the same key inside the TTL: no new fetch.
    const fetcher2 = vi.fn(async () => 'v2')
    const r2 = useCachedResource<string>({ key: 'c2', fetcher: fetcher2, ttlMs: 60_000, revalidateOnFocus: false, persist: true })
    expect(r2.data.value).toBe('v1')
    expect(fetcher2).not.toHaveBeenCalled()
  })
})
