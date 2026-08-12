// profile-revisit.spec.ts — 02-11 gap-closure diagnostic (D-10: measure before
// fix). Standalone, additive script — does NOT edit surfaces.ts / measure.ts
// / surface-perf.spec.ts (the frozen 02-01 harness stays frozen).
//
// 02-VERIFICATION.md's gap 2 confirmed six surfaces regressed on revisit with
// flat-or-improved RPC counts — i.e. the cost is client-side render/reactivity,
// not network. This script captures a REAL CPU profile (CDP Profiler domain —
// the same sampling data Chrome DevTools' Performance panel visualizes as a
// flame chart) during exactly the harness's own measured window (click chain
// -> contentSelector visible) for each named surface's revisit, then
// aggregates self-time by function so the dominant cost can be named with
// profiling evidence instead of guessed from source reading alone.
//
// Usage:
//   ARCHY_BASE_URL=http://a test node ARCHY_PASSWORD=*** \
//     npx playwright test e2e/perf/profile-revisit.spec.ts --project=chromium --reporter=line
import { expect, test, type Page } from '@playwright/test'
import { SURFACES, type Surface } from './surfaces'

const PASSWORD = process.env.ARCHY_PASSWORD ?? 'password123'
const NEUTRAL_SELECTOR = '[data-controller-zone="sidebar"] a[href="/dashboard/settings"]'
const NAV_TIMEOUT = 20_000
const CONTENT_TIMEOUT = 20_000

// Every surface 02-VERIFICATION.md named as regressed, plus Fleet (the
// out-of-scope bonus 02-10 flagged as the most severe magnitude of the same
// mechanism) — all six of this gap plan's must-haves.
const TARGET_IDS = ['web5', 'server', 'discover', 'app-details', 'openwrt-gateway', 'fleet']

async function login(page: Page): Promise<void> {
  await page.goto('/login', { waitUntil: 'domcontentloaded' })
  await page.evaluate(() => {
    localStorage.setItem('neode_intro_seen', '1')
    localStorage.setItem('neode_onboarding_complete', '1')
  })
  await page.goto('/login', { waitUntil: 'networkidle' })
  const passwordInput = page.locator('input[type="password"]').first()
  await passwordInput.waitFor({ timeout: 15_000 })
  await passwordInput.fill(PASSWORD)
  await page
    .locator('button:has-text("Login"), button:has-text("Unlock"), button:has-text("Continue"), button[type="submit"]')
    .first()
    .click()
  await page.waitForURL('**/dashboard**', { timeout: 20_000 })
}

async function dismissOverlays(page: Page): Promise<void> {
  try {
    await page.keyboard.press('Escape')
  } catch {
    /* best-effort */
  }
  const closeButtons = page.locator(
    '[role="dialog"] button[aria-label*="Close" i], .fixed.inset-0 button[aria-label*="Close" i]'
  )
  const count = await closeButtons.count().catch(() => 0)
  if (count > 0) await closeButtons.first().click({ timeout: 2_000 }).catch(() => {})
  // HealthNotifications.vue's dismiss button has no aria-label at all (a bare
  // SVG X icon) — the aria-label selector above never matches it, and per
  // 02-FINDINGS.md's Outstanding section its `.fixed.right-4.z-[200]` wrapper
  // has no `pointer-events: none`, so a still-open toast can intercept a
  // click meant for page content underneath it (e.g. a disk-usage warning
  // sitting over the "OpenWrt Gateway" link). Dismiss any visible one.
  const healthToastClose = page.locator('.fixed.right-4.z-\\[200\\] button')
  const healthCount = await healthToastClose.count().catch(() => 0)
  for (let i = 0; i < healthCount; i++) {
    await healthToastClose.first().click({ timeout: 1_000 }).catch(() => {})
  }
}

async function clickWithGuard(page: Page, selector: string, timeoutMs: number): Promise<void> {
  const attempts = 4
  const perAttemptMs = Math.max(2_000, Math.floor(timeoutMs / attempts))
  let lastErr: unknown
  for (let i = 0; i < attempts; i++) {
    await dismissOverlays(page)
    try {
      await page.locator(selector).first().click({ timeout: perAttemptMs, force: i === attempts - 1 })
      return
    } catch (err) {
      lastErr = err
    }
  }
  throw lastErr instanceof Error ? lastErr : new Error(String(lastErr))
}

async function clickChain(page: Page, steps: string[], timeoutMs: number): Promise<void> {
  for (const selector of steps) await clickWithGuard(page, selector, timeoutMs)
}

async function goHome(page: Page, timeoutMs: number): Promise<void> {
  if (new URL(page.url()).pathname === '/dashboard/chat') {
    const closed = await page.locator('.chat-close-btn').first().click({ timeout: 5_000 }).then(() => true).catch(() => false)
    if (!closed) await page.goto('/dashboard', { waitUntil: 'domcontentloaded' }).catch(() => {})
  }
  await clickWithGuard(page, '[data-controller-zone="sidebar"] a[href="/dashboard"]', timeoutMs)
  await page.waitForURL((url) => url.pathname === '/dashboard', { timeout: timeoutMs })
}

interface CpuProfileNode {
  id: number
  callFrame: { functionName: string; url: string; lineNumber: number; columnNumber: number; scriptId: string }
  hitCount?: number
  children?: number[]
}
interface CpuProfile {
  nodes: CpuProfileNode[]
  startTime: number
  endTime: number
  samples?: number[]
  timeDeltas?: number[]
}

// Production build has no sourcemaps deployed (confirmed: assets/*.js.map ->
// 404 on a test node), so minified function names inside vendor-*.js/
// index-*.js can't be resolved to source. Bucket by the DEPLOYED CHUNK NAME
// instead (still meaningful: vendor = Vue/Pinia/vue-router runtime bundled
// together; index = app entry/shared code; per-route chunk name = that
// view's own lazy-loaded code) rather than by node_modules path, which only
// exists pre-build.
function categorize(url: string): string {
  if (!url) return '(native/gc/idle — no JS frame)'
  const file = url.split('/').pop() ?? url
  if (/^vendor-/.test(file)) return 'assets/vendor-*.js (Vue/Pinia/vue-router runtime bundle)'
  if (/^index-/.test(file)) return 'assets/index-*.js (app entry/shared chunk)'
  if (/^Dashboard-/.test(file)) return 'assets/Dashboard-*.js (Dashboard/DashboardRouterView chunk)'
  if (/^Fleet-/.test(file)) return 'assets/Fleet-*.js (Fleet.vue chunk)'
  if (/^Web5-/i.test(file)) return 'assets/Web5-*.js chunk'
  if (/^Server-/.test(file)) return 'assets/Server-*.js chunk'
  if (/^Discover-/.test(file)) return 'assets/Discover-*.js chunk'
  if (/^AppDetails-/.test(file)) return 'assets/AppDetails-*.js chunk'
  if (/^OpenWrtGateway-/.test(file)) return 'assets/OpenWrtGateway-*.js chunk'
  if (url.startsWith('assets/')) return `assets/${file} (other chunk)`
  if (url.includes('node_modules')) return 'other node_modules (dev-only build)'
  return '(native/gc/idle — no JS frame)'
}

function analyzeProfile(profile: CpuProfile, label: string): void {
  const byId = new Map<number, CpuProfileNode>()
  for (const n of profile.nodes) byId.set(n.id, n)

  const selfTimeById = new Map<number, number>()
  const samples = profile.samples ?? []
  const timeDeltas = profile.timeDeltas ?? []
  let total = 0
  for (let i = 0; i < samples.length; i++) {
    const dt = timeDeltas[i] ?? 0
    total += dt
    const id = samples[i]!
    selfTimeById.set(id, (selfTimeById.get(id) ?? 0) + dt)
  }
  const totalMs = total / 1000
  const windowMs = (profile.endTime - profile.startTime) / 1000

  // Aggregate by function identity (name@file:line) and by coarse category.
  const byFunction = new Map<string, number>()
  const byCategory = new Map<string, number>()
  for (const [id, us] of selfTimeById) {
    const node = byId.get(id)
    if (!node) continue
    const fnName = node.callFrame.functionName || '(anonymous)'
    const file = node.callFrame.url ? node.callFrame.url.split('/').slice(-2).join('/') : '(native)'
    const key = `${fnName} @ ${file}:${node.callFrame.lineNumber}`
    byFunction.set(key, (byFunction.get(key) ?? 0) + us)
    const cat = categorize(node.callFrame.url)
    byCategory.set(cat, (byCategory.get(cat) ?? 0) + us)
  }

  console.log(`\n===== CPU PROFILE: ${label} =====`)
  console.log(`Profiled window (Profiler start->stop): ${windowMs.toFixed(1)}ms`)
  console.log(`Total sampled self-time: ${totalMs.toFixed(1)}ms (${samples.length} samples)`)
  console.log(`--- By category (self time) ---`)
  const catSorted = Array.from(byCategory.entries()).sort((a, b) => b[1] - a[1])
  for (const [cat, us] of catSorted) {
    const ms = us / 1000
    console.log(`  ${ms.toFixed(1)}ms (${((us / total) * 100).toFixed(1)}%)  ${cat}`)
  }
  console.log(`--- Top 20 functions (self time) ---`)
  const fnSorted = Array.from(byFunction.entries()).sort((a, b) => b[1] - a[1]).slice(0, 20)
  for (const [key, us] of fnSorted) {
    const ms = us / 1000
    console.log(`  ${ms.toFixed(2)}ms (${((us / total) * 100).toFixed(1)}%)  ${key}`)
  }
}

test.describe.configure({ mode: 'serial' })

test('profile revisit CPU cost for named regressed surfaces', async ({ page }) => {
  test.setTimeout(15 * 60 * 1000)
  await login(page)

  for (const id of TARGET_IDS) {
    const surface = SURFACES.find((s) => s.id === id) as Surface
    expect(surface, `surface ${id} must exist in SURFACES`).toBeTruthy()

    await goHome(page, NAV_TIMEOUT)

    // First visit — warm the cache (matches the harness's own first-visit/
    // revisit structure) — not profiled.
    await clickChain(page, surface.navSteps, NAV_TIMEOUT)
    await page.locator(surface.contentSelector).first().waitFor({ state: 'visible', timeout: CONTENT_TIMEOUT })

    // Away — to the neutral Settings tab (or close, for an in-page trigger).
    if (surface.closeSelector) {
      await page.locator(surface.closeSelector).first().click({ timeout: NAV_TIMEOUT })
    } else {
      await clickWithGuard(page, NEUTRAL_SELECTOR, NAV_TIMEOUT)
      await page.waitForURL((url) => url.pathname === '/dashboard/settings', { timeout: NAV_TIMEOUT })
    }
    // Let the away-transition settle before starting the profiler so it
    // captures only the revisit window, not Settings' own leave-transition.
    await page.waitForTimeout(600)

    // Instrument window.setTimeout/requestAnimationFrame for the profiled
    // window only, so a large "idle" share in the CPU profile can be
    // attributed to a concrete scheduled delay (app timer) vs. a chain of
    // animation frames (CSS-transition-bound) vs. neither (network wait).
    await page.evaluate(() => {
      const w = window as unknown as {
        __timerLog: Array<{ type: string; delay?: number; at: number }>
        __origSetTimeout: typeof setTimeout
        __origRaf: typeof requestAnimationFrame
      }
      w.__timerLog = []
      w.__origSetTimeout = window.setTimeout.bind(window)
      w.__origRaf = window.requestAnimationFrame.bind(window)
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      ;(window as any).setTimeout = (fn: TimerHandler, delay?: number, ...args: unknown[]) => {
        w.__timerLog.push({ type: 'setTimeout', delay, at: performance.now() })
        return w.__origSetTimeout(fn as any, delay, ...args) // eslint-disable-line @typescript-eslint/no-explicit-any
      }
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      ;(window as any).requestAnimationFrame = (cb: FrameRequestCallback) => {
        w.__timerLog.push({ type: 'raf', at: performance.now() })
        return w.__origRaf(cb)
      }
    })

    // Wall-clock RPC start/finish (matches measure.ts's own attachRpcTracker
    // convention: Date.now() offsets, method name only, no bodies — T-02-06).
    const rpcCalls: Array<{ method: string; startedAtMs: number; durationMs: number | null }> = []
    const pendingRpc = new Map<import('@playwright/test').Request, { method: string; start: number }>()
    let tRpc0 = 0
    const isRpc = (req: import('@playwright/test').Request) => req.method() === 'POST' && req.url().includes('/rpc/v1')
    const onReq = (req: import('@playwright/test').Request) => {
      if (!isRpc(req)) return
      let method = 'unknown'
      try {
        const body = req.postData()
        if (body) method = (JSON.parse(body) as { method?: string }).method ?? 'unknown'
      } catch {
        /* keep 'unknown' */
      }
      pendingRpc.set(req, { method, start: Date.now() - tRpc0 })
    }
    const onSettled = (req: import('@playwright/test').Request) => {
      const info = pendingRpc.get(req)
      if (!info) return
      pendingRpc.delete(req)
      rpcCalls.push({ method: info.method, startedAtMs: info.start, durationMs: Date.now() - tRpc0 - info.start })
    }
    page.on('request', onReq)
    page.on('requestfinished', onSettled)
    page.on('requestfailed', onSettled)

    // Capture every CSS transition/animation start+end on the document during
    // the window — decisive evidence for/against "a long CSS transition is
    // what the human waits through" (Playwright's own 'visible' check does
    // NOT wait for transitions/opacity, only a non-empty bounding box + not
    // visibility:hidden, so a slow transition would NOT show up as CPU cost
    // or as a blocked contentSelector — it would show up here, and would
    // explain a "feels slow" gap between first-paint and contentSelector).
    await page.evaluate(() => {
      const w = window as unknown as { __animLog: Array<{ type: string; name: string; target: string; elapsedMs: number; at: number }> }
      w.__animLog = []
      const describe = (el: EventTarget | null): string => {
        const e = el as HTMLElement | null
        if (!e || !e.tagName) return '(unknown)'
        const cls = (e.className && typeof e.className === 'string') ? '.' + e.className.trim().split(/\s+/).slice(0, 2).join('.') : ''
        return `${e.tagName.toLowerCase()}${cls}`
      }
      const log = (type: string) => (ev: Event) => {
        const te = ev as TransitionEvent | AnimationEvent
        w.__animLog.push({
          type,
          name: (te as TransitionEvent).propertyName ?? (te as AnimationEvent).animationName ?? '',
          target: describe(ev.target),
          elapsedMs: Math.round((te.elapsedTime ?? 0) * 1000),
          at: performance.now(),
        })
      }
      document.addEventListener('transitionrun', log('transitionrun'), true)
      document.addEventListener('transitionend', log('transitionend'), true)
      document.addEventListener('transitioncancel', log('transitioncancel'), true)
      document.addEventListener('animationstart', log('animationstart'), true)
      document.addEventListener('animationend', log('animationend'), true)
    })

    const client = await page.context().newCDPSession(page)
    await client.send('Profiler.enable')
    await client.send('Profiler.setSamplingInterval', { interval: 100 })
    await client.send('Profiler.start')

    // Raw Chrome trace events (the SAME data DevTools' Performance panel
    // renders as a flame chart / summary tab) — categorizes rendering work
    // (Layout, RecalculateStyles, Paint, CompositeLayers, RunTask, TimerFire,
    // FireAnimationFrame, ...) that a bare JS CPU profile only sees as
    // "(program)"/"(idle)" because layout/paint/compositing run on the same
    // renderer main thread but outside any JS call frame.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const traceEvents: any[] = []
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    client.on('Tracing.dataCollected', (data: any) => { traceEvents.push(...(data.value ?? [])) })
    const tracingComplete = new Promise<void>((resolve) => client.once('Tracing.tracingComplete', () => resolve()))
    await client.send('Tracing.start', {
      categories: 'disabled-by-default-devtools.timeline,devtools.timeline,toplevel,v8,blink.user_timing',
      transferMode: 'ReportEvents',
    })

    // First-paint probe, independent of Playwright's own contentSelector
    // wait: records performance.now() the moment contentSelector FIRST gets
    // a non-empty bounding box, polled every animation frame (not gated by
    // Playwright's stricter "visible" actionability rules) — so first-paint
    // and content-visible can be reported as two distinct numbers per the
    // gap plan's explicit ask.
    const pageT0 = await page.evaluate((sel) => {
      const w = window as unknown as { __firstPaintAt: number | null }
      w.__firstPaintAt = null
      function poll() {
        if (w.__firstPaintAt == null) {
          const el = document.querySelector(sel)
          if (el) {
            const r = el.getBoundingClientRect()
            if (r.width > 0 && r.height > 0) {
              w.__firstPaintAt = performance.now()
              return
            }
          }
          requestAnimationFrame(poll)
        }
      }
      requestAnimationFrame(poll)
      return performance.now()
    }, surface.contentSelector)

    const t0 = Date.now()
    tRpc0 = t0
    await clickChain(page, surface.navSteps, NAV_TIMEOUT)
    await page.locator(surface.contentSelector).first().waitFor({ state: 'visible', timeout: CONTENT_TIMEOUT })
    const wallMs = Date.now() - t0
    const firstPaintAt = await page.evaluate(() => (window as unknown as { __firstPaintAt: number | null }).__firstPaintAt)
    const firstPaintMs = firstPaintAt != null ? Math.round(firstPaintAt - pageT0) : null

    const { profile } = await client.send('Profiler.stop')
    await client.send('Profiler.disable')
    await client.send('Tracing.end')
    await tracingComplete
    await client.detach().catch(() => {})
    page.off('request', onReq)
    page.off('requestfinished', onSettled)
    page.off('requestfailed', onSettled)

    // Find the renderer main-thread tid (thread_name metadata event) so the
    // breakdown below isn't polluted by compositor/IO/GPU-process threads.
    const mainThreadMeta = traceEvents.find(
      (e) => e.ph === 'M' && e.name === 'thread_name' && (e.args?.name === 'CrRendererMain')
    )
    const mainTid = mainThreadMeta?.tid
    const durByName = new Map<string, number>()
    for (const e of traceEvents) {
      if (mainTid != null && e.tid !== mainTid) continue
      if (e.ph !== 'X') continue // complete events only (have a real duration)
      if (typeof e.dur !== 'number') continue
      durByName.set(e.name, (durByName.get(e.name) ?? 0) + e.dur)
    }
    console.log(`--- Chrome trace event self/total time by name (main thread, category=devtools.timeline; NOTE: 'X' events can nest, so this is TOTAL not self time — use for relative magnitude, not a sum-to-100% budget) ---`)
    const traceSorted = Array.from(durByName.entries()).sort((a, b) => b[1] - a[1]).slice(0, 20)
    for (const [name, us] of traceSorted) {
      console.log(`  ${(us / 1000).toFixed(1)}ms  ${name}`)
    }

    const timerLog = await page.evaluate(() => {
      const w = window as unknown as {
        __timerLog: Array<{ type: string; delay?: number; at: number }>
        __origSetTimeout: typeof setTimeout
        __origRaf: typeof requestAnimationFrame
      }
      const log = w.__timerLog ?? []
      window.setTimeout = w.__origSetTimeout
      window.requestAnimationFrame = w.__origRaf
      return log
    })

    const animLog = await page.evaluate(() => (window as unknown as { __animLog: Array<{ type: string; name: string; target: string; elapsedMs: number; at: number }> }).__animLog ?? [])

    console.log(`\n>>> ${surface.label} (${surface.id}) revisit wall-clock: ${wallMs}ms  |  first-paint: ${firstPaintMs}ms (contentSelector's first non-empty bounding box, unrelated to Playwright's own stricter 'visible' check)`)
    analyzeProfile(profile as CpuProfile, `${surface.label} (${surface.id})`)

    const setTimeoutEntries = timerLog.filter((e) => e.type === 'setTimeout')
    const rafCount = timerLog.filter((e) => e.type === 'raf').length
    console.log(`--- Timers scheduled during the revisit window (app-code setTimeout/rAF calls) ---`)
    console.log(`  setTimeout calls: ${setTimeoutEntries.length}${setTimeoutEntries.length ? ' — delays(ms): ' + setTimeoutEntries.map((e) => e.delay ?? 0).sort((a, b) => a - b).join(',') : ''}`)
    console.log(`  requestAnimationFrame calls: ${rafCount}`)

    console.log(`--- CSS transition/animation events during the window (relative to window start, pageT0) ---`)
    if (animLog.length === 0) {
      console.log('  (none observed)')
    } else {
      for (const e of animLog) {
        console.log(`  +${Math.round(e.at - pageT0)}ms  ${e.type} name="${e.name}" target=${e.target} elapsed=${e.elapsedMs}ms`)
      }
    }

    console.log(`--- RPC calls during the revisit window (wall-clock ms from window start) ---`)
    for (const c of rpcCalls) {
      console.log(`  ${c.method}: start=+${c.startedAtMs}ms duration=${c.durationMs}ms`)
    }
    console.log(`  total RPC calls: ${rpcCalls.length}, wall-clock window: ${wallMs}ms`)
  }
})
