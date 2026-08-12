<template>
  <div
    class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5 mb-8 transition-opacity duration-300"
    :class="{ 'opacity-0 pointer-events-none': !show }"
  >
    <RouterLink
      v-for="(goal, idx) in goals"
      :key="goal.id"
      :to="`/dashboard/goals/${goal.id}`"
      class="goal-card glass-card p-6 block"
      :class="{ 'home-card-animate': animate }"
      :style="{ '--card-stagger': idx }"
    >
      <div class="flex items-start justify-between mb-4">
        <!-- App icons for goals with required apps, emoji fallback otherwise -->
        <div v-if="goalAppIcons(goal).length > 0" class="flex items-center gap-1.5 shrink-0">
          <img
            v-for="icon in goalAppIcons(goal)"
            :key="icon.appId"
            :src="icon.url"
            :alt="icon.appId"
            class="w-8 h-8 rounded-lg object-contain bg-white/5 border border-white/10 p-0.5"
            @error="($event.target as HTMLImageElement).style.display = 'none'"
          />
        </div>
        <div v-else class="w-10 h-10 rounded-xl bg-white/10 flex items-center justify-center shrink-0">
          <span class="text-xl">{{ goalIcon(goal.icon) }}</span>
        </div>
        <span class="goal-status-badge" :class="statusBadgeClass(goal.id)">
          <span v-if="goalStatuses[goal.id] === 'completed'" class="w-1.5 h-1.5 rounded-full bg-green-400"></span>
          <span v-else-if="goalStatuses[goal.id] === 'in-progress'" class="w-1.5 h-1.5 rounded-full bg-orange-400"></span>
          {{ statusLabel(goal.id) }}
        </span>
      </div>

      <h3 class="text-lg font-semibold text-white mb-1">{{ goal.title }}</h3>
      <p class="text-sm text-white/55 mb-4 leading-relaxed">{{ goal.subtitle }}</p>

      <div class="flex items-center justify-between">
        <span class="text-xs text-white/40">{{ goal.estimatedTime }}</span>
        <span class="text-xs text-white/50 flex items-center gap-1">
          {{ goal.difficulty === 'beginner' ? t('easyHome.beginner') : t('easyHome.intermediate') }}
        </span>
      </div>
    </RouterLink>
  </div>
</template>

<script setup lang="ts">
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { GOALS } from '@/data/goals'
import { useGoalStore } from '@/stores/goals'
import type { GoalDefinition } from '@/types/goals'

defineProps<{
  show: boolean
  animate: boolean
}>()

const { t } = useI18n()
const goalStore = useGoalStore()
const goals = GOALS
const goalStatuses = goalStore.goalStatuses

/** Map appId to its icon file path under /assets/img/app-icons/ */
const APP_ICON_MAP: Record<string, string> = {
  'bitcoin-knots': '/assets/img/app-icons/bitcoin-knots.webp',
  lnd: '/assets/img/app-icons/lnd.png',
  'btcpay-server': '/assets/img/app-icons/btcpay-server.png',
  filebrowser: '/assets/img/app-icons/file-browser.webp',
  nextcloud: '/assets/img/app-icons/nextcloud.webp',
  fedimint: '/assets/img/app-icons/fedimint.png',
  mempool: '/assets/img/app-icons/mempool.webp',
  electrs: '/assets/img/app-icons/electrumx.png',
  electrumx: '/assets/img/app-icons/electrumx.png',
}

function goalAppIcons(goal: GoalDefinition): { appId: string; url: string }[] {
  return goal.requiredApps
    .filter((appId) => APP_ICON_MAP[appId] !== undefined)
    .map((appId) => ({ appId, url: APP_ICON_MAP[appId] as string }))
}

function goalIcon(icon: string): string {
  const icons: Record<string, string> = {
    shop: '🏪',
    payments: '⚡',
    photos: '📸',
    files: '📁',
    lightning: '⚡',
    identity: '🔑',
    backup: '💾',
  }
  return icons[icon] || '📦'
}

function statusLabel(goalId: string): string {
  const status = goalStatuses[goalId]
  if (status === 'completed') return t('easyHome.done')
  if (status === 'in-progress') return t('easyHome.inProgress')
  return t('easyHome.start')
}

function statusBadgeClass(goalId: string): string {
  const status = goalStatuses[goalId]
  if (status === 'completed') return 'goal-status-badge-completed'
  if (status === 'in-progress') return 'goal-status-badge-in-progress'
  return 'goal-status-badge-not-started'
}
</script>
