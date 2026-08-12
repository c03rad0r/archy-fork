/**
 * Demo-content gate (operator decision 2026-08-07): seed/mock showcase
 * conversations exist for the demo.archipelago-foundation.org website ONLY
 * and must not ship in the node build. Demo builds set
 * `VITE_DEMO_CONTENT=true`; local dev keeps them for development.
 *
 * Anything behind this flag is fabricated example content — never present
 * it as real node data in a shipped build.
 */
export const DEMO_CONTENT_ENABLED =
  import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true'
