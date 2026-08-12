// Re-runnable surface-perf harness (02-01-PLAN.md Task 1). Logs in using the
// exact flow from app-launch.spec.ts, walks every SURFACES row via
// measureSurface(), and writes the full result array + a run header to
// ARCHY_PERF_OUT (defaulting to e2e/test-results/surface-perf.json).
//
// Redaction is structural, not a cleanup pass: measure.ts's RpcTracker only
// ever records method name + timing (T-02-06) — request/response bodies,
// page text and screenshots are never captured into the artifact.
import { execSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { expect, test, type Page } from '@playwright/test'
import { measureSurface, type SurfaceMeasurement } from './measure'
import { SURFACES } from './surfaces'

const PASSWORD = process.env.ARCHY_PASSWORD ?? 'password123'
const RUNS = process.env.ARCHY_PERF_RUNS ? Number(process.env.ARCHY_PERF_RUNS) : 3
const OUT_PATH = resolve(process.cwd(), process.env.ARCHY_PERF_OUT ?? 'e2e/test-results/surface-perf.json')

async function login(page: Page): Promise<void> {
  // Mirrors e2e/app-launch.spec.ts's login() verbatim — do not invent a
  // second auth path.
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

function currentCommit(): string {
  try {
    // `__dirname` is unavailable under this package's `"type": "module"` ESM
    // runtime (Playwright's own transform swallows the ReferenceError into
    // the catch below, silently yielding 'unknown') — use `process.cwd()`
    // instead, which Playwright always sets to the project root it was
    // invoked from.
    return execSync('git rev-parse --short HEAD', { cwd: process.cwd() }).toString().trim()
  } catch {
    return 'unknown'
  }
}

test('surface-perf: measure every D-09 surface and write the baseline artifact', async ({ page, baseURL }) => {
  test.setTimeout(20 * 60 * 1000) // 15 surfaces x 3 runs x network round-trips can run long on a real node

  await login(page)

  const results: SurfaceMeasurement[] = []
  const skipped: string[] = []

  for (const surface of SURFACES) {
    try {
      const measurement = await measureSurface(page, surface, { runs: RUNS })
      results.push(measurement)
      if (measurement.error) skipped.push(`${surface.id}: ${measurement.error}`)
    } catch (err) {
      // A surface that throws outside measureSurface's own per-run try/catch
      // (e.g. login state got corrupted) is still recorded, never dropped —
      // an unmeasured surface must never silently disappear from the array.
      results.push({
        id: surface.id,
        label: surface.label,
        path: surface.path,
        kind: surface.kind,
        runs: RUNS,
        samples: [],
        firstVisitMs: null,
        revisitMs: null,
        firstVisitRpcCount: null,
        revisitRpcCount: null,
        revisitRpcCalls: [],
        maxConcurrentRpc: null,
        rpcWallClockMs: null,
        remounted: null,
        error: err instanceof Error ? err.message : String(err),
      })
      skipped.push(`${surface.id}: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const header = {
    baseUrl: baseURL ?? process.env.ARCHY_BASE_URL ?? 'http://localhost:8100',
    takenAt: new Date().toISOString(),
    commit: currentCommit(),
    runs: RUNS,
    notes: skipped.length > 0 ? `Skipped/errored surfaces: ${skipped.join('; ')}` : 'All surfaces measured cleanly.',
  }

  const artifact = { ...header, results }

  mkdirSync(dirname(OUT_PATH), { recursive: true })
  writeFileSync(OUT_PATH, JSON.stringify(artifact, null, 2))

  expect(results.length).toBe(SURFACES.length)
})
