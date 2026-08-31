import { describe, it, expect, afterEach, beforeEach } from 'vitest'
import { companionIntroRequested, openCompanionIntro } from '../useCompanionIntro'

// #61: the manual intro trigger (App Store banner etc.) must be a no-op inside
// the companion app's WebView — the "install the companion" pitch is nonsense
// where the user is already running it. The auto-popup was already gated
// (CompanionIntroOverlay.onMounted); openCompanionIntro is the second, manual
// path and the banner render (CompanionBanner) the third.

type TestWindow = Window & { ArchipelagoNative?: unknown }
const w = window as TestWindow

beforeEach(() => {
  companionIntroRequested.value = false
})

afterEach(() => {
  delete w.ArchipelagoNative
})

describe('openCompanionIntro', () => {
  it('raises the manual intro request in a plain browser/PWA', () => {
    expect(companionIntroRequested.value).toBe(false)
    openCompanionIntro()
    expect(companionIntroRequested.value).toBe(true)
  })

  it('is a no-op inside the companion app (bridge with openInApp)', () => {
    w.ArchipelagoNative = { openInApp: () => {}, openExternal: () => {} }
    openCompanionIntro()
    expect(companionIntroRequested.value).toBe(false)
  })

  it('still fires when the bridge exists but is not the companion shell', () => {
    // Partial bridge (no openInApp) is not the companion app — a future
    // embedder must still see the pitch.
    w.ArchipelagoNative = { openExternal: () => {} }
    openCompanionIntro()
    expect(companionIntroRequested.value).toBe(true)
  })
})
