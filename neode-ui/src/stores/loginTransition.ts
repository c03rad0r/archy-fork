import { defineStore } from 'pinia'
import { ref } from 'vue'

/** Signals that we just logged in - Dashboard uses this for zoom + oomph */
export const useLoginTransitionStore = defineStore('loginTransition', () => {
  const justLoggedIn = ref(false)
  /**
   * True only when the user just finished the onboarding wizard
   * (first password setup), as distinct from a regular re-login.
   * Dashboard uses this to decide whether to play the full glitchy
   * reveal vs just a quick interface-draw.
   */
  const justCompletedOnboarding = ref(false)
  /** Show empty welcome block until typing starts (hide static text) */
  const pendingWelcomeTyping = ref(false)
  /** Trigger welcome typing on Home - set true after dashboard animation finishes */
  const startWelcomeTyping = ref(false)
  /**
   * True while the dashboard entrance cinematic (zoom/glitch reveal) is
   * playing. Overlays that would visually compete with it (e.g. the
   * companion intro popup) hold off until this flips back to false.
   */
  const introCinematicPlaying = ref(false)

  function setJustLoggedIn(value: boolean) {
    justLoggedIn.value = value
  }

  function setJustCompletedOnboarding(value: boolean) {
    justCompletedOnboarding.value = value
  }

  function setPendingWelcomeTyping(value: boolean) {
    pendingWelcomeTyping.value = value
  }

  function setStartWelcomeTyping(value: boolean) {
    startWelcomeTyping.value = value
  }

  function setIntroCinematicPlaying(value: boolean) {
    introCinematicPlaying.value = value
  }

  return {
    justLoggedIn,
    setJustLoggedIn,
    justCompletedOnboarding,
    setJustCompletedOnboarding,
    pendingWelcomeTyping,
    setPendingWelcomeTyping,
    startWelcomeTyping,
    setStartWelcomeTyping,
    introCinematicPlaying,
    setIntroCinematicPlaying,
  }
})
