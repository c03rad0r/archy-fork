<template>
  <div class="h-full flex flex-col">
    <div class="p-4 space-y-3" style="border-bottom: 1px solid rgba(255, 255, 255, 0.08)">
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-bold text-white/90">
          Node Apps
        </h3>
        <span class="text-xs font-mono text-white/30">
          {{ filteredApps.length }} apps
        </span>
      </div>

      <input
        v-model="search"
        type="text"
        placeholder="Search node apps..."
        class="w-full px-3 py-2 rounded-lg text-base outline-none transition-colors bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10"
      />

      <div class="flex flex-wrap gap-1.5">
        <button
          v-for="cat in categories"
          :key="cat.value"
          class="text-xs px-2 py-1 rounded-md transition-all duration-150"
          :class="activeCategory === cat.value
            ? 'nav-tab-active'
            : 'text-white/40 hover:text-white/70 hover:bg-white/5'"
          @click="activeCategory = activeCategory === cat.value ? null : cat.value"
        >
          {{ cat.label }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-4 pb-16">
      <div class="grid grid-cols-2 gap-2">
        <button
          v-for="app in filteredApps"
          :key="app.id"
          class="text-left p-3 rounded-xl transition-all duration-200 glass-card"
          :class="app.liveStatus === 'running' ? 'hover:bg-white/10 cursor-pointer' : 'opacity-70'"
          @click="handleAppClick(app)"
        >
          <div class="flex items-center gap-2 mb-1.5">
            <span class="text-lg leading-none">{{ app.icon }}</span>
            <span class="text-xs font-semibold text-white/90 truncate">{{ app.name }}</span>
          </div>
          <p class="text-xs text-white/50 line-clamp-2 leading-relaxed">
            {{ app.description }}
          </p>
          <div class="mt-2 flex items-center gap-1.5">
            <span
              class="w-1.5 h-1.5 rounded-full"
              :class="statusDotClass(app.liveStatus)"
            />
            <span class="text-xs" :class="statusTextClass(app.liveStatus)">
              {{ statusLabel(app.liveStatus) }}
            </span>
          </div>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { ARCHY_APPS, type ArchyAppMeta } from '@/data/archy-apps'
import { useArchy } from '@/composables/useArchy'

interface MergedApp extends ArchyAppMeta {
  liveStatus: 'running' | 'stopped' | 'not-installed'
}

const { isEmbedded, installedApps, requestAction } = useArchy()

const search = ref('')
const activeCategory = ref<string | null>(null)

const categories = [
  { label: 'Bitcoin', value: 'bitcoin' },
  { label: 'Lightning', value: 'lightning' },
  { label: 'Storage', value: 'storage' },
  { label: 'Social', value: 'social' },
  { label: 'Tools', value: 'tools' },
  { label: 'AI', value: 'ai' },
]

const mergedApps = computed<MergedApp[]>(() => {
  return ARCHY_APPS.map((app) => {
    const live = installedApps.value.find((a) => a.id === app.id)
    let liveStatus: MergedApp['liveStatus'] = 'not-installed'
    if (live) {
      liveStatus = live.state === 'running' ? 'running' : 'stopped'
    }
    return { ...app, liveStatus }
  })
})

const filteredApps = computed(() => {
  let apps = mergedApps.value
  if (activeCategory.value) {
    apps = apps.filter((a) => a.category === activeCategory.value)
  }
  if (search.value.trim()) {
    const q = search.value.toLowerCase()
    apps = apps.filter((a) =>
      a.name.toLowerCase().includes(q) || a.description.toLowerCase().includes(q),
    )
  }
  return apps
})

function statusDotClass(status: MergedApp['liveStatus']) {
  if (status === 'running') return 'bg-green-400'
  if (status === 'stopped') return 'bg-yellow-400'
  return 'bg-white/20'
}

function statusTextClass(status: MergedApp['liveStatus']) {
  if (status === 'running') return 'text-green-400/80'
  if (status === 'stopped') return 'text-yellow-400/70'
  return 'text-white/30'
}

function statusLabel(status: MergedApp['liveStatus']) {
  if (status === 'running') return 'Running'
  if (status === 'stopped') return 'Stopped'
  return isEmbedded.value ? 'Not installed' : 'Available'
}

function handleAppClick(app: MergedApp) {
  if (app.liveStatus === 'running' && isEmbedded.value) {
    requestAction('open-app', { appId: app.id })
  }
}
</script>
