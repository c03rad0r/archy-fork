import { describe, it, expect, afterEach } from 'vitest'
import { isCompanionApp } from '../openExternal'

// isCompanionApp() is the single companion-detection source used to skip the
// demo intro (App.vue + RootRedirect.vue) and by appLauncher — it must be true
// iff the native shell injected window.ArchipelagoNative with openInApp.

type TestWindow = Window & { ArchipelagoNative?: unknown }
const w = window as TestWindow

afterEach(() => {
  delete w.ArchipelagoNative
})

describe('isCompanionApp', () => {
  it('is false in a plain browser/PWA (no bridge injected)', () => {
    expect(isCompanionApp()).toBe(false)
  })

  it('is true when the native shell injected the bridge with openInApp', () => {
    w.ArchipelagoNative = { openInApp: () => {} }
    expect(isCompanionApp()).toBe(true)
  })

  it('is false when the bridge exists but lacks a callable openInApp', () => {
    w.ArchipelagoNative = { openExternal: () => {} }
    expect(isCompanionApp()).toBe(false)
  })
})
