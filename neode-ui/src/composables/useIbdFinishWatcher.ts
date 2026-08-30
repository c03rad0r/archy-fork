import { computed, watch, watchEffect, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { GOALS } from '@/data/goals'
import { useGoalStore } from '@/stores/goals'
import { useAppStore } from '@/stores/app'
import { useToast } from '@/composables/useToast'
import {
  acquireBitcoinSync,
  bitcoinSynced,
  bitcoinSyncLoaded,
} from '@/composables/useBitcoinSync'

// Session-level guard: the "finish setup" toast fires at most once per page load.
let firedThisSession = false

/**
 * Watches for Bitcoin IBD completing while a Lightning setup goal is mid-flight
 * and pops a "Finish setup" toast linking back to that goal's wizard (which is
 * sitting on the fund-wallet / open-channel steps). Mount once in the
 * dashboard layout.
 */
export function useIbdFinishWatcher() {
  const goalStore = useGoalStore()
  const appStore = useAppStore()
  const router = useRouter()
  const toast = useToast()

  // A goal qualifies while it's in progress and its manual fund/channel steps
  // aren't done yet. If several qualify, the first wins — finishing the shared
  // fund + channel steps completes the lightning part of any of them.
  const pendingLightningGoalId = computed<string | null>(() => {
    if (firedThisSession) return null
    for (const goal of GOALS) {
      const hasFundStep = goal.steps.some((s) => s.action === 'fund')
      if (!hasFundStep) continue
      if (goalStore.getGoalStatus(goal.id) !== 'in-progress') continue
      const done = goalStore.progress[goal.id]?.completedSteps ?? []
      const manualPending = goal.steps.some(
        (s) => s.action !== 'install' && !done.includes(s.id),
      )
      if (manualPending) return goal.id
    }
    return null
  })

  // Only poll the chain while there's actually a goal waiting on it.
  let release: (() => void) | null = null
  watchEffect(() => {
    const shouldWatch = pendingLightningGoalId.value !== null && !bitcoinSynced.value
    if (shouldWatch && !release) {
      release = acquireBitcoinSync()
    } else if (!shouldWatch && release) {
      // Goal finished/reset or the chain synced — stop polling.
      release()
      release = null
    }
  })

  // Fire only on a REAL transition: we must have observed the chain unsynced
  // at least once this session, so a node that's already synced at page load
  // doesn't toast.
  let sawUnsynced = false
  watch([bitcoinSynced, bitcoinSyncLoaded], ([synced, loaded]) => {
    if (!loaded) return
    if (!synced) {
      sawUnsynced = true
      return
    }
    if (!sawUnsynced || firedThisSession) return
    const goalId = pendingLightningGoalId.value
    if (!goalId) return
    firedThisSession = true
    // The on-chain wallet lives in LND, not Bitcoin Core — the address the
    // fund flow shows comes from `lnd.newaddress`. While the goal's
    // install-LND step is still pending, "fund your wallet" would point at
    // something that doesn't exist yet, so the toast names the actual next
    // step instead (issue #143).
    const lndInstalled = Object.keys(appStore.packages).includes('lnd')
    toast.action(
      lndInstalled
        ? 'Bitcoin is fully synced — you can now fund your wallet and open your Lightning channel.'
        : "Bitcoin is fully synced — next, install Lightning (LND) to get your node's on-chain wallet.",
      {
        label: 'Finish setup',
        onClick: () => { router.push(`/dashboard/goals/${goalId}`) },
      },
    )
    if (release) {
      release()
      release = null
    }
  })

  onUnmounted(() => {
    if (release) {
      release()
      release = null
    }
  })
}
