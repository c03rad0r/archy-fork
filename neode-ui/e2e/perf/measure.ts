// measureSurface() — first-visit vs revisit timing, RPC request trace, and a
// remount probe for one SURFACES row (02-01-PLAN.md Task 1).
//
// Design notes (see surfaces.ts header for the navigation rationale):
//  - Navigation between surfaces always happens via real UI clicks
//    (RouterLink/button), never `page.goto()`, so a revisit exercises actual
//    Vue Router client-side transitions rather than a full page reload.
//  - The remount probe stamps `rootSelector`'s DOM node with a unique value
//    right after first-visit paints, then reads it back after the away/back
//    round-trip. A surviving value means the component instance was reused
//    (no remount); a missing/changed value means it was destroyed and
//    recreated.
//  - RPC calls are traced via `page.on('request')`/`requestfinished`,
//    filtered to the `/rpc/v1` endpoint the rpc-client posts to. Only method
//    name + timing are recorded — no request/response bodies (T-02-06).
//  - `maxConcurrentRpc` / `rpcWallClockMs` are derived from the recorded
//    calls' [start, start+duration] intervals via a sweep-line, so a serial
//    waterfall (maxConcurrentRpc === 1, 2+ calls) is distinguishable from an
//    already-parallel fan-out without re-reading application source.

import type { Page, Request } from '@playwright/test'
import type { Surface } from './surfaces'

const RPC_PATH = '/rpc/v1'
const NEUTRAL_SELECTOR = '[data-controller-zone="sidebar"] a[href="/dashboard/settings"]'
const DEFAULT_NAV_TIMEOUT = 20_000
const DEFAULT_CONTENT_TIMEOUT = 20_000

export interface RpcCall {
  method: string
  /** Wall-clock offset (ms) from the start of the measured phase. */
  startedAtMs: number
  /** Request duration in ms (null if it never finished/failed to resolve timing). */
  durationMs: number | null
}

export interface SurfaceSample {
  firstVisitMs: number | null
  revisitMs: number | null
  firstVisitRpcCount: number | null
  revisitRpcCount: number | null
  revisitRpcCalls: RpcCall[]
  maxConcurrentRpc: number | null
  rpcWallClockMs: number | null
  /** true = component instance was reused across the away/back round-trip
   * (no remount); false = it was destroyed and recreated; null = not probed
   * (e.g. the run errored before the probe could run). */
  remounted: boolean | null
  error: string | null
}

export interface SurfaceMeasurement {
  id: string
  label: string
  path: string
  kind: Surface['kind']
  runs: number
  samples: SurfaceSample[]
  /** Median across successful samples (null if every sample errored). */
  firstVisitMs: number | null
  revisitMs: number | null
  firstVisitRpcCount: number | null
  revisitRpcCount: number | null
  /** RPC call trace from the sample nearest the median revisitMs (or the
   * last successful sample if no median could be computed). */
  revisitRpcCalls: RpcCall[]
  maxConcurrentRpc: number | null
  rpcWallClockMs: number | null
  /** Majority vote across samples that were actually probed. */
  remounted: boolean | null
  /** Set only when every sample for this surface errored. */
  error: string | null
}

export interface MeasureOptions {
  /** Number of first-visit/revisit round-trips to sample (default 3). */
  runs?: number
  navTimeoutMs?: number
  contentTimeoutMs?: number
}

function median(values: Array<number | null>): number | null {
  const nums = values.filter((v): v is number => v != null).sort((a, b) => a - b)
  if (nums.length === 0) return null
  const mid = Math.floor(nums.length / 2)
  return nums.length % 2 === 0 ? (nums[mid - 1]! + nums[mid]!) / 2 : nums[mid]!
}

/** Sweep-line over [start, start+duration] intervals — max overlap count and
 * total wall-clock span from first start to last end. */
function deriveConcurrency(calls: RpcCall[]): { maxConcurrentRpc: number | null; rpcWallClockMs: number | null } {
  const timed = calls.filter((c) => c.durationMs != null)
  if (timed.length === 0) return { maxConcurrentRpc: calls.length > 0 ? null : 0, rpcWallClockMs: calls.length > 0 ? null : 0 }
  type Edge = { at: number; delta: number }
  const edges: Edge[] = []
  let minStart = Infinity
  let maxEnd = -Infinity
  for (const c of timed) {
    const end = c.startedAtMs + (c.durationMs ?? 0)
    edges.push({ at: c.startedAtMs, delta: 1 }, { at: end, delta: -1 })
    minStart = Math.min(minStart, c.startedAtMs)
    maxEnd = Math.max(maxEnd, end)
  }
  edges.sort((a, b) => a.at - b.at)
  let running = 0
  let max = 0
  for (const e of edges) {
    running += e.delta
    if (running > max) max = running
  }
  return { maxConcurrentRpc: max, rpcWallClockMs: maxEnd - minStart }
}

interface RpcTracker {
  calls: RpcCall[]
  detach: () => void
}

function attachRpcTracker(page: Page, phaseStart: number): RpcTracker {
  const calls: RpcCall[] = []
  const pending = new Map<Request, { method: string; start: number }>()

  const isRpcRequest = (req: Request) => req.method() === 'POST' && req.url().includes(RPC_PATH)

  const onRequest = (req: Request) => {
    if (!isRpcRequest(req)) return
    let method = 'unknown'
    try {
      const body = req.postData()
      if (body) method = (JSON.parse(body) as { method?: string }).method ?? 'unknown'
    } catch {
      // Malformed/unreadable body — keep 'unknown', never persist the body itself.
    }
    pending.set(req, { method, start: Date.now() - phaseStart })
  }

  const onSettled = (req: Request) => {
    const info = pending.get(req)
    if (!info) return
    pending.delete(req)
    calls.push({ method: info.method, startedAtMs: info.start, durationMs: Date.now() - phaseStart - info.start })
  }

  page.on('request', onRequest)
  page.on('requestfinished', onSettled)
  page.on('requestfailed', onSettled)

  return {
    calls,
    detach: () => {
      page.off('request', onRequest)
      page.off('requestfinished', onSettled)
      page.off('requestfailed', onSettled)
    },
  }
}

/**
 * Click with a dismiss-and-retry guard: a stray overlay (e.g. the Companion
 * app's once-per-browser auto-show intro) can appear mid-attempt, after
 * `dismissOverlays()` already ran but before Playwright's own actionability
 * wait resolves. Splitting the timeout budget across a few short attempts,
 * re-running `dismissOverlays()` between each, clears it reliably instead of
 * burning the whole budget on a single blocked attempt.
 */
async function clickWithGuard(page: Page, selector: string, timeoutMs: number): Promise<void> {
  const attempts = 3
  const perAttemptMs = Math.max(2_000, Math.floor(timeoutMs / attempts))
  let lastErr: unknown
  for (let i = 0; i < attempts; i++) {
    await dismissOverlays(page)
    try {
      await page.locator(selector).first().click({ timeout: perAttemptMs })
      return
    } catch (err) {
      lastErr = err
    }
  }
  throw lastErr instanceof Error ? lastErr : new Error(String(lastErr))
}

async function clickChain(page: Page, steps: string[], timeoutMs: number): Promise<void> {
  for (const selector of steps) {
    await clickWithGuard(page, selector, timeoutMs)
  }
}

async function stampRoot(page: Page, rootSelector: string, mark: string): Promise<boolean> {
  return page.evaluate(
    ({ sel, mark }) => {
      const el = document.querySelector(sel) as HTMLElement | null
      if (!el) return false
      el.dataset.perfProbe = mark
      return true
    },
    { sel: rootSelector, mark }
  )
}

async function readRootProbe(page: Page, rootSelector: string): Promise<string | null> {
  return page.evaluate((sel) => {
    const el = document.querySelector(sel) as HTMLElement | null
    return el?.dataset.perfProbe ?? null
  }, rootSelector)
}

/**
 * Best-effort dismissal of a stray full-screen overlay (a filter/pairing/
 * update modal opened by the surface under test, or left over from a prior
 * one) before it blocks the next click. One surface's leftover UI must never
 * cascade a click-intercept failure into every remaining surface in the run.
 */
async function dismissOverlays(page: Page): Promise<void> {
  try {
    await page.keyboard.press('Escape')
  } catch {
    // no-op — best-effort only
  }
  // Match any "Close"-flavoured aria-label (e.g. "Close companion modal"),
  // not just an exact "Close" — modal close buttons across the app phrase
  // this inconsistently.
  const closeButtons = page.locator(
    '[role="dialog"] button[aria-label*="Close" i], .fixed.inset-0 button[aria-label*="Close" i]'
  )
  const count = await closeButtons.count().catch(() => 0)
  if (count > 0) {
    await closeButtons.first().click({ timeout: 2_000 }).catch(() => {})
  }
}

async function goHome(page: Page, timeoutMs: number): Promise<void> {
  // DashboardSidebar.vue is `v-show="!chatFullscreen"` — if a prior surface's
  // run ended sitting on /dashboard/chat (e.g. chat's own revisit re-opened
  // it as its last step, or a prior run errored out mid-chat), the sidebar is
  // hidden and unclickable. Back out of chat first so the sidebar reappears
  // before relying on it below.
  if (new URL(page.url()).pathname === '/dashboard/chat') {
    const closed = await page
      .locator('.chat-close-btn')
      .first()
      .click({ timeout: 5_000 })
      .then(() => true)
      .catch(() => false)
    // On real AIUI hardware the close pill can sit behind AIUI's own
    // "connecting" overlay for longer than any reasonable click budget — if
    // the close button itself is unreachable, this is a recovery path, not a
    // measurement, so a hard reload to break out is preferable to leaving
    // every remaining surface stuck behind a permanently hidden sidebar.
    if (!closed) {
      await page.goto('/dashboard', { waitUntil: 'domcontentloaded' }).catch(() => {})
    }
  }
  await clickWithGuard(page, '[data-controller-zone="sidebar"] a[href="/dashboard"]', timeoutMs)
  await page.waitForURL((url) => url.pathname === '/dashboard', { timeout: timeoutMs })
}

async function runOnce(page: Page, surface: Surface, opts: Required<Pick<MeasureOptions, 'navTimeoutMs' | 'contentTimeoutMs'>>): Promise<SurfaceSample> {
  const { navTimeoutMs, contentTimeoutMs } = opts

  // Start every run from a known, stable point (dashboard home) so
  // firstVisitMs measures "navigate from the dashboard home to the surface"
  // exactly as specified, regardless of what the previous surface left us on.
  await goHome(page, navTimeoutMs)

  const t0 = Date.now()
  const firstTracker = attachRpcTracker(page, t0)
  await clickChain(page, surface.navSteps, navTimeoutMs)
  await page.locator(surface.contentSelector).first().waitFor({ state: 'visible', timeout: contentTimeoutMs })
  const firstVisitMs = Date.now() - t0
  firstTracker.detach()
  const firstVisitRpcCount = firstTracker.calls.length

  const mark = `probe-${Date.now()}-${Math.random().toString(36).slice(2)}`
  const stamped = await stampRoot(page, surface.rootSelector, mark)

  // Away step: for an in-page trigger (modal), close it; otherwise navigate
  // to the neutral Settings tab via the sidebar (never back to this surface's
  // own navSteps, so the round-trip is genuine).
  //
  // Deliberately NOT routed through clickWithGuard here: dismissOverlays()
  // treats any "Close"-labelled button inside a dialog as a stray overlay to
  // dismiss — which is exactly this element when closeSelector targets the
  // surface's own dialog/panel, causing it to close a beat before this click
  // runs and leaving the click with nothing to find.
  if (surface.closeSelector) {
    await page.locator(surface.closeSelector).first().click({ timeout: navTimeoutMs })
  } else {
    await clickWithGuard(page, NEUTRAL_SELECTOR, navTimeoutMs)
    await page.waitForURL((url) => url.pathname === '/dashboard/settings', { timeout: navTimeoutMs })
  }

  const t1 = Date.now()
  const revisitTracker = attachRpcTracker(page, t1)
  await clickChain(page, surface.navSteps, navTimeoutMs)
  await page.locator(surface.contentSelector).first().waitFor({ state: 'visible', timeout: contentTimeoutMs })
  const revisitMs = Date.now() - t1
  revisitTracker.detach()
  const revisitRpcCalls = revisitTracker.calls
  const revisitRpcCount = revisitRpcCalls.length

  const remounted = stamped ? (await readRootProbe(page, surface.rootSelector)) !== mark : null

  const { maxConcurrentRpc, rpcWallClockMs } = deriveConcurrency(revisitRpcCalls)

  return {
    firstVisitMs,
    revisitMs,
    firstVisitRpcCount,
    revisitRpcCount,
    revisitRpcCalls,
    maxConcurrentRpc,
    rpcWallClockMs,
    remounted,
    error: null,
  }
}

function errorSample(err: unknown): SurfaceSample {
  return {
    firstVisitMs: null,
    revisitMs: null,
    firstVisitRpcCount: null,
    revisitRpcCount: null,
    revisitRpcCalls: [],
    maxConcurrentRpc: null,
    rpcWallClockMs: null,
    remounted: null,
    error: err instanceof Error ? err.message : String(err),
  }
}

export async function measureSurface(page: Page, surface: Surface, opts: MeasureOptions = {}): Promise<SurfaceMeasurement> {
  const runs = opts.runs ?? 3
  const navTimeoutMs = opts.navTimeoutMs ?? DEFAULT_NAV_TIMEOUT
  const contentTimeoutMs = opts.contentTimeoutMs ?? DEFAULT_CONTENT_TIMEOUT

  const samples: SurfaceSample[] = []
  for (let i = 0; i < runs; i++) {
    try {
      samples.push(await runOnce(page, surface, { navTimeoutMs, contentTimeoutMs }))
    } catch (err) {
      samples.push(errorSample(err))
    }
  }

  const successful = samples.filter((s) => s.error == null)
  const firstVisitMs = median(successful.map((s) => s.firstVisitMs))
  const revisitMs = median(successful.map((s) => s.revisitMs))
  const firstVisitRpcCount = median(successful.map((s) => s.firstVisitRpcCount))
  const revisitRpcCount = median(successful.map((s) => s.revisitRpcCount))

  // Representative sample for the call trace / concurrency fields: the
  // successful sample whose revisitMs is closest to the computed median
  // (falls back to the last successful sample when no median exists).
  let representative: SurfaceSample | null = null
  if (successful.length > 0) {
    if (revisitMs != null) {
      representative = successful.reduce((best, s) =>
        Math.abs((s.revisitMs ?? Infinity) - revisitMs) < Math.abs((best.revisitMs ?? Infinity) - revisitMs) ? s : best
      )
    } else {
      representative = successful[successful.length - 1]!
    }
  }

  const remountedVotes = successful.map((s) => s.remounted).filter((v): v is boolean => v != null)
  const trueCount = remountedVotes.filter(Boolean).length
  const remounted = remountedVotes.length === 0 ? null : trueCount >= remountedVotes.length - trueCount

  return {
    id: surface.id,
    label: surface.label,
    path: surface.path,
    kind: surface.kind,
    runs,
    samples,
    firstVisitMs,
    revisitMs,
    firstVisitRpcCount,
    revisitRpcCount,
    revisitRpcCalls: representative?.revisitRpcCalls ?? [],
    maxConcurrentRpc: representative?.maxConcurrentRpc ?? null,
    rpcWallClockMs: representative?.rpcWallClockMs ?? null,
    remounted,
    error: successful.length === 0 ? (samples[samples.length - 1]?.error ?? 'all runs failed') : null,
  }
}
