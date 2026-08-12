import { ref } from 'vue'

/**
 * Cross-view trigger for the Remote Companion intro/pairing modal
 * (CompanionIntroOverlay, mounted once in Dashboard.vue). Views like the
 * App Store banner call openCompanionIntro() to pop it on demand — this
 * bypasses the once-per-browser auto-show gate.
 */
export const companionIntroRequested = ref(false)

export function openCompanionIntro(): void {
  companionIntroRequested.value = true
}
