// Route classification for the Dashboard KeepAlive host (D-01/D-03/D-04).
//
// Exact-match only — a prefix match would sweep in secondary screens like
// `/dashboard/marketplace/:id`, which D-04 explicitly excludes from the
// instance cache (unbounded item counts would bloat it). Deliberately not
// derived from `isDetailRoute()` in useRouteTransitions.ts: that helper only
// recognises `/apps/` and `/marketplace/` details and misses `cloud/:folderId`,
// `server/openwrt`, `web5/credentials`, `goals/:goalId` and `app-session/:appId`.

import type { RouteLocationNormalizedLoaded } from 'vue-router'
import { TAB_ORDER } from './useRouteTransitions'

/** Maximum number of main-tab component instances kept alive at once (D-03).
 * Bounds memory on low-power fleet nodes — the oldest unused instance evicts
 * once this cap is exceeded (Vue's own LRU eviction).
 *
 * 02-08 (FA-D) tuned this against an on-device measurement rather than
 * leaving the carried-forward estimate unexamined: on a test node (real
 * node hardware, deployed build), a headless Chromium session logged in via
 * the real UI, cycled every main tab (all of TAB_ORDER incl. the withheld
 * `/dashboard/settings`, plus `/dashboard/discover` — 11 tabs) through 4 full
 * two-way round-trips (44 navigations total, well past the single "twice"
 * the task calls for), reading the JS heap via CDP `Performance.getMetrics`
 * before and after each cycle. Result: ~8.8MB before any navigation, then
 * 10.02 / 20.69 / 16.67 / 14.12 MB after cycles 0-3 respectively (final
 * reading 14.68MB) — fluctuating, not monotonically growing, across 10
 * distinct cache-registered paths exceeding this cap. That confirms the
 * eviction is actually bounding memory rather than the cap sitting unused,
 * and the absolute growth (single-digit MB) is inconsequential on any
 * low-power fleet node's available RAM. Left at 6, unchanged — a measured,
 * confirmed 6 rather than the FA-D estimate it replaces. */
export const KEEP_ALIVE_MAX = 6

/** TAB_ORDER paths deliberately withheld from the instance cache this plan
 * (02-04) would otherwise register. Two distinct reasons populate this set —
 * kept in one place so `KEEP_ALIVE_PATHS` stays a single derivation:
 *
 * 1. Measured `already fast` in 02-FINDINGS.md with `Remounted: false` — no
 *    instance cache to gain (D-02). (None as of 02-04: every TAB_ORDER path
 *    measured `Remounted: true` (Home, Apps, Marketplace, Cloud, Web5,
 *    Fleet) or was `unmeasured` (Mesh, Chat) — neither unmeasured row has a
 *    `Remounted: false`, so per the plan's literal exclusion rule both stay
 *    registered, and their lifecycle was audited in 02-04's Task 1.)
 * 2. Unaudited-risk (02-04 Rule-3 deviation, not in 02-FINDINGS.md at all):
 *    `/dashboard/settings` is in TAB_ORDER but neither Settings.vue nor any
 *    of its child sections are in this plan's Task 1/2 file lists. A grep
 *    across `neode-ui/src/views/settings/*.vue` found real un-audited side
 *    effects that would misbehave under KeepAlive exactly as this plan
 *    warns against: `SystemDangerZone.vue`'s reboot poll/elapsed intervals,
 *    and one-shot `onMounted`-only fetches in `VpnStatusSection.vue`,
 *    `KioskDisplaySection.vue`, `TransportPrefsCard.vue` and
 *    `ClaudeAuthSection.vue` that would never refresh again once cached.
 *    Registering Settings without that audit would ship the exact
 *    off-screen-timer/stale-data bug this plan exists to prevent — excluded
 *    until a future plan audits it the way Task 1 did for Home/Web5/Chat/
 *    Cloud/Server/Mesh. */
const WITHHELD_FROM_CACHE: ReadonlySet<string> = new Set([
  '/dashboard/settings',
])

/** Paths whose component instance survives a tab switch (D-01), derived from
 * the single-sourced TAB_ORDER main-tab list (plus `/dashboard/discover`,
 * the App Store's second tab, measured `remount storm` in 02-FINDINGS.md but
 * not part of TAB_ORDER) rather than restating every path literally — a
 * future tab addition to TAB_ORDER is registered automatically instead of
 * silently missing registration.
 *
 * 02-04 widened this from the 02-02 tracer's single seed path after
 * auditing every main tab's mount/activate/deactivate lifecycle (see
 * 02-04-SUMMARY.md's per-view side-effect table) — every registered path's
 * timers, subscriptions and window listeners are now placed for an
 * activate/deactivate lifecycle, so it's safe for their instances to
 * survive a tab switch. */
export const KEEP_ALIVE_PATHS: ReadonlySet<string> = new Set([
  ...TAB_ORDER.filter((path) => !WITHHELD_FROM_CACHE.has(path)),
  '/dashboard/discover',
])

/** True only for an exact path match against KEEP_ALIVE_PATHS. */
export function shouldKeepAlive(route: RouteLocationNormalizedLoaded | { path: string }): boolean {
  return KEEP_ALIVE_PATHS.has(route.path)
}
