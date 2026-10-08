// Goal-based workflow types for Easy mode

export interface GoalDefinition {
  id: string
  title: string
  subtitle: string
  icon: string
  category: 'commerce' | 'payments' | 'storage' | 'identity' | 'network' | 'backup' | 'community'
  route?: string
  requiredApps: string[]
  steps: GoalStep[]
  estimatedTime: string
  difficulty: 'beginner' | 'intermediate'
}

export interface GoalStep {
  id: string
  title: string
  description: string
  appId?: string
  /**
   * 'fund' renders the bitcoin-wallet funding UI: gated on IBD completion
   * (with a live sync timer), then a "Fund Wallet" receive flow.
   */
  action: 'install' | 'configure' | 'verify' | 'info' | 'fund'
  isAutomatic: boolean
  /** Custom step icon (e.g. the Zeus logo) — overrides the appId-derived icon */
  icon?: string
  /** Custom label for the step's CTA button (configure steps) */
  ctaLabel?: string
}

export type GoalStatus = 'not-started' | 'in-progress' | 'completed' | 'error'

export interface GoalProgress {
  goalId: string
  status: GoalStatus
  currentStepIndex: number
  completedSteps: string[]
  startedAt?: number
}
