import { ref } from 'vue'

import { isCompanionApp } from '@/utils/openExternal'

/**
 * Cross-view trigger for the Remote Companion intro/pairing modal
 * (CompanionIntroOverlay, mounted once in Dashboard.vue). Views like the
 * App Store banner call openCompanionIntro() to pop it on demand — this
 * bypasses the once-per-browser auto-show gate.
 *
 * Inside the companion app's own WebView the whole pitch is nonsense — the
 * user is already running it — so the trigger is a no-op there (#61: the
 * auto-popup was gated, this manual path and CompanionBanner weren't).
 */
export const companionIntroRequested = ref(false)

export function openCompanionIntro(): void {
  if (isCompanionApp()) return
  companionIntroRequested.value = true
}
