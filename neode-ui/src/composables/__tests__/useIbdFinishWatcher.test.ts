import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, nextTick } from 'vue'

// Controllable doubles shared between the hoisted block and the mock
// factories. Plain holders — each test writes to them before importing the
// composable under a fresh module registry (the watcher keeps a module-level
// `firedThisSession` session guard, so every case needs its own module).
const state = vi.hoisted(() => ({
  packages: {} as Record<string, unknown>,
  goalStatus: 'in-progress',
  goalProgress: {} as Record<string, { completedSteps: string[] }>,
  toastAction: vi.fn(),
  routerPush: vi.fn(),
  // Re-bound every time the useBitcoinSync factory is (re)evaluated; holds the
  // exact refs the freshly imported composable watches.
  syncRefs: null as null | { synced: { value: boolean }; loaded: { value: boolean } },
}))

vi.mock('@/composables/useBitcoinSync', async () => {
  const { ref } = await import('vue')
  const synced = ref(false)
  const loaded = ref(false)
  state.syncRefs = { synced, loaded }
  return {
    bitcoinSynced: synced,
    bitcoinSyncLoaded: loaded,
    acquireBitcoinSync: () => () => {},
  }
})

vi.mock('@/stores/goals', () => ({
  useGoalStore: () => ({
    getGoalStatus: () => state.goalStatus,
    progress: state.goalProgress,
  }),
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    get packages() {
      return state.packages
    },
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ action: state.toastAction }),
}))

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: state.routerPush }),
}))

/**
 * Fresh module registry → fresh `firedThisSession`, then mount the composable
 * inside a real component so its watchers live in a proper effect scope.
 */
async function mountWatcher() {
  vi.resetModules()
  const { useIbdFinishWatcher } = await import('../useIbdFinishWatcher')
  const Host = defineComponent({
    setup() {
      useIbdFinishWatcher()
      return () => null
    },
  })
  return mount(Host)
}

/** Drive a real unsynced→synced transition through the mocked sync refs. */
async function completeSync() {
  const refs = state.syncRefs!
  refs.loaded.value = true
  refs.synced.value = false // the watcher must observe unsynced at least once
  await nextTick()
  refs.synced.value = true
  await nextTick()
  await nextTick()
}

describe('useIbdFinishWatcher', () => {
  beforeEach(() => {
    state.packages = {}
    state.goalStatus = 'in-progress'
    state.goalProgress = {}
    state.toastAction.mockClear()
    state.routerPush.mockClear()
  })

  it('says to install LND next when Lightning is not installed yet (#143)', async () => {
    // Bitcoin synced mid-goal, but the goal's install-LND step is still
    // pending: the on-chain wallet lives in LND, so "fund your wallet" would
    // promise a flow that cannot work yet.
    state.packages = { 'bitcoin-knots': { state: 'running' } }
    const wrapper = await mountWatcher()
    await completeSync()

    expect(state.toastAction).toHaveBeenCalledTimes(1)
    const [message, opts] = state.toastAction.mock.calls[0]
    expect(message).toBe(
      "Bitcoin is fully synced — next, install Lightning (LND) to get your node's on-chain wallet.",
    )
    expect(opts.label).toBe('Finish setup')
    opts.onClick()
    // "Finish setup" lands on the goal wizard, whose active step is the
    // pending install-LND one — the correct next action.
    expect(state.routerPush).toHaveBeenCalledWith('/dashboard/goals/open-a-shop')
    wrapper.unmount()
  })

  it('says to fund the wallet when LND is already installed', async () => {
    state.packages = { 'bitcoin-knots': { state: 'running' }, lnd: { state: 'running' } }
    const wrapper = await mountWatcher()
    await completeSync()

    expect(state.toastAction).toHaveBeenCalledTimes(1)
    const [message, opts] = state.toastAction.mock.calls[0]
    expect(message).toBe(
      'Bitcoin is fully synced — you can now fund your wallet and open your Lightning channel.',
    )
    expect(opts.label).toBe('Finish setup')
    opts.onClick()
    expect(state.routerPush).toHaveBeenCalledWith('/dashboard/goals/open-a-shop')
    wrapper.unmount()
  })

  it('stays silent when no Lightning goal is in progress', async () => {
    state.goalStatus = 'not-started'
    const wrapper = await mountWatcher()
    await completeSync()

    expect(state.toastAction).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('stays silent when the chain was already synced at page load', async () => {
    // A node that's already synced never shows unsynced this session, so the
    // toast must not fire (it only marks real IBD-completion transitions).
    const wrapper = await mountWatcher()
    const refs = state.syncRefs!
    refs.loaded.value = true
    refs.synced.value = true
    await nextTick()
    await nextTick()

    expect(state.toastAction).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
