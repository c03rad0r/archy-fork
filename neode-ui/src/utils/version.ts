/**
 * Format a version string for display with exactly one leading "v".
 *
 * Version strings reach the UI from several sources — manifests (bare like
 * "1.7.96"), node/federation state and the update RPC (sometimes already
 * "v1.7.96"). Templates used to hard-code a `v` prefix (`v{{ version }}`),
 * which produced "vv1.7.96" whenever the source already carried a "v". This
 * normalizes both shapes to a single "v".
 */
export function displayVersion(v?: string | null): string {
  if (v === null || v === undefined) return ''
  const bare = String(v).trim().replace(/^v+/i, '')
  return bare ? `v${bare}` : ''
}

// Exposed globally as `$ver` (see main.ts) so templates can normalize version
// labels without a per-file import.
declare module 'vue' {
  interface ComponentCustomProperties {
    $ver: (v?: string | null) => string
  }
}
