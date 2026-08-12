import { describe, expect, it } from 'vitest'
import { GOALS } from '@/data/goals'
import { goalStepRouteOverride } from '../goalStepActions'
import type { GoalStep } from '@/types/goals'

describe('goalStepActions', () => {
  it('app-backed configure steps have no route override — they launch the app itself', () => {
    expect(goalStepRouteOverride(step({ id: 'configure-filebrowser', appId: 'filebrowser' }))).toBeNull()
    expect(goalStepRouteOverride(step({ id: 'configure-store', appId: 'btcpay-server' }))).toBeNull()
  })

  it('routes built-in identity and backup steps to their owning screens', () => {
    expect(goalStepRouteOverride(step({ id: 'setup-nostr' }))).toBe('/dashboard/web5')
    expect(goalStepRouteOverride(step({ id: 'export-identity' }))).toBe('/dashboard/web5/credentials')
    expect(goalStepRouteOverride(step({ id: 'create-passphrase' }))).toBe('/dashboard/settings')
  })

  it('routes channel steps to the Lightning channels screen', () => {
    expect(goalStepRouteOverride(step({ id: 'open-channel' }))).toBe('/dashboard/apps/lnd/channels')
    expect(goalStepRouteOverride(step({ id: 'open-channels' }))).toBe('/dashboard/apps/lnd/channels')
    expect(goalStepRouteOverride(step({ id: 'open-zeus-channel' }))).toBe('/dashboard/apps/lnd/channels')
  })

  it('keeps passive info steps without a target route', () => {
    expect(goalStepRouteOverride(step({ id: 'sync-setup' }))).toBeNull()
  })

  it('gives every shipped configure step a destination — a route override or an app to launch', () => {
    const configureSteps = GOALS.flatMap((goal) => goal.steps.filter((candidate) => candidate.action === 'configure'))

    for (const candidate of configureSteps) {
      const destination = goalStepRouteOverride(candidate) ?? candidate.appId ?? null
      expect(destination, `configure step ${candidate.id} has no destination`).not.toBeNull()
    }
  })
})

function step(overrides: Partial<GoalStep>): GoalStep {
  return {
    id: 'step',
    title: 'Step',
    description: '',
    action: 'configure',
    isAutomatic: false,
    ...overrides,
  }
}
