// Stale-while-revalidate resource hook over the shared resources store.
//
// Usage:
//   const files = useCachedResource<CloudFile[]>({
//     key: 'cloud.my-files',
//     fetcher: (signal) => rpcClient.call({ method: 'content.list', signal, dedup: true }),
//     ttlMs: 30_000,
//   })
//   // template: files.data renders instantly on revisit (cache), while
//   // files.loadState === 'refreshing' drives a subtle refresh indicator.
//
// Behavior:
// - Synchronous hydrate: memory (survives navigation) → sessionStorage
//   snapshot (survives reload) → fetch.
// - Sticky-ready: never regresses ready → loading; refreshes are
//   'refreshing' so content stays on screen.
// - Stale-while-revalidate: on mount, cached data is shown immediately and a
//   background refresh runs only if the TTL has lapsed (or never fetched).
// - Keep-last-value on error, with `isStale`/`ageMs` for badges.
// - revalidateOnFocus: refreshes when the tab regains focus and the data is
//   stale (debounced by TTL, so focus-flapping is free).
// - Abort-on-unmount: the fetcher receives an AbortSignal that fires when
//   the last subscribed component unmounts.

import { computed, getCurrentScope, onActivated, onScopeDispose, type ComputedRef } from 'vue'
import { useResourcesStore, type ResourceEntry, type ResourceLoadState } from '@/stores/resources'

export interface CachedResourceOptions<T> {
  /** Cache key. Include identifying params, e.g. `peer-files:${onion}`. */
  key: string
  /** Fetch fresh data. Receives an abort signal tied to component lifetime. */
  fetcher: (signal: AbortSignal) => Promise<T>
  /** Data older than this triggers a background revalidate (default 30s). */
  ttlMs?: number
  /** Snapshot to sessionStorage so reloads paint instantly. REQUIRED (no
   * default) — `persist ?? true` was the exact footgun CR-01 hit (a wallet
   * resource omitted this and silently persisted balances to
   * sessionStorage), so every call site must make the decision explicitly.
   * Sensitive data (money, identity, peer-identity/DID/pubkey payloads)
   * MUST be `false`; static/aggregate/non-identifying data may be `true`. */
  persist: boolean
  /** Revalidate (if stale) when the window regains focus (default true). */
  revalidateOnFocus?: boolean
  /** Fetch on first use (default true). Set false for lazy resources. */
  immediate?: boolean
}

export interface CachedResource<T> {
  entry: ResourceEntry<T>
  /** Convenience computed views over the entry. */
  data: ComputedRef<T | null>
  loadState: ComputedRef<ResourceLoadState>
  error: ComputedRef<string | null>
  /** True when data exists but is older than the TTL (drive an age badge). */
  isStale: ComputedRef<boolean>
  ageMs: ComputedRef<number | null>
  /** Force a refresh now (deduped with any in-flight one). */
  refresh: () => Promise<void>
  /** Mark stale + debounce-refresh all mounted users of this key. */
  invalidate: () => void
  /** Optimistically update cached data; returns rollback for RPC failure. */
  optimistic: (update: (current: T | null) => T) => () => void
}

export function useCachedResource<T>(opts: CachedResourceOptions<T>): CachedResource<T> {
  const store = useResourcesStore()
  const ttlMs = opts.ttlMs ?? 30_000
  const persist = opts.persist
  const entry = store.entry<T>(opts.key, persist)

  const aborter = new AbortController()
  const fetcher = () => opts.fetcher(aborter.signal)
  const refresh = () => store.refresh(opts.key, fetcher, { persist })

  const stale = () => entry.fetchedAt === null || Date.now() - entry.fetchedAt > ttlMs
  const refreshIfStale = () => {
    if (stale()) void refresh()
  }

  // Register as a live revalidator so invalidate(key) reaches us.
  const unsubscribe = store.subscribe(opts.key, () => void refresh())

  const onFocus = () => refreshIfStale()
  if (opts.revalidateOnFocus ?? true) {
    window.addEventListener('focus', onFocus)
  }

  // Tied to the owning effect scope (component setup or manual scope);
  // outside any scope (tests, module init) there's nothing to dispose.
  if (getCurrentScope()) {
    onScopeDispose(() => {
      unsubscribe()
      window.removeEventListener('focus', onFocus)
      aborter.abort()
    })

    // Reactivation (a KeepAlive'd component being shown again) is a distinct
    // trigger from mount and from window focus: onScopeDispose doesn't fire
    // on deactivate (the instance is preserved, not destroyed), and the
    // focus listener doesn't fire on an in-SPA tab switch (the window never
    // loses focus). Without this a kept-alive tab would paint instantly
    // forever and never revalidate. Vue no-ops onActivated outside a
    // <KeepAlive> boundary, so this is safe for every existing consumer.
    //
    // `immediate: false` resources are a distinct case (found in 02-04's
    // audit, once Cloud.vue/Server.vue's main-tab paths joined
    // KEEP_ALIVE_PATHS): `stale()` is true for any never-fetched entry
    // (`fetchedAt === null`), so a bare `refreshIfStale()` here would fire
    // the fetch the moment the tab is first activated — defeating a resource
    // deliberately marked "fetch on first use" (e.g. a tab-gated fetch that
    // should wait for the user to open that sub-tab). Only auto-revalidate
    // an `immediate: false` resource on activation once it has actually been
    // fetched at least once; before that, activation is a no-op and the
    // resource's own explicit trigger (a watcher, an onMounted/onActivated
    // "kick") still owns the first fetch.
    onActivated(() => {
      if (opts.immediate === false && entry.fetchedAt === null) return
      refreshIfStale()
    })
  }

  if (opts.immediate ?? true) refreshIfStale()

  return {
    entry,
    data: computed(() => entry.data),
    loadState: computed(() => entry.loadState),
    error: computed(() => entry.error),
    isStale: computed(() => entry.data !== null && stale()),
    ageMs: computed(() => (entry.fetchedAt === null ? null : Date.now() - entry.fetchedAt)),
    refresh,
    invalidate: () => store.invalidate(opts.key),
    // Pass this resource's own already-decided `persist` through explicitly
    // (WR-04) — store.optimistic() requires it rather than defaulting, so
    // the entry's persist decision can never silently diverge by omission.
    optimistic: (update) => store.optimistic<T>(opts.key, update, persist),
  }
}
