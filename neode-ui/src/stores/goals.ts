import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { GoalProgress, GoalStatus } from '@/types/goals'
import { GOALS } from '@/data/goals'
import { useAppStore } from './app'

const STORAGE_KEY = 'archipelago-goal-progress'

/** App ID aliases — goal definitions use canonical IDs but the backend may register under variant names */
const APP_ALIASES: Record<string, string[]> = {
  immich: ['immich-server', 'immich-app', 'immich_server'],
  nextcloud: ['nextcloud-aio', 'nextcloud-server'],
  'bitcoin-knots': ['bitcoin', 'bitcoin-core'],
}

/** Pre-built reverse lookup: variant pkgId → Set of canonical appIds it matches */
const ALIAS_REVERSE = new Map<string, Set<string>>()
for (const [canonical, aliases] of Object.entries(APP_ALIASES)) {
  for (const alias of aliases) {
    let set = ALIAS_REVERSE.get(alias)
    if (!set) { set = new Set(); ALIAS_REVERSE.set(alias, set) }
    set.add(canonical)
  }
}

function matchesAppId(pkgId: string, appId: string): boolean {
  if (pkgId === appId) return true
  const reverseHits = ALIAS_REVERSE.get(pkgId)
  return reverseHits ? reverseHits.has(appId) : false
}

export const useGoalStore = defineStore('goals', () => {
  const progress = ref<Record<string, GoalProgress>>({})

  function load() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) progress.value = JSON.parse(raw)
    } catch {
      /* ignore corrupt data */
    }
  }

  function save() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(progress.value)) } catch { /* localStorage full or unavailable */ }
  }

  function getGoalStatus(goalId: string): GoalStatus {
    const goal = GOALS.find((g) => g.id === goalId)
    if (!goal) return 'not-started'

    // Goals with no required apps use manual progress tracking
    if (goal.requiredApps.length === 0) {
      return progress.value[goalId]?.status || 'not-started'
    }

    const appStore = useAppStore()
    const packages = appStore.packages

    // Auto-sync install step completion from actual package state
    // This ensures steps tick when apps are installed outside the wizard
    let didSync = false
    for (const step of goal.steps) {
      if (step.appId && step.action === 'install') {
        const isInstalled = Object.keys(packages).some((pkgId) => matchesAppId(pkgId, step.appId!))
        if (isInstalled) {
          if (!progress.value[goalId]) {
            progress.value[goalId] = {
              goalId,
              status: 'in-progress',
              currentStepIndex: 0,
              completedSteps: [],
              startedAt: Date.now(),
            }
            didSync = true
          }
          if (!progress.value[goalId].completedSteps.includes(step.id)) {
            progress.value[goalId].completedSteps.push(step.id)
            didSync = true
          }
        }
      }
    }
    if (didSync) save()

    const allRunning = goal.requiredApps.every((appId) =>
      Object.entries(packages).some(
        ([pkgId, pkg]) => matchesAppId(pkgId, appId) && pkg.state === 'running',
      ),
    )

    // Manual steps (fund the wallet, open a channel, configure the store…)
    // must be walked through too — running apps alone don't finish a goal.
    const done = progress.value[goalId]?.completedSteps ?? []
    const allManualDone = goal.steps
      .filter((s) => s.action !== 'install')
      .every((s) => done.includes(s.id))
    if (allRunning && allManualDone) return 'completed'

    const anyInstalled = goal.requiredApps.some((appId) =>
      Object.keys(packages).some((pkgId) => matchesAppId(pkgId, appId)),
    )
    if (allRunning || anyInstalled || progress.value[goalId]) return 'in-progress'

    return 'not-started'
  }

  const goalStatuses = computed(() => {
    const statuses: Record<string, GoalStatus> = {}
    for (const goal of GOALS) {
      statuses[goal.id] = getGoalStatus(goal.id)
    }
    return statuses
  })

  function startGoal(goalId: string) {
    progress.value[goalId] = {
      goalId,
      status: 'in-progress',
      currentStepIndex: 0,
      completedSteps: [],
      startedAt: Date.now(),
    }
    save()
  }

  function completeStep(goalId: string, stepId: string) {
    const p = progress.value[goalId]
    if (!p) return

    if (!p.completedSteps.includes(stepId)) {
      p.completedSteps.push(stepId)
    }

    const goal = GOALS.find((g) => g.id === goalId)
    if (goal && p.completedSteps.length >= goal.steps.length) {
      p.status = 'completed'
    } else {
      p.currentStepIndex = Math.min(p.currentStepIndex + 1, (goal?.steps.length ?? 1) - 1)
    }

    save()
  }

  function resetGoal(goalId: string) {
    delete progress.value[goalId]
    save()
  }

  // Load on store creation
  load()

  return {
    progress,
    goalStatuses,
    getGoalStatus,
    startGoal,
    completeStep,
    resetGoal,
  }
})
