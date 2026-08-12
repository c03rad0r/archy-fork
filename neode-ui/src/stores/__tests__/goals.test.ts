import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

// Mock the app store
const mockPackages: Record<string, { state: string }> = {}
vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    packages: mockPackages,
  }),
}))

// Mock the goals data with a controlled set
vi.mock('@/data/goals', () => ({
  GOALS: [
    {
      id: 'accept-payments',
      title: 'Accept Payments',
      subtitle: 'Receive Bitcoin and Lightning payments',
      icon: 'payments',
      category: 'payments',
      requiredApps: ['bitcoin-knots', 'lnd'],
      steps: [
        { id: 'install-bitcoin', title: 'Install Bitcoin', description: '', appId: 'bitcoin-knots', action: 'install', isAutomatic: true },
        { id: 'install-lnd', title: 'Install LND', description: '', appId: 'lnd', action: 'install', isAutomatic: true },
        { id: 'open-channel', title: 'Open Channel', description: '', action: 'configure', isAutomatic: false },
      ],
      estimatedTime: '~30 min',
      difficulty: 'beginner',
    },
    {
      id: 'create-identity',
      title: 'Create Identity',
      subtitle: 'Sovereign digital identity',
      icon: 'identity',
      category: 'identity',
      requiredApps: [],
      steps: [
        { id: 'generate-did', title: 'Generate DID', description: '', action: 'verify', isAutomatic: true },
        { id: 'setup-nostr', title: 'Setup Nostr', description: '', action: 'configure', isAutomatic: false },
      ],
      estimatedTime: '~5 min',
      difficulty: 'beginner',
    },
    {
      id: 'store-photos',
      title: 'Store Photos',
      subtitle: 'Private photo backup',
      icon: 'photos',
      category: 'storage',
      requiredApps: ['immich'],
      steps: [
        { id: 'install-immich', title: 'Install Immich', description: '', appId: 'immich', action: 'install', isAutomatic: true },
        { id: 'configure-immich', title: 'Configure', description: '', action: 'configure', isAutomatic: false },
      ],
      estimatedTime: '~15 min',
      difficulty: 'beginner',
    },
  ],
}))

import { useGoalStore } from '../goals'

describe('useGoalStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    localStorage.clear()
    // Clear mock packages
    Object.keys(mockPackages).forEach((k) => delete mockPackages[k])
  })

  it('starts with empty progress', () => {
    const store = useGoalStore()
    expect(store.progress).toEqual({})
  })

  it('loads progress from localStorage', () => {
    const savedProgress = {
      'accept-payments': {
        goalId: 'accept-payments',
        status: 'in-progress',
        currentStepIndex: 1,
        completedSteps: ['install-bitcoin'],
        startedAt: 1000,
      },
    }
    localStorage.setItem('archipelago-goal-progress', JSON.stringify(savedProgress))

    const store = useGoalStore()
    expect(store.progress['accept-payments']).toBeDefined()
    expect(store.progress['accept-payments']!.completedSteps).toContain('install-bitcoin')
  })

  it('handles corrupt localStorage data', () => {
    localStorage.setItem('archipelago-goal-progress', 'not-valid-json{{{')

    const store = useGoalStore()
    expect(store.progress).toEqual({})
  })

  it('startGoal creates progress entry and saves', () => {
    const store = useGoalStore()

    store.startGoal('accept-payments')

    expect(store.progress['accept-payments']).toBeDefined()
    expect(store.progress['accept-payments']!.status).toBe('in-progress')
    expect(store.progress['accept-payments']!.currentStepIndex).toBe(0)
    expect(store.progress['accept-payments']!.completedSteps).toEqual([])
    expect(localStorage.getItem('archipelago-goal-progress')).not.toBeNull()
  })

  it('completeStep adds step to completedSteps', () => {
    const store = useGoalStore()
    store.startGoal('accept-payments')

    store.completeStep('accept-payments', 'install-bitcoin')

    expect(store.progress['accept-payments']!.completedSteps).toContain('install-bitcoin')
  })

  it('completeStep does not duplicate step IDs', () => {
    const store = useGoalStore()
    store.startGoal('accept-payments')

    store.completeStep('accept-payments', 'install-bitcoin')
    store.completeStep('accept-payments', 'install-bitcoin')

    expect(store.progress['accept-payments']!.completedSteps.filter((s) => s === 'install-bitcoin')).toHaveLength(1)
  })

  it('completeStep marks goal completed when all steps done', () => {
    const store = useGoalStore()
    store.startGoal('accept-payments')

    store.completeStep('accept-payments', 'install-bitcoin')
    store.completeStep('accept-payments', 'install-lnd')
    store.completeStep('accept-payments', 'open-channel')

    expect(store.progress['accept-payments']!.status).toBe('completed')
  })

  it('completeStep is a no-op when goal not started', () => {
    const store = useGoalStore()

    store.completeStep('accept-payments', 'install-bitcoin')

    expect(store.progress['accept-payments']).toBeUndefined()
  })

  it('resetGoal removes progress entry', () => {
    const store = useGoalStore()
    store.startGoal('accept-payments')
    expect(store.progress['accept-payments']).toBeDefined()

    store.resetGoal('accept-payments')

    expect(store.progress['accept-payments']).toBeUndefined()
  })

  describe('getGoalStatus', () => {
    it('returns not-started for unknown goal', () => {
      const store = useGoalStore()
      expect(store.getGoalStatus('nonexistent')).toBe('not-started')
    })

    it('returns not-started when no apps installed and no progress', () => {
      const store = useGoalStore()
      expect(store.getGoalStatus('accept-payments')).toBe('not-started')
    })

    it('returns completed when all required apps run AND manual steps are done', () => {
      mockPackages['bitcoin-knots'] = { state: 'running' }
      mockPackages['lnd'] = { state: 'running' }

      const store = useGoalStore()
      // Running apps alone no longer finish a goal — manual steps must be walked
      expect(store.getGoalStatus('accept-payments')).toBe('in-progress')

      store.startGoal('accept-payments')
      store.completeStep('accept-payments', 'open-channel')
      expect(store.getGoalStatus('accept-payments')).toBe('completed')
    })

    it('returns in-progress when some required apps are installed', () => {
      mockPackages['bitcoin-knots'] = { state: 'running' }

      const store = useGoalStore()
      expect(store.getGoalStatus('accept-payments')).toBe('in-progress')
    })

    it('uses manual progress for goals without required apps', () => {
      const store = useGoalStore()

      // create-identity has no required apps
      expect(store.getGoalStatus('create-identity')).toBe('not-started')

      store.startGoal('create-identity')
      expect(store.getGoalStatus('create-identity')).toBe('in-progress')
    })

    it('recognizes app aliases (immich-server matches immich)', () => {
      mockPackages['immich-server'] = { state: 'running' }

      const store = useGoalStore()
      store.startGoal('store-photos')
      store.completeStep('store-photos', 'configure-immich')
      expect(store.getGoalStatus('store-photos')).toBe('completed')
    })

    it('auto-syncs install steps from actual package state', () => {
      mockPackages['bitcoin-knots'] = { state: 'stopped' }

      const store = useGoalStore()
      store.getGoalStatus('accept-payments')

      // Should have auto-created progress and marked install-bitcoin as completed
      expect(store.progress['accept-payments']).toBeDefined()
      expect(store.progress['accept-payments']!.completedSteps).toContain('install-bitcoin')
    })
  })

  it('goalStatuses computes status for all goals', () => {
    mockPackages['bitcoin-knots'] = { state: 'running' }
    mockPackages['lnd'] = { state: 'running' }

    const store = useGoalStore()
    store.startGoal('accept-payments')
    store.completeStep('accept-payments', 'open-channel')
    const statuses = store.goalStatuses

    expect(statuses['accept-payments']).toBe('completed')
    expect(statuses['create-identity']).toBe('not-started')
    expect(statuses['store-photos']).toBe('not-started')
  })
})
