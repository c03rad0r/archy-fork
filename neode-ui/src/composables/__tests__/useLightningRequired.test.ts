import { describe, it, expect, beforeEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useLightningRequired } from '../useLightningRequired'

// The gate reads install state off the app store's package list. Stub the
// store rather than the RPC layer so the test pins the decision, not the
// transport.
const packages = vi.hoisted(() => ({ value: {} as Record<string, unknown> }))
vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    get packages() {
      return packages.value
    },
  }),
}))

describe('useLightningRequired', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    packages.value = {}
    // Module-scope `show` is shared by design (one global modal), so reset it
    // between cases or the first opener leaks into the next test.
    useLightningRequired().close()
  })

  it('lets the action through when a Lightning node is running', () => {
    packages.value = { lnd: { state: 'running' }, 'bitcoin-knots': { state: 'running' } }
    const lightning = useLightningRequired()

    expect(lightning.lightningStatus()).toBe('running')
    expect(lightning.requireLightningNode()).toBe(true)
    expect(lightning.show.value).toBe(false)
  })

  it('blocks when the node is present but NOT running, and says so', () => {
    // The bug this closes: `id in packages` is not "usable". A node with an
    // lnd entry in a non-running state produced a raw connection-refused
    // error ("Operation failed. Check server logs for details.").
    packages.value = { lnd: { state: 'stopped' } }
    const lightning = useLightningRequired()

    expect(lightning.lightningStatus()).toBe('stopped')
    expect(lightning.requireLightningNode()).toBe(false)
    expect(lightning.show.value).toBe(true)
    expect(lightning.status.value).toBe('stopped')
  })

  it('blocks and raises the install modal when no Lightning node is installed', () => {
    packages.value = { 'bitcoin-knots': { state: 'running' }, immich: { state: 'running' } }
    const lightning = useLightningRequired()

    expect(lightning.lightningStatus()).toBe('absent')
    expect(lightning.hasLightningNode()).toBe(false)
    // Returns false so the caller bails WITHOUT surfacing an error string —
    // that was the whole defect: a missing prerequisite rendered as a failure.
    expect(lightning.requireLightningNode()).toBe(false)
    expect(lightning.show.value).toBe(true)
    expect(lightning.status.value).toBe('absent')
  })

  it('shares one modal state across call sites', () => {
    packages.value = {}
    const a = useLightningRequired()
    const b = useLightningRequired()

    a.requireLightningNode()
    expect(b.show.value).toBe(true)
    b.close()
    expect(a.show.value).toBe(false)
  })

  it('treats an empty package list as absent', () => {
    packages.value = {}
    expect(useLightningRequired().lightningStatus()).toBe('absent')
  })
})
