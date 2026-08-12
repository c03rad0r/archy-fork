export interface IntroSplashDecisionInput {
  seenIntro: boolean
  routePath: string
  fromBoot: boolean
  devMode?: string
  onboardingComplete: boolean | null
  /** Explicit "Replay intro" click — overrides every suppression rule. */
  replayRequested?: boolean
}

export function shouldShowIntroSplash(input: IntroSplashDecisionInput): boolean {
  if (input.replayRequested) return true

  const isDirectRoute = input.routePath !== '/'
  // A node the backend CONFIRMS has never completed onboarding always gets
  // the full intro on a root boot. `seenIntro` is per-origin browser state —
  // after a reinstall (or a DHCP-recycled IP), the browser still carries the
  // previous node's flag at the same origin, which silently muted the intro
  // on genuinely fresh installs.
  if (input.onboardingComplete === false && (input.fromBoot || (!isDirectRoute && input.devMode !== 'boot'))) {
    return true
  }
  if (input.seenIntro) return false
  if (input.onboardingComplete === true) return false

  if (input.fromBoot) return true
  if (input.devMode === 'boot') return false
  return !isDirectRoute
}
