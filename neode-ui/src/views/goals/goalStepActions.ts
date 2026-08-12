import type { GoalStep } from '@/types/goals'

// Steps that land on an internal screen rather than launching an app UI.
const STEP_ROUTE_OVERRIDES: Record<string, string> = {
  'setup-nostr': '/dashboard/web5',
  'export-identity': '/dashboard/web5/credentials',
  'create-passphrase': '/dashboard/settings',
  'create-backup': '/dashboard/settings',
  'save-backup': '/dashboard/settings',
  // Channel steps land directly on the Lightning channels screen (which
  // carries the "open a channel with Zeus" suggestion).
  'open-channel': '/dashboard/apps/lnd/channels',
  'open-channels': '/dashboard/apps/lnd/channels',
  'open-zeus-channel': '/dashboard/apps/lnd/channels',
}

/**
 * Internal route a step navigates to, or null when the step should launch its
 * app instead (via the app launcher — iframe apps overlay on top, tab-only
 * apps open a tab / the mobile in-app browser).
 */
export function goalStepRouteOverride(step: GoalStep): string | null {
  return STEP_ROUTE_OVERRIDES[step.id] ?? null
}
