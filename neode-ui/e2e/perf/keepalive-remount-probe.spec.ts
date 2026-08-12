// keepalive-remount-probe.spec.ts — 02-09 gap-closure Task 1, Step B.
//
// Standalone, re-runnable Playwright spec that logs into the deployed
// a test node build (D-11) and, for EVERY path in KEEP_ALIVE_PATHS,
// performs a visit -> away (to the neutral /dashboard/settings tab) ->
// return round trip, reporting whether the component instance survived.
//
// Deliberately does NOT edit measure.ts / surfaces.ts / surface-perf.spec.ts
// — the 02-01 harness stays frozen so 02-10 can re-run it unmodified. This
// spec reuses SURFACES' navSteps/contentSelector/rootSelector (read-only
// import) as the click recipe to reach each tab, but implements its own
// corrected stamp/read method plus three instruments the ad-hoc 02-08 probe
// did not have:
//
//   1. A monotonic per-mount signal written by the page itself, independent
//      of the DOM-element dataset stamp: Vue unconditionally attaches
//      `el.__vueParentComponent` to every mounted root element (confirmed
//      by reading node_modules/@vue/runtime-core's `mountElement`, not
//      gated behind a dev-only flag), whose `.uid` is a per-instance
//      monotonic counter. Reading this before/after the round trip gives an
//      INSTANCE-identity signal that can disagree with the dataset-mark's
//      ELEMENT-identity signal if the probe's selector picks a different
//      cached instance than the one actually under test (suspect 1).
//   2. `page.on('pageerror')` / `page.on('console')` captured for the whole
//      round trip and printed — a Server-specific runtime error during
//      activation/deactivation (suspect 2) would surface here even though
//      it might not throw synchronously enough to fail the Playwright step
//      itself.
//   3. After each hop: `document.querySelectorAll('.view-container').length`
//      and `location.pathname`, so cache population and the actual route
//      path are visible in the transcript (suspects 3 and 4).
//
// Corrected remount method (documented in 02-FINDINGS.md's ## Results
// preamble): stamp/read the `.view-container`-class ANCESTOR (or the
// surface's own `rootSelector`, for the two surfaces — Mesh, Chat — that
// don't use `.view-container`) of the surface's own VISIBLE contentSelector
// match, using `getBoundingClientRect`/`offsetParent` to exclude KeepAlive's
// inactive cached instances — never `document.querySelector`'s first DOM
// match, which can silently pick a different cached instance once multiple
// `.view-container`s coexist in the document at once.
import { expect, test, type Page } from '@playwright/test'
import { SURFACES, type Surface } from './surfaces'
// Re-declared rather than imported from '@/views/dashboard/keepAliveRoutes':
// the e2e package's tsconfig (see tsconfig.app.json's `include`) does not
// cover `e2e/**`, so the `@/*` path alias used across neode-ui/src is not
// guaranteed to resolve under Playwright's own TS transform. surfaces.ts and
// measure.ts already avoid the alias for the same reason (no `@/` import in
// either file) — this list is the exact literal from keepAliveRoutes.ts
// (TAB_ORDER minus withheld `/dashboard/settings`, plus `/dashboard/discover`).
const KEEP_ALIVE_PATHS = new Set<string>([
  '/dashboard',
  '/dashboard/apps',
  '/dashboard/marketplace',
  '/dashboard/cloud',
  '/dashboard/mesh',
  '/dashboard/server',
  '/dashboard/web5',
  '/dashboard/fleet',
  '/dashboard/chat',
  '/dashboard/discover',
])

const PASSWORD = process.env.ARCHY_PASSWORD ?? 'password123'
const NEUTRAL_SELECTOR = '[data-controller-zone="sidebar"] a[href="/dashboard/settings"]'
const NAV_TIMEOUT = 20_000
const CONTENT_TIMEOUT = 20_000

async function login(page: Page): Promise<void> {
  // Mirrors surface-perf.spec.ts's login() (itself mirroring app-launch.spec.ts)
  // verbatim — do not invent a second auth path.
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

interface HopSnapshot {
  viewContainerCount: number
  pathname: string
}

async function snapshotHop(page: Page): Promise<HopSnapshot> {
  return page.evaluate(() => ({
    viewContainerCount: document.querySelectorAll('.view-container').length,
    pathname: location.pathname,
  }))
}

/**
 * Wait until the DOM has stopped churning: the raw element count for
 * `contentSelector` (NOT filtered by visibility — some contentSelectors,
 * e.g. `.home-card`, legitimately match several sibling cards at once, so
 * "exactly 1" is the wrong invariant) reads identically across 5 consecutive
 * 100ms-spaced polls.
 *
 * Discovered mid-investigation (not hypothesized up front): `/dashboard/settings`
 * — the away tab EVERY round trip in this probe (and in measure.ts's own
 * `NEUTRAL_SELECTOR` convention) uses — renders `AccountInfoSection.vue` and
 * `KioskDisplaySection.vue` unconditionally (Settings.vue has no tabs/
 * accordion gating), and BOTH carry `data-controller-container`. Settings'
 * own root ALSO gets the `view-container` fallthrough class (every non-full-
 * bleed route does). That means Settings' OWN content matches the exact
 * generic `.view-container [data-controller-container]` selector Server,
 * Web5 and Fleet all use — and Settings' leave-transition keeps its DOM
 * genuinely present (in the document, still counted by querySelectorAll)
 * for the transition's full duration while the RETURN target's enter-
 * transition is already progressing. A stamp/read taken during that overlap
 * window can silently pick the still-present-but-leaving Settings element
 * instead of the actual target — a false "remounted" verdict that has
 * nothing to do with KeepAlive at all. Waiting for the raw count to settle
 * lets the leave-transition finish (Vue's <Transition> removes the leaving
 * element from the DOM once its leave hook completes) before this probe
 * stamps or reads anything.
 */
async function waitForDomSettled(page: Page, contentSelector: string, timeoutMs: number): Promise<void> {
  await page.evaluate((sel) => {
    const w = window as unknown as { __probeStableCount?: number; __probeLastLen?: number }
    w.__probeStableCount = 0
    w.__probeLastLen = document.querySelectorAll(sel).length
  }, contentSelector)
  await page.waitForFunction(
    (sel) => {
      const w = window as unknown as { __probeStableCount?: number; __probeLastLen?: number }
      const len = document.querySelectorAll(sel).length
      if (w.__probeLastLen === len) {
        w.__probeStableCount = (w.__probeStableCount ?? 0) + 1
      } else {
        w.__probeStableCount = 0
      }
      w.__probeLastLen = len
      return (w.__probeStableCount ?? 0) >= 5
    },
    contentSelector,
    { timeout: timeoutMs, polling: 100 }
  )
}

interface ProbeReading {
  ok: boolean
  uid: number | null
  typeName: string | null
}

/** Locate the VISIBLE contentSelector match's rootSelector ancestor (or
 * self, if contentSelector === rootSelector) and stamp it with `mark`,
 * recording the Vue instance uid/type-name found on it at the same moment. */
async function stampVisibleRoot(page: Page, contentSelector: string, rootSelector: string, mark: string): Promise<ProbeReading> {
  return page.evaluate(
    ({ contentSelector, rootSelector, mark }) => {
      const candidates = Array.from(document.querySelectorAll(contentSelector)) as HTMLElement[]
      const visible = candidates.find((el) => {
        const rect = el.getBoundingClientRect()
        return el.offsetParent !== null && (rect.width > 0 || rect.height > 0)
      })
      if (!visible) return { ok: false, uid: null, typeName: null }
      const root = (visible.closest(rootSelector) as HTMLElement | null) ?? (visible.matches(rootSelector) ? visible : null)
      if (!root) return { ok: false, uid: null, typeName: null }
      root.dataset.perfProbeMark = mark
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const comp = (root as any).__vueParentComponent ?? null
      const uid: number | null = comp ? (comp.uid ?? null) : null
      const typeName: string | null = comp ? (comp.type?.__name ?? comp.type?.name ?? null) : null
      return { ok: true, uid, typeName }
    },
    { contentSelector, rootSelector, mark }
  )
}

interface RootMarkDebug {
  mark: string | null
  visible: boolean
  connected: boolean
}

interface ReadResult {
  ok: boolean
  markMatches: boolean
  uid: number | null
  typeName: string | null
  allRootMarks: RootMarkDebug[]
  /** The authoritative "what does the user actually see" signal:
   * `document.elementFromPoint()` at the viewport center performs real hit
   * testing (respects stacking/z-index/opacity), unlike the
   * `offsetParent`/`getBoundingClientRect` heuristic used above, which
   * cannot distinguish the true foreground root from another root that
   * merely has non-zero layout dimensions while stacked behind it. */
  elementFromPointMark: string | null
}

async function readVisibleRoot(
  page: Page,
  contentSelector: string,
  rootSelector: string,
  expectedMark: string
): Promise<ReadResult> {
  return page.evaluate(
    ({ contentSelector, rootSelector, expectedMark }) => {
      // Diagnostic: every element matching rootSelector ANYWHERE in the
      // document (not just the one reachable from the visible content
      // match), reporting its own mark + visibility. If the ORIGINAL
      // stamped root still carries its mark but is not the one this read
      // considers "visible", that is a completely different finding
      // (a still-alive-but-orphaned cached instance) than the mark being
      // gone from every root entirely (a genuine destroy+recreate).
      const allRoots = Array.from(document.querySelectorAll(rootSelector)) as HTMLElement[]
      const allRootMarks = allRoots.map((el) => {
        const rect = el.getBoundingClientRect()
        return {
          mark: el.dataset.perfProbeMark ?? null,
          visible: el.offsetParent !== null && (rect.width > 0 || rect.height > 0),
          connected: el.isConnected,
        }
      })

      // Authoritative real-hit-test signal, independent of the
      // offsetParent/rect heuristic above.
      const cx = Math.floor(window.innerWidth / 2)
      const cy = Math.floor(window.innerHeight / 2)
      const topEl = document.elementFromPoint(cx, cy) as HTMLElement | null
      const topRoot = (topEl?.closest(rootSelector) as HTMLElement | null) ?? null
      const elementFromPointMark = topRoot?.dataset.perfProbeMark ?? null

      const candidates = Array.from(document.querySelectorAll(contentSelector)) as HTMLElement[]
      const visible = candidates.find((el) => {
        const rect = el.getBoundingClientRect()
        return el.offsetParent !== null && (rect.width > 0 || rect.height > 0)
      })
      if (!visible) return { ok: false, markMatches: false, uid: null, typeName: null, allRootMarks, elementFromPointMark }
      const root = (visible.closest(rootSelector) as HTMLElement | null) ?? (visible.matches(rootSelector) ? visible : null)
      if (!root) return { ok: false, markMatches: false, uid: null, typeName: null, allRootMarks, elementFromPointMark }
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const comp = (root as any).__vueParentComponent ?? null
      const uid: number | null = comp ? (comp.uid ?? null) : null
      const typeName: string | null = comp ? (comp.type?.__name ?? comp.type?.name ?? null) : null
      return { ok: true, markMatches: root.dataset.perfProbeMark === expectedMark, uid, typeName, allRootMarks, elementFromPointMark }
    },
    { contentSelector, rootSelector, expectedMark }
  )
}

interface RoundTripResult {
  path: string
  label: string
  /** Primary verdict: did the SAME element survive the round trip (the
   * corrected 02-08 method's own signal)? null = could not be probed. */
  elementSurvived: boolean | null
  /** Independent instance-identity signal (instrument 1): did the SAME Vue
   * component instance (by internal uid) survive? null = could not be read
   * (e.g. __vueParentComponent absent, or the view was never found). */
  instanceSurvived: boolean | null
  instanceTypeNameBeforeAway: string | null
  instanceTypeNameAfterReturn: string | null
  /** Diagnostic: every rootSelector-matching element in the document at
   * read-back time, with its own mark + visibility/connected state — shows
   * whether an unmatched original root is genuinely gone vs still present
   * (just not the one the visibility filter picked). */
  allRootMarksAtRead: RootMarkDebug[]
  /** Authoritative real-hit-test signal at read-back time (see ReadResult's
   * own doc comment) — null means either the read failed or elementFromPoint
   * found no rootSelector ancestor at the viewport center. */
  elementFromPointMarkAtRead: string | null
  elementFromPointSurvived: boolean | null
  afterVisit: HopSnapshot | null
  afterAway: HopSnapshot | null
  afterReturn: HopSnapshot | null
  consoleMessages: string[]
  pageErrors: string[]
  error: string | null
}

/**
 * Best-effort dismissal of a stray full-screen overlay before it blocks the
 * next click — mirrors measure.ts's own `dismissOverlays()` (this spec
 * deliberately does not import from measure.ts, so the logic is duplicated
 * here rather than shared, per the "don't edit the frozen 02-01 harness"
 * constraint). a test node currently runs at 85% disk (02-FINDINGS.md
 * Outstanding), which keeps `HealthNotifications.vue`'s disk-usage toast
 * live for the whole session; that toast's `.fixed.inset-0…z-[3000]` wrapper
 * has no `pointer-events: none`, so it silently intercepts clicks on
 * whatever sits behind it — an environmental condition unrelated to the
 * KeepAlive remount question this probe exists to answer, and one this
 * probe must route around rather than be blocked by.
 */
async function dismissOverlays(page: Page): Promise<void> {
  try {
    await page.keyboard.press('Escape')
  } catch {
    // no-op — best-effort only
  }
  const closeButtons = page.locator(
    '[role="dialog"] button[aria-label*="Close" i], .fixed.inset-0 button[aria-label*="Close" i]'
  )
  const count = await closeButtons.count().catch(() => 0)
  if (count > 0) {
    await closeButtons.first().click({ timeout: 2_000 }).catch(() => {})
  }
}

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

async function goHome(page: Page): Promise<void> {
  if (new URL(page.url()).pathname === '/dashboard/chat') {
    await page.locator('.chat-close-btn').first().click({ timeout: 5_000 }).catch(() => {})
  }
  await clickWithGuard(page, '[data-controller-zone="sidebar"] a[href="/dashboard"]', NAV_TIMEOUT)
  await page.waitForURL((url) => url.pathname === '/dashboard', { timeout: NAV_TIMEOUT })
}

async function clickChain(page: Page, steps: string[]): Promise<void> {
  for (const selector of steps) {
    await clickWithGuard(page, selector, NAV_TIMEOUT)
  }
}

async function roundTrip(page: Page, surface: Surface): Promise<RoundTripResult> {
  const consoleMessages: string[] = []
  const pageErrors: string[] = []
  const onConsole = (msg: { type: () => string; text: () => string }) => {
    consoleMessages.push(`[${msg.type()}] ${msg.text()}`)
  }
  const onPageError = (err: Error) => {
    pageErrors.push(err.message)
  }
  page.on('console', onConsole)
  page.on('pageerror', onPageError)

  let afterVisit: HopSnapshot | null = null
  let afterAway: HopSnapshot | null = null
  let afterReturn: HopSnapshot | null = null
  let stampReading: ProbeReading = { ok: false, uid: null, typeName: null }
  let readReading: ReadResult = { ok: false, markMatches: false, uid: null, typeName: null, allRootMarks: [], elementFromPointMark: null }

  try {
    await goHome(page)

    await clickChain(page, surface.navSteps)
    // Wait for the URL first, THEN the content selector: three of these ten
    // surfaces (Server, Web5, Fleet) share the generic
    // `.view-container [data-controller-container]` contentSelector
    // (02-FINDINGS.md's own documented ambiguity), so waiting on the
    // selector alone can resolve instantly against the PREVIOUS tab's still-
    // visible content before the navigation actually lands — a probe
    // artifact this instrumentation (pathname logging, instrument 3) caught
    // directly rather than one I could have caught by inspection alone.
    if (!surface.path.includes(':')) {
      await page.waitForURL((url) => url.pathname === surface.path, { timeout: NAV_TIMEOUT })
    }
    await page.locator(surface.contentSelector).first().waitFor({ state: 'visible', timeout: CONTENT_TIMEOUT })
    await waitForDomSettled(page, surface.contentSelector, CONTENT_TIMEOUT)
    afterVisit = await snapshotHop(page)

    const mark = `probe-${Date.now()}-${Math.random().toString(36).slice(2)}`
    stampReading = await stampVisibleRoot(page, surface.contentSelector, surface.rootSelector, mark)

    if (surface.closeSelector) {
      // Deliberately NOT routed through clickWithGuard: dismissOverlays()
      // treats any "Close"-labelled dialog button as a stray overlay to
      // dismiss, which is exactly this element for a modal-trigger surface
      // — same reasoning as measure.ts's own runOnce().
      await page.locator(surface.closeSelector).first().click({ timeout: NAV_TIMEOUT })
    } else {
      await clickWithGuard(page, NEUTRAL_SELECTOR, NAV_TIMEOUT)
      await page.waitForURL((url) => url.pathname === '/dashboard/settings', { timeout: NAV_TIMEOUT })
    }
    afterAway = await snapshotHop(page)

    await clickChain(page, surface.navSteps)
    if (!surface.path.includes(':')) {
      await page.waitForURL((url) => url.pathname === surface.path, { timeout: NAV_TIMEOUT })
    }
    await page.locator(surface.contentSelector).first().waitFor({ state: 'visible', timeout: CONTENT_TIMEOUT })
    await waitForDomSettled(page, surface.contentSelector, CONTENT_TIMEOUT)
    afterReturn = await snapshotHop(page)

    readReading = await readVisibleRoot(page, surface.contentSelector, surface.rootSelector, mark)

    const elementSurvived = stampReading.ok && readReading.ok ? readReading.markMatches : null
    const instanceSurvived =
      stampReading.ok && readReading.ok && stampReading.uid != null && readReading.uid != null
        ? stampReading.uid === readReading.uid
        : null
    const elementFromPointSurvived = readReading.elementFromPointMark != null ? readReading.elementFromPointMark === mark : null

    return {
      path: surface.path,
      label: surface.label,
      elementSurvived,
      instanceSurvived,
      instanceTypeNameBeforeAway: stampReading.typeName,
      instanceTypeNameAfterReturn: readReading.typeName,
      allRootMarksAtRead: readReading.allRootMarks,
      elementFromPointMarkAtRead: readReading.elementFromPointMark,
      elementFromPointSurvived,
      afterVisit,
      afterAway,
      afterReturn,
      consoleMessages,
      pageErrors,
      error: null,
    }
  } catch (err) {
    return {
      path: surface.path,
      label: surface.label,
      elementSurvived: null,
      instanceSurvived: null,
      instanceTypeNameBeforeAway: stampReading.typeName,
      instanceTypeNameAfterReturn: readReading.typeName,
      allRootMarksAtRead: readReading.allRootMarks,
      elementFromPointMarkAtRead: readReading.elementFromPointMark,
      elementFromPointSurvived: null,
      afterVisit,
      afterAway,
      afterReturn,
      consoleMessages,
      pageErrors,
      error: err instanceof Error ? err.message : String(err),
    }
  } finally {
    page.off('console', onConsole)
    page.off('pageerror', onPageError)
  }
}

test('keepalive-remount-probe: every KEEP_ALIVE_PATHS surface survives a tab round-trip (or reports why it could not be probed)', async ({ page }) => {
  test.setTimeout(10 * 60 * 1000)

  await login(page)

  // Session-wide capture, in addition to roundTrip()'s own per-surface
  // listeners: a delayed/async error (e.g. a promise that settles after a
  // round trip's own listeners are already detached, mid-flight when we've
  // moved on to the next surface) would otherwise be silently missed and
  // wrongly attributed to "no error" for the surface actually responsible.
  const sessionLog: string[] = []
  const sessionStart = Date.now()
  const onSessionConsole = (msg: { type: () => string; text: () => string }) => {
    sessionLog.push(`[+${Date.now() - sessionStart}ms] [console:${msg.type()}] ${msg.text()}`)
  }
  const onSessionPageError = (err: Error) => {
    sessionLog.push(`[+${Date.now() - sessionStart}ms] [pageerror] ${err.message}`)
  }
  page.on('console', onSessionConsole)
  page.on('pageerror', onSessionPageError)

  // De-duplicate by path, keeping the FIRST matching SURFACES row: two rows
  // share `path: '/dashboard'` (the `home` main-tab row and the `wallet-send`
  // modal-trigger row, which records Home's own path only for reference,
  // per surfaces.ts's own doc comment on `closeSelector`) — the modal is not
  // itself a KEEP_ALIVE_PATHS-registered route, so only `home` should count.
  const seenPaths = new Set<string>()
  const probeSurfaces = SURFACES.filter((s) => {
    if (!KEEP_ALIVE_PATHS.has(s.path) || seenPaths.has(s.path)) return false
    seenPaths.add(s.path)
    return true
  })
  expect(probeSurfaces.length).toBe(KEEP_ALIVE_PATHS.size)

  const results: RoundTripResult[] = []
  for (const surface of probeSurfaces) {
    const result = await roundTrip(page, surface)
    results.push(result)

    // eslint-disable-next-line no-console
    console.log(
      `[keepalive-remount-probe] ${result.path} (${result.label}): ` +
        `elementSurvived=${result.elementSurvived} instanceSurvived=${result.instanceSurvived} ` +
        `typeName(before/after)=${result.instanceTypeNameBeforeAway}/${result.instanceTypeNameAfterReturn} ` +
        `viewContainerCount(visit/away/return)=${result.afterVisit?.viewContainerCount ?? 'n/a'}/${result.afterAway?.viewContainerCount ?? 'n/a'}/${result.afterReturn?.viewContainerCount ?? 'n/a'} ` +
        `pathname(visit/return)=${result.afterVisit?.pathname ?? 'n/a'}/${result.afterReturn?.pathname ?? 'n/a'} ` +
        `elementFromPointSurvived=${result.elementFromPointSurvived} ` +
        `pageErrors=${result.pageErrors.length} error=${result.error ?? 'none'}`
    )
    if (result.pageErrors.length > 0) {
      // eslint-disable-next-line no-console
      console.log(`[keepalive-remount-probe]   pageErrors: ${JSON.stringify(result.pageErrors)}`)
    }
    if (result.elementSurvived === false) {
      // eslint-disable-next-line no-console
      console.log(`[keepalive-remount-probe]   allRootMarksAtRead (${result.path}): ${JSON.stringify(result.allRootMarksAtRead)}`)
    }
  }

  page.off('console', onSessionConsole)
  page.off('pageerror', onSessionPageError)

  // eslint-disable-next-line no-console
  console.log(`[keepalive-remount-probe] full session log (${sessionLog.length} entries):\n${sessionLog.join('\n')}`)

  // eslint-disable-next-line no-console
  console.log(`[keepalive-remount-probe] full results JSON:\n${JSON.stringify(results, null, 2)}`)

  // Structural assertion only — every registered path must have been
  // attempted and produce a result row, mirroring surface-perf.spec.ts's own
  // sole assertion (`expect(results.length).toBe(SURFACES.length)`). Whether
  // each one SURVIVED, or could even be probed at all, is the finding this
  // probe exists to produce, not a pass/fail gate on the spec itself: Mesh's
  // device-not-reporting-connected condition and Chat's AIUI-connection
  // timing are both pre-existing, environment-dependent blockers
  // 02-FINDINGS.md's own `## Results` section already documents as
  // "unmeasured" rather than "failed" — an errored sample is recorded, never
  // discarded and never used to fail the harness itself, exactly like
  // `measure.ts`'s `measureSurface()` treats its own per-run errors.
  expect(results.length).toBe(probeSurfaces.length)
})
