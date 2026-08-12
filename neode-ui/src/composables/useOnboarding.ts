/**
 * Onboarding state - backend is authoritative.
 * "Unknown" (backend unreachable) must NEVER default to false —
 * that would falsely send an already-onboarded user back through
 * the intro after a browser clear / update / reboot.
 */
import { rpcClient } from '@/api/rpc-client'

async function callWithRetry<T>(fn: () => Promise<T>, maxRetries = 5): Promise<T | null> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await fn()
    } catch (e) {
      const msg = e instanceof Error ? e.message : ''
      const isRetryable = /502|503|504|timeout|fetch|network|abort/i.test(msg)
      if (!isRetryable || i === maxRetries - 1) return null
      // Exponential-ish backoff: 500, 1000, 2000, 4000, 8000 (capped)
      const delay = Math.min(500 * Math.pow(2, i), 8000)
      await new Promise((r) => setTimeout(r, delay))
    }
  }
  return null
}

/**
 * Returns true/false if the backend gave a definitive answer, null if
 * the backend is unreachable. Callers MUST handle null explicitly —
 * do not coerce to boolean without thinking about the consequences.
 */
export async function checkOnboardingStatus(): Promise<boolean | null> {
  const result = await callWithRetry(() => rpcClient.isOnboardingComplete(), 5)
  if (result !== null) {
    if (result) {
      try { localStorage.setItem('neode_onboarding_complete', '1') } catch {}
    } else {
      try { localStorage.removeItem('neode_onboarding_complete') } catch {}
    }
  }
  return result
}

/**
 * Boolean-only variant for places that genuinely cannot wait.
 * Backend answer wins; on backend-unreachable, trusts a prior
 * localStorage cache (set by a past successful check on THIS node).
 * Returns false only when both the backend and the cache agree —
 * or when the cache is empty on a genuinely fresh install.
 *
 * Prefer checkOnboardingStatus() where possible so the caller can
 * distinguish "confirmed fresh install" from "can't reach backend".
 */
export async function isOnboardingComplete(): Promise<boolean> {
  const result = await checkOnboardingStatus()
  if (result !== null) return result
  // Backend unreachable — trust the local cache. If the cache says
  // we're onboarded, we almost certainly are (this browser saw a
  // prior backend 'true' and re-seeded the flag). If the cache is
  // empty, we genuinely don't know; returning false here is the
  // last-resort fallback, and the calling views should additionally
  // keep polling the backend instead of treating this as gospel.
  return localStorage.getItem('neode_onboarding_complete') === '1'
}

export async function completeOnboarding(): Promise<void> {
  await callWithRetry(() => rpcClient.completeOnboarding(), 3)
  localStorage.setItem('neode_onboarding_complete', '1')
  localStorage.removeItem('neode_onboarding_step')
}

/** Save current onboarding step so refresh resumes where user left off */
export function saveOnboardingStep(step: string): void {
  localStorage.setItem('neode_onboarding_step', step)
}

/** Get the last saved onboarding step, or 'intro' if none */
export function getSavedOnboardingStep(): string {
  return localStorage.getItem('neode_onboarding_step') || 'intro'
}
