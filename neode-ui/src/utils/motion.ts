/**
 * Design-system-aware GSAP setup — the single place animation code pulls
 * timing, easing, and colour tokens from, so every GSAP-driven surface moves
 * (and is coloured) like the rest of the glass UI instead of inventing its
 * own physics per component.
 *
 * Usage: `import { gsap, motionTokens, prefersReducedMotion } from '@/utils/motion'`
 * — never `import gsap from 'gsap'` directly, or the shared defaults are lost.
 */
import { gsap } from 'gsap'

/** Colour tokens mirrored from style.css / tailwind.config.js. The UI is
 *  dark-only (style.css pins `color-scheme: dark`), so these are constants,
 *  not theme-dependent lookups. */
export const motionTokens = {
  color: {
    /** Brand accent — the orange used for focus glows and highlights
     *  (tailwind orange-400, e.g. `.glass-button:focus-visible`). */
    accent: '#fb923c',
    /** Trust-level palette — matches NodeList / trust badges. */
    trusted: '#4ade80',
    observer: '#fb923c',
    untrusted: '#ef4444',
    neutral: '#9ca3af',
    /** Pending/attention — inbound peer requests awaiting a decision. */
    pending: '#facc15',
    /** Text/line opacities on the dark glass ground. */
    textPrimary: 'rgba(255, 255, 255, 0.95)',
    textSecondary: 'rgba(255, 255, 255, 0.7)',
    textFaint: 'rgba(255, 255, 255, 0.45)',
    line: 'rgba(255, 255, 255, 0.18)',
    lineFaint: 'rgba(255, 255, 255, 0.08)',
    glassDark: 'rgba(0, 0, 0, 0.35)',
    glassDarker: 'rgba(0, 0, 0, 0.6)',
  },
  /** Durations (seconds) — align with the CSS transitions already shipped
   *  (modal 0.3s, press feedback 0.1s). */
  duration: {
    fast: 0.18,
    base: 0.3,
    slow: 0.6,
    /** Scene-setting intros (map fly-in, hero moments). */
    cinematic: 1.4,
  },
  ease: {
    /** Default UI ease — matches the snappy glass feel. */
    out: 'power3.out',
    inOut: 'power2.inOut',
    /** Playful overshoot for elements "arriving" (node pop-ins). */
    arrive: 'back.out(1.6)',
    /** Springy attention pulse. */
    pulse: 'sine.inOut',
  },
} as const

// Shared defaults: any tween that doesn't say otherwise moves like the rest
// of the design system.
gsap.defaults({
  ease: motionTokens.ease.out,
  duration: motionTokens.duration.base,
})

/** Live reduced-motion check. Query at animation-build time (not module
 *  scope) so OS-level toggles apply without a reload. Callers should skip
 *  intros / idle loops and jump to end state when this is true. */
export function prefersReducedMotion(): boolean {
  return typeof window !== 'undefined'
    && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches === true
}

export { gsap }
