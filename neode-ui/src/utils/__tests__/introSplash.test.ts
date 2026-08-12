import { describe, expect, it } from 'vitest'
import { shouldShowIntroSplash } from '../introSplash'

describe('shouldShowIntroSplash', () => {
  it('skips intro on an already-onboarded node even without a browser intro flag', () => {
    expect(shouldShowIntroSplash({
      seenIntro: false,
      routePath: '/',
      fromBoot: false,
      onboardingComplete: true,
    })).toBe(false)
  })

  it('shows intro for a fresh root visit when onboarding is not complete', () => {
    expect(shouldShowIntroSplash({
      seenIntro: false,
      routePath: '/',
      fromBoot: false,
      onboardingComplete: false,
    })).toBe(true)
  })

  it('does not interrupt direct routes', () => {
    expect(shouldShowIntroSplash({
      seenIntro: false,
      routePath: '/dashboard/web5',
      fromBoot: false,
      onboardingComplete: null,
    })).toBe(false)
  })

  it('an explicit replay request overrides every suppression rule', () => {
    expect(shouldShowIntroSplash({
      seenIntro: true,
      routePath: '/',
      fromBoot: false,
      onboardingComplete: true,
      replayRequested: true,
    })).toBe(true)
  })

  it('a confirmed-fresh node plays the intro despite a stale per-origin seenIntro flag (reinstall / DHCP-recycled IP)', () => {
    expect(shouldShowIntroSplash({
      seenIntro: true,
      routePath: '/',
      fromBoot: false,
      onboardingComplete: false,
    })).toBe(true)
  })

  it('a confirmed-fresh node plays the intro on the boot-screen handoff too', () => {
    expect(shouldShowIntroSplash({
      seenIntro: true,
      routePath: '/login',
      fromBoot: true,
      onboardingComplete: false,
    })).toBe(true)
  })

  it('stale seenIntro still suppresses when the backend answer is unknown', () => {
    expect(shouldShowIntroSplash({
      seenIntro: true,
      routePath: '/',
      fromBoot: false,
      onboardingComplete: null,
    })).toBe(false)
  })

  it('fresh node on a deep route without boot handoff stays suppressed', () => {
    expect(shouldShowIntroSplash({
      seenIntro: false,
      routePath: '/onboarding/seed',
      fromBoot: false,
      onboardingComplete: false,
    })).toBe(false)
  })

  it('boot dev mode never root-boots into the intro', () => {
    expect(shouldShowIntroSplash({
      seenIntro: false,
      routePath: '/',
      fromBoot: false,
      devMode: 'boot',
      onboardingComplete: false,
    })).toBe(false)
  })
})
