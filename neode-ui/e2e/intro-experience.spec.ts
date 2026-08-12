/**
 * End-to-end verification of the FIRST-VISIT cinematic:
 *   splash (tap logo → alien typing → Welcome Noderunner + speech + synthwave
 *   → Archipelago logo) → onboarding intro → login (video background, finale
 *   armed) → dashboard full reveal (background zoom + interface assembly +
 *   oomph) → welcome typing — and that a SECOND login stays deliberately
 *   low-key.
 *
 * Run against the local demo stack:
 *   DEMO=1 node mock-backend.js          (port 5959)
 *   VITE_DEMO=1 npx vite --port 8100
 *   ARCHY_BASE_URL=http://localhost:8100 npx playwright test e2e/intro-experience.spec.ts
 *
 * Audio can't be heard headless, so every HTMLMediaElement.play() and
 * WebAudio oscillator start is recorded into window.__audioLog via an init
 * script — the assertions check the right cues fired at the right phases.
 */
import { test, expect, type Page } from '@playwright/test'

// Real Chrome (new headless): the bundled headless-shell has no working video
// pipeline (0 fps, ~80% dropped frames), which makes every media assertion
// meaningless there. Requires Google Chrome installed.
test.use({ channel: 'chrome' })

const BASE = process.env.ARCHY_BASE_URL ?? 'http://localhost:8100'

declare global {
  interface Window {
    __audioLog: string[]
    __videoStats: { stalls: number; waiting: number; dropped: number; total: number; readyStateAtPlay: number }
    __revealSeen: { zoom?: boolean; glass?: boolean }
  }
}

async function instrumentAudioAndVideo(page: Page) {
  await page.addInitScript(() => {
    window.__audioLog = []
    window.__videoStats = { stalls: 0, waiting: 0, dropped: 0, total: 0, readyStateAtPlay: -1 }

    // The dashboard reveal classes live only ~8s; a slow run can burn that
    // window between two sequential expect() polls. Record their appearance
    // the moment it happens instead, and let the test assert on the record.
    window.__revealSeen = {}
    new MutationObserver(() => {
      if (!window.__revealSeen.zoom && document.querySelector('.zoom-reveal-bg')) window.__revealSeen.zoom = true
      if (!window.__revealSeen.glass && document.querySelector('.glass-throw-active')) window.__revealSeen.glass = true
    }).observe(document, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] })

    const origPlay = HTMLMediaElement.prototype.play
    HTMLMediaElement.prototype.play = function (...args) {
      const src = (this.currentSrc || this.src || (this.querySelector?.('source') as HTMLSourceElement | null)?.src || 'unknown')
      window.__audioLog.push(`media-play:${src.split('/').pop()}`)
      if (this.tagName === 'VIDEO') {
        const v = this as HTMLVideoElement
        if (window.__videoStats.readyStateAtPlay === -1) window.__videoStats.readyStateAtPlay = v.readyState
        v.addEventListener('stalled', () => { window.__videoStats.stalls++ })
        v.addEventListener('waiting', () => { window.__videoStats.waiting++ })
      }
      return origPlay.apply(this, args)
    }

    // WebAudio: oscillator/buffer starts = synth pops, oomph layers, synthwave.
    const OrigOsc = OscillatorNode.prototype.start
    OscillatorNode.prototype.start = function (...args) {
      window.__audioLog.push('osc-start')
      return OrigOsc.apply(this, args)
    }
  })
}

async function freshVisit(page: Page) {
  await page.goto(BASE + '/')
  await page.evaluate(() => { localStorage.clear(); sessionStorage.clear() })
  await page.goto(BASE + '/')
}

/** Walk the full splash from tap-to-start through completion (real time). */
async function runSplash(page: Page, { skip }: { skip: boolean }) {
  // Phase 1: tap-to-start — "Enter to Exit" + logo (the overlay animates away
  // on click, so don't wait for post-click actionability)
  await expect(page.getByText('Enter to Exit')).toBeVisible({ timeout: 15_000 })
  await page.locator('.tap-to-start-logo').click({ noWaitAfter: true, force: true })

  // Phase 2: alien typing begins (first line types out)
  await expect(page.getByText('In the future there will be 3 types', { exact: false }))
    .toBeVisible({ timeout: 10_000 })

  if (skip) {
    await page.getByRole('button', { name: 'Skip Intro' }).click({ noWaitAfter: true })
  } else {
    // Let all four lines type out for real (~20s)
    await expect(page.getByText('And Noderunners...', { exact: false })).toBeVisible({ timeout: 45_000 })
  }

  // Phase 3+4 (Welcome Noderunner → logo) mount the background video. The
  // text is only on screen ~6s, so verify the phases via durable signals:
  // the video's live health here, and the audio log (speech/song) after.
  await page.waitForFunction(() => !!document.querySelector('video'), { timeout: 30_000 })
  // Wait for actual smooth playback (cold-cache buffering right after mount
  // is masked by the design's 0.3-opacity fade — smoothness is what matters).
  await page.waitForFunction(() => {
    const v = document.querySelector('video')
    return !!v && v.readyState >= 3 && v.currentTime > 0.3
  }, { timeout: 20_000 })
  const s1 = await page.evaluate(() => {
    const v = document.querySelector('video')!
    const q = v.getVideoPlaybackQuality?.()
    return { t: v.currentTime, dropped: q?.droppedVideoFrames ?? 0, total: q?.totalVideoFrames ?? 0 }
  })
  await page.waitForTimeout(2_000)
  const s2 = await page.evaluate(() => {
    const v = document.querySelector('video')
    if (!v) return null
    const q = v.getVideoPlaybackQuality?.()
    return { t: v.currentTime, dropped: q?.droppedVideoFrames ?? 0, total: q?.totalVideoFrames ?? 0 }
  })
  expect(s2).not.toBeNull()
  // The 8.1s video loops; a loop wrap makes (t - t1) negative — treat as full progress.
  const progressed = s2!.t >= s1.t ? s2!.t - s1.t : s2!.t + (8.1 - s1.t)
  expect(progressed).toBeGreaterThan(1.2) // ≥1.2s progress in 2s wall = playing smoothly
  // Steady-state frame drops over the sample window (startup catch-up excluded).
  const dTotal = s2!.total - s1.total
  const dDropped = s2!.dropped - s1.dropped
  if (dTotal > 30) expect(dDropped / dTotal).toBeLessThan(0.2)

  // Splash completes → demo routes to the onboarding intro
  await page.waitForURL('**/onboarding/intro', { timeout: 60_000 })
}

async function enterDemoAndLogin(page: Page) {
  // The CTA unmounts mid-click when the router transitions away — dispatch
  // once, swallow the detach retry, and trust the URL change instead.
  const cta = page.getByRole('button', { name: /Enter the demo/ })
  await cta.waitFor({ timeout: 15_000 })
  await Promise.all([
    page.waitForURL('**/login', { timeout: 15_000 }),
    cta.click({ noWaitAfter: true }).catch(() => {}),
  ])
  // Demo prefills the password; the finale flag must be armed at this point.
  expect(await page.evaluate(() => sessionStorage.getItem('archy_onboarding_finale'))).toBe('1')
  const loginBtn = page.getByRole('button', { name: /log ?in/i })
  await loginBtn.waitFor({ timeout: 10_000 })
  await Promise.all([
    page.waitForURL('**/dashboard**', { timeout: 25_000 }),
    loginBtn.click({ noWaitAfter: true }).catch(() => {}),
  ])
}

test.describe('first-visit cinematic', () => {
  test('full no-skip run: sounds, video health, zoom reveal, welcome typing', async ({ page }) => {
    test.setTimeout(240_000)
    await instrumentAudioAndVideo(page)
    await freshVisit(page)

    await runSplash(page, { skip: false })

    // Cinematic audio fired: intro typing loop, the Welcome Noderunner speech
    // and the synthwave bed (cosmic-updrift) — the exact regression reported.
    const log = await page.evaluate(() => window.__audioLog.join('|'))
    expect(log).toContain('welcome-noderunner.mp3')
    expect(log).toContain('cosmic-updrift.mp3')

    await enterDemoAndLogin(page)

    // FULL first-entry reveal: big background zoom + glass assembly classes
    // (recorded by the init-script observer the instant they appear — the
    // classes only live ~8s and sequential polling can miss the window).
    await expect
      .poll(() => page.evaluate(() => window.__revealSeen), { timeout: 15_000 })
      .toMatchObject({ zoom: true, glass: true })

    // Welcome typing kicks in ~4s into the reveal and animates the home cards.
    await expect(page.locator('.home-card-animate').first()).toBeVisible({ timeout: 15_000 })

    // The reveal runs 8s, then the zoom layer class clears.
    await expect(page.locator('.zoom-reveal-bg')).toHaveCount(0, { timeout: 20_000 })

    // The dashboard oomph is WebAudio oscillators — at least the login pop +
    // oomph layers must have started after the splash's own sounds.
    const oscCount = await page.evaluate(() => window.__audioLog.filter(e => e === 'osc-start').length)
    expect(oscCount).toBeGreaterThan(0)
  })

  test('skip-intro run still gets speech, song and the dashboard reveal', async ({ page }) => {
    test.setTimeout(180_000)
    await instrumentAudioAndVideo(page)
    await freshVisit(page)

    await runSplash(page, { skip: true })

    const log = await page.evaluate(() => window.__audioLog.join('|'))
    expect(log).toContain('welcome-noderunner.mp3')
    expect(log).toContain('cosmic-updrift.mp3')

    await enterDemoAndLogin(page)
    await expect
      .poll(() => page.evaluate(() => window.__revealSeen.zoom), { timeout: 15_000 })
      .toBe(true)
  })

  test('second login is deliberately low-key (no zoom reveal)', async ({ page }) => {
    test.setTimeout(180_000)
    await instrumentAudioAndVideo(page)
    await freshVisit(page)
    await runSplash(page, { skip: true })
    await enterDemoAndLogin(page)
    await expect
      .poll(() => page.evaluate(() => window.__revealSeen.zoom), { timeout: 15_000 })
      .toBe(true)

    // End the session the way logout does (auth token only — the intro/login
    // flags survive) and revisit login directly. An authenticated /login visit
    // would just bounce back to the dashboard via the router guard.
    await page.evaluate(() => localStorage.removeItem('neode-auth'))
    // Direct /login navigation (no splash — not a root boot) and re-login.
    await page.goto(BASE + '/login')
    await page.locator('#login-password').waitFor({ timeout: 15_000 })
    // First login happened → static rotated background, not the video.
    await expect(page.locator('.bg-login-static')).toBeVisible({ timeout: 10_000 })
    expect(await page.evaluate(() => localStorage.getItem('neode_first_login_done'))).toBe('1')

    const reloginBtn = page.getByRole('button', { name: /log ?in/i })
    await Promise.all([
      page.waitForURL('**/dashboard**', { timeout: 25_000 }),
      reloginBtn.click({ noWaitAfter: true }).catch(() => {}),
    ])
    // Low-key entrance: NO zoom reveal on a regular re-login. The observer
    // reset with the /login reload, so it would have caught even a transient
    // reveal since then.
    await page.waitForTimeout(1500)
    expect(await page.evaluate(() => window.__revealSeen.zoom ?? false)).toBe(false)
    await expect(page.locator('.zoom-reveal-bg')).toHaveCount(0)
  })

  test('demo replays the cinematic on a fresh boot at root', async ({ page }) => {
    test.setTimeout(120_000)
    await freshVisit(page)
    await runSplash(page, { skip: true })
    await enterDemoAndLogin(page)

    // Reload at root = a fresh boot → the splash must return even though
    // neode_intro_seen is now set.
    await page.goto(BASE + '/')
    await expect(page.getByText('Enter to Exit')).toBeVisible({ timeout: 15_000 })
  })

  test('video is warmed before the splash needs it', async ({ page }) => {
    await freshVisit(page)
    // The warm-up is a detached <video preload=auto> kept on window (Chromium
    // has no <link rel=preload as=video>). Give it a moment to buffer.
    await expect
      .poll(async () => page.evaluate(() => {
        const w = (window as unknown as { __introVideoWarm?: HTMLVideoElement }).__introVideoWarm
        return w ? { src: w.src, readyState: w.readyState } : null
      }), { timeout: 15_000 })
      .toMatchObject({ src: expect.stringContaining('video-intro.mp4') })
    const ready = await page.evaluate(() =>
      (window as unknown as { __introVideoWarm?: HTMLVideoElement }).__introVideoWarm?.readyState ?? 0)
    expect(ready).toBeGreaterThanOrEqual(1) // metadata in = download underway
  })
})
