<template>
  <div
    v-if="healthNotifications.length > 0"
    class="fixed right-4 z-[200] flex flex-col gap-2 max-w-sm"
    style="top: calc(var(--safe-area-top, env(safe-area-inset-top, 0px)) + 16px);"
  >
    <div
      v-for="notif in healthNotifications"
      :key="notif.id"
      class="p-3 rounded-xl border backdrop-blur-lg shadow-lg"
      :class="notif.level === 'error'
        ? 'bg-red-500/15 border-red-500/30'
        : notif.level === 'warning'
          ? 'bg-yellow-500/15 border-yellow-500/30'
          : 'bg-blue-500/15 border-blue-500/30'"
    >
      <div class="flex items-start gap-2">
        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mt-0.5 shrink-0" :class="notif.level === 'error' ? 'text-red-400' : notif.level === 'warning' ? 'text-yellow-400' : 'text-blue-400'" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <div class="flex-1 min-w-0">
          <p class="text-sm font-medium text-white">{{ notif.title }}</p>
          <p class="text-xs text-white/60 mt-0.5">{{ notif.message }}</p>
        </div>
        <button
          class="text-white/40 hover:text-white/80 transition-colors shrink-0"
          @click="dismissNotification(notif.id)"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

const HEALTH_NOTIFICATION_MAX_AGE_MS = 30 * 60 * 1000
const GENERIC_NOTIFICATION_MAX_AGE_MS = 10 * 60 * 1000

const dismissedNotifications = ref<Set<string>>(new Set())

const healthNotifications = computed(() => {
  const notifs = store.data?.notifications ?? []
  const packages = store.data?.['package-data'] ?? {}
  const visible = notifs.filter((n) => {
    if (dismissedNotifications.value.has(n.id)) return false

    const appId = n.app_id || appIdFromNotificationTitle(n.title)
    if (appId) {
      if (isOlderThan(n.timestamp, HEALTH_NOTIFICATION_MAX_AGE_MS)) return false
      const pkg = packages[appId]
      if (!pkg) return false
      if (pkg.health !== 'unhealthy') return false
      if (pkg.state === 'removing' || pkg.state === 'stopped' || pkg.state === 'exited') return false
    } else if (isOlderThan(n.timestamp, GENERIC_NOTIFICATION_MAX_AGE_MS)) {
      return false
    }

    return true
  })
  // Deduplicate: keep only the latest notification per container/title
  const seen = new Map<string, typeof visible[0]>()
  for (const n of visible) {
    seen.set(n.title, n)
  }
  return [...seen.values()].slice(-3)
})

function dismissNotification(id: string) {
  // Dismiss all notifications with the same title (container name)
  const notif = (store.data?.notifications ?? []).find(n => n.id === id)
  if (notif) {
    for (const n of store.data?.notifications ?? []) {
      if (n.title === notif.title) dismissedNotifications.value.add(n.id)
    }
  }
  dismissedNotifications.value.add(id)
}

function appIdFromNotificationTitle(title: string): string | undefined {
  const suffix = ' is unhealthy'
  return title.endsWith(suffix) ? title.slice(0, -suffix.length) : undefined
}

function isOlderThan(timestamp: string, maxAgeMs: number): boolean {
  const ts = Date.parse(timestamp)
  return Number.isFinite(ts) && Date.now() - ts > maxAgeMs
}
</script>
