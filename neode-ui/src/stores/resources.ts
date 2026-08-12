// Shared cache for RPC-backed page data (the "stale-while-revalidate" layer).
//
// Pages used to fetch-on-mount with a spinner on every navigation — Dashboard
// keys its router-view by route.path, so each visit unmounted and refetched
// everything. This store is the single place resource state lives instead:
// keyed entries survive navigation (Pinia) and reloads (sessionStorage
// snapshot), and `useCachedResource` renders them instantly while
// revalidating in the background.
//
// Semantics (generalized from homeStatus.ts / useFleetData.ts, the proven
// hand-rolled versions):
// - sticky-ready: once a key is 'ready' it never regresses to 'loading';
//   refreshes show as 'refreshing' so the UI keeps the data visible.
// - keep-last-known-value on error: a failed revalidate leaves data in place
//   (with `error` set and `fetchedAt` untouched → age badge shows staleness).
// - in-flight dedup per key: concurrent refreshes collapse into one fetch.

import { defineStore } from 'pinia'
import { reactive } from 'vue'

export type ResourceLoadState = 'idle' | 'loading' | 'ready' | 'refreshing' | 'error'

export interface ResourceEntry<T = unknown> {
  data: T | null
  loadState: ResourceLoadState
  /** Epoch ms of the last SUCCESSFUL fetch (drives TTL + stale badges). */
  fetchedAt: number | null
  error: string | null
}

const SNAPSHOT_PREFIX = 'resource:'

// One-time legacy-snapshot purge (CR-01 follow-up, T-02-01). Bumping a cache
// key's persist decision (e.g. Web5.vue's web5.lnd-info going true → false)
// only stops FUTURE writes — a tab that was already open before the update
// ships reloads in-place onto the new bundle and keeps whatever the OLD
// bundle already wrote to sessionStorage under the old decision, forever
// (nothing else ever purges a `resource:` snapshot except clearAll() on
// logout). This marker/version pair is what detects that case:
//   - marker present + matches CURRENT_SCHEMA_VERSION: this tab's snapshots
//     were written under the CURRENT persist contract — trust them, no-op.
//   - marker absent (tab predates this migration) or stale (persist
//     contract changed since this tab last checked): every `resource:`
//     snapshot in this session is untrustworthy — purge all of them, then
//     write the current marker so this is a one-time cost per tab session,
//     not a per-navigation/per-reload one (sessionStorage — and this
//     marker — survive reloads of the same tab; only a brand-new tab starts
//     with no marker at all, and a brand-new tab has nothing to purge).
//
// CONTRACT: bump CURRENT_SCHEMA_VERSION any time a cache key's `persist`
// value changes (true -> false, or a newly-added key defaults the wrong
// way) so the next deploy automatically purges any snapshot written under
// the old, now-incorrect decision. Forgetting to bump this is exactly how
// CR-01 stayed exposed for updating users even after the code fix landed.
// Exported so tests can assert against the real marker key/version instead
// of a hardcoded duplicate that could silently drift from this file.
export const SCHEMA_MARKER_KEY = 'resource:__schema'
export const CURRENT_SCHEMA_VERSION = '1'

function readSnapshot<T>(key: string): { data: T; fetchedAt: number } | null {
  try {
    const raw = sessionStorage.getItem(SNAPSHOT_PREFIX + key)
    if (!raw) return null
    const parsed = JSON.parse(raw)
    if (parsed && typeof parsed.fetchedAt === 'number' && 'data' in parsed) return parsed
  } catch {
    /* corrupt/absent snapshot — fall through to a fresh fetch */
  }
  return null
}

function writeSnapshot(key: string, data: unknown, fetchedAt: number): void {
  try {
    sessionStorage.setItem(SNAPSHOT_PREFIX + key, JSON.stringify({ data, fetchedAt }))
  } catch {
    /* quota exceeded or unserializable — memory cache still works */
  }
}

export const useResourcesStore = defineStore('resources', () => {
  const entries = reactive(new Map<string, ResourceEntry>())
  // Non-reactive bookkeeping: in-flight fetches + active revalidators.
  const inflight = new Map<string, Promise<void>>()
  const revalidators = new Map<string, Set<() => void>>()
  const invalidateTimers = new Map<string, ReturnType<typeof setTimeout>>()
  // Records each key's persist decision at creation time (WR-04) — reused to
  // detect (and warn on, in dev) a later call for the same key disagreeing
  // about persist, since that almost always means two call sites disagree
  // about whether a cache key is safe to write to sessionStorage (T-02-01).
  const entryPersist = new Map<string, boolean>()
  // Bumped by clearAll() so an in-flight fetch from a just-ended session that
  // resolves afterward can detect it and skip writing its result — without
  // this guard the resolving promise would still call writeSnapshot() and
  // repopulate sessionStorage even though the cache was just purged.
  let generation = 0

  /** Get (or create) the reactive entry for a key, hydrating from the
   * sessionStorage snapshot on first sight so revisits after a reload paint
   * before any RPC completes. `persist` is REQUIRED (no default) so a call
   * site can never silently opt a cache key into sessionStorage by omission
   * (WR-04/T-02-01) — it only takes effect on the very first call for a
   * given key; every later call for the same key must keep passing the SAME
   * decision, asserted (dev-only warning) rather than silently reused. */
  function entry<T>(key: string, persist: boolean): ResourceEntry<T> {
    let e = entries.get(key)
    if (!e) {
      const snap = persist ? readSnapshot<T>(key) : null
      e = reactive<ResourceEntry>({
        data: snap ? snap.data : null,
        loadState: snap ? 'ready' : 'idle',
        fetchedAt: snap ? snap.fetchedAt : null,
        error: null,
      })
      entries.set(key, e)
      entryPersist.set(key, persist)
    } else if (import.meta.env.DEV && entryPersist.get(key) !== persist) {
      console.warn(
        `[resources] entry("${key}") called with persist=${persist}, but this key was already created with persist=${entryPersist.get(key)}. `
        + 'The original decision wins for the lifetime of this entry — persist is decided once per key, never per call (T-02-01).',
      )
    }
    return e as ResourceEntry<T>
  }

  /** Run `fetcher` for `key` with sticky-ready + keep-last-value semantics.
   * Concurrent calls for the same key share one in-flight fetch. `persist`
   * is REQUIRED (no default) for the same reason as entry()/optimistic()
   * (WR-04/T-02-01): `opts.persist ?? true` was the exact footgun that let
   * CR-01 happen (Web5.vue's wallet resources omitted it and silently
   * persisted), so no call site can opt a key into sessionStorage by
   * omission anymore — every caller must make the decision explicitly. */
  function refresh<T>(
    key: string,
    fetcher: () => Promise<T>,
    opts: { persist: boolean },
  ): Promise<void> {
    const existing = inflight.get(key)
    if (existing) return existing
    const startGeneration = generation
    const e = entry<T>(key, opts.persist)
    e.loadState = e.loadState === 'ready' || e.loadState === 'refreshing' ? 'refreshing' : 'loading'
    const p = (async () => {
      try {
        const data = await fetcher()
        // A clearAll() (logout) ran while this fetch was in flight — drop
        // the result rather than repopulate a cache that was just purged.
        if (generation !== startGeneration) return
        e.data = data
        e.error = null
        e.fetchedAt = Date.now()
        e.loadState = 'ready'
        if (opts.persist) writeSnapshot(key, data, e.fetchedAt)
      } catch (err) {
        if (generation !== startGeneration) return
        e.error = err instanceof Error ? err.message : String(err)
        // Keep last-known data visible; only 'error' when we have nothing.
        e.loadState = e.data !== null ? 'ready' : 'error'
      } finally {
        inflight.delete(key)
      }
    })()
    inflight.set(key, p)
    return p
  }

  /** Mark a key stale and (debounced) re-run every mounted subscriber's
   * fetcher. Call after a mutation or on a relevant WS push. */
  function invalidate(key: string, opts: { debounceMs?: number } = {}): void {
    const e = entries.get(key)
    if (e) e.fetchedAt = null
    const subs = revalidators.get(key)
    if (!subs || subs.size === 0) return
    const t = invalidateTimers.get(key)
    if (t) clearTimeout(t)
    invalidateTimers.set(
      key,
      setTimeout(() => {
        invalidateTimers.delete(key)
        for (const fn of subs) fn()
      }, opts.debounceMs ?? 800),
    )
  }

  /** Register a live revalidator for a key (used by useCachedResource);
   * returns an unsubscribe fn. */
  function subscribe(key: string, revalidate: () => void): () => void {
    let subs = revalidators.get(key)
    if (!subs) {
      subs = new Set()
      revalidators.set(key, subs)
    }
    subs.add(revalidate)
    return () => {
      subs.delete(revalidate)
    }
  }

  /** Optimistically apply `update` to the cached value; returns a rollback.
   * Pattern: rollback on RPC failure (generalized TransportPrefsCard).
   * `persist` is REQUIRED (no default) for the same reason as entry() —
   * silently falling back to persist:true here was the exact footgun WR-04
   * flagged: a caller that runs before any useCachedResource({persist:false})
   * has created the entry would otherwise start writing to sessionStorage
   * with no indication anything is wrong. */
  function optimistic<T>(key: string, update: (current: T | null) => T, persist: boolean): () => void {
    const e = entry<T>(key, persist)
    const before = e.data
    const beforeState = e.loadState
    e.data = update(before)
    if (e.loadState === 'idle' || e.loadState === 'error') e.loadState = 'ready'
    return () => {
      e.data = before
      e.loadState = beforeState
    }
  }

  /** Drop a key entirely (memory + snapshot). */
  function evict(key: string): void {
    entries.delete(key)
    try {
      sessionStorage.removeItem(SNAPSHOT_PREFIX + key)
    } catch {
      /* noop */
    }
  }

  /** Remove every `resource:`-prefixed sessionStorage key — snapshots AND
   * the schema marker itself (whichever caller triggered this always
   * rewrites its own marker/state right after, so there's no window where
   * a missing marker is user-visible). This is the ONLY place that
   * enumerates/removes `resource:` keys; clearAll() (logout, T-02-02) and
   * the one-time schema migration (CR-01 follow-up, T-02-01, below) both
   * call it rather than duplicating the loop. Strictly bounded to the
   * `resource:` prefix — every other sessionStorage/localStorage key this
   * app uses (auth's `neode-auth` in localStorage, onboarding/seed-entry
   * scratch keys, PWA/splash flags, the hand-rolled per-view caches like
   * `archipelago.web5.identities.v1`) uses a different prefix or storage
   * entirely and is untouched by this loop. */
  function purgeAllSnapshots(): void {
    try {
      const keys: string[] = []
      for (let i = 0; i < sessionStorage.length; i++) {
        const k = sessionStorage.key(i)
        if (k && k.startsWith(SNAPSHOT_PREFIX)) keys.push(k)
      }
      for (const k of keys) sessionStorage.removeItem(k)
    } catch {
      /* sessionStorage unavailable or inaccessible — memory is already clear */
    }
  }

  /** Purge every cached resource — memory and sessionStorage — so no payload
   * from this session outlives a logout or identity switch (T-02-02). Cancels
   * pending invalidate timers and drops in-flight/revalidator bookkeeping
   * first, so a fetch that resolves after this call cannot repopulate the
   * cache with data from the ending session. */
  function clearAll(): void {
    generation++
    for (const timer of invalidateTimers.values()) clearTimeout(timer)
    invalidateTimers.clear()
    inflight.clear()
    revalidators.clear()
    entries.clear()
    purgeAllSnapshots()
  }

  // One-time legacy-snapshot migration (see CURRENT_SCHEMA_VERSION's comment
  // above). Runs synchronously here, at store setup — i.e. once per Pinia
  // instance (once per tab's page load), and strictly BEFORE this store is
  // returned to any caller, so no entry()/refresh()/optimistic() call can
  // possibly be in flight yet: `entries`/`inflight` are still the fresh
  // empty Maps declared above. There is nothing for this purge to race.
  // A plain in-place reload of an already-migrated tab reads a matching
  // marker and returns immediately — the whole point is that this is cheap
  // (one sessionStorage read) on every init and only actually purges once,
  // on the specific pre-update tab that still holds legacy data.
  try {
    if (sessionStorage.getItem(SCHEMA_MARKER_KEY) !== CURRENT_SCHEMA_VERSION) {
      purgeAllSnapshots()
      sessionStorage.setItem(SCHEMA_MARKER_KEY, CURRENT_SCHEMA_VERSION)
    }
  } catch {
    /* sessionStorage unavailable — nothing was ever persisted, nothing to migrate */
  }

  return { entries, entry, refresh, invalidate, subscribe, optimistic, evict, clearAll }
})
