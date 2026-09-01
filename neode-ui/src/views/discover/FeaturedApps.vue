<template>
  <div class="mb-10">
    <div class="flex items-center gap-3 mb-5">
      <span class="discover-terminal-tag">featured</span>
      <h2 class="text-xl font-bold text-white">Sovereignty Stack</h2>
      <div class="flex-1 h-px bg-white/10"></div>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
      <div
        v-for="(app, index) in featuredApps"
        :key="app.id"
        data-controller-container
        tabindex="0"
        role="link"
        class="glass-card p-6 transition-all hover:-translate-y-1 cursor-pointer"
        :class="{ 'card-stagger': showStagger }"
        :style="{ '--stagger-index': index }"
        @click="$emit('view-details', app)"
        @keydown.enter="$emit('view-details', app)"
      >
        <div class="flex items-start gap-5">
          <img
            v-if="app.icon"
            :src="app.icon"
            :alt="app.title"
            class="w-20 h-20 rounded-xl archy-app-icon flex-shrink-0"
            @error="handleImageError"
          />
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 mb-1">
              <h3 class="text-xl font-bold text-white truncate">{{ app.title }}</h3>
              <span
                v-if="getAppTier(app.id) !== 'optional'"
                class="tier-badge"
                :class="getAppTier(app.id) === 'core' ? 'tier-badge-core' : 'tier-badge-recommended'"
              >{{ getAppTier(app.id) }}</span>
              <span v-if="isInstalled(app.id)" class="discover-installed-badge">installed</span>
            </div>
            <p class="text-white/50 text-sm mb-3">{{ app.author }} &middot; {{ $ver(app.version) }}</p>
            <p class="text-white/80 text-sm leading-relaxed">{{ app.featuredDescription }}</p>
          </div>
        </div>
        <div class="flex items-center justify-between mt-4 pt-4 border-t border-white/8">
          <div class="flex items-center gap-2">
            <svg class="w-4 h-4 text-orange-400/70" fill="currentColor" viewBox="0 0 20 20">
              <path fill-rule="evenodd" d="M5 9V7a5 5 0 0110 0v2a2 2 0 012 2v5a2 2 0 01-2 2H5a2 2 0 01-2-2v-5a2 2 0 012-2zm8-2v2H7V7a3 3 0 016 0z" clip-rule="evenodd" />
            </svg>
            <span class="text-white/60 text-sm font-mono">{{ app.privacyTag }}</span>
          </div>
          <button
            v-if="isInstalled(app.id) && !isStartingUp(app.id)"
            @click.stop="$emit('launch', app)"
            class="glass-button glass-button-sm rounded-lg text-sm font-medium"
          >Launch</button>
          <span
            v-else-if="isInstalled(app.id) && isStartingUp(app.id)"
            class="text-yellow-200 text-sm flex items-center gap-2"
          >
            <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            Starting...
          </span>
          <button
            v-else-if="!IS_DEMO && !containersScanned && app.dockerImage"
            disabled
            class="text-white/40 text-sm flex items-center gap-2"
          >
            <svg class="animate-spin h-3.5 w-3.5 opacity-60" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            Checking...
          </button>
          <button
            v-else-if="IS_DEMO && !isInstalled(app.id) && !isDemoApp(app.id)"
            disabled
            class="glass-button glass-button-sm rounded-lg text-sm font-medium opacity-50 cursor-not-allowed"
          >Not available in demo</button>
          <button
            v-else-if="!isInstalled(app.id) && app.dockerImage"
            data-controller-install-btn
            @click.stop="$emit('install', app)"
            :disabled="installingApps.has(app.id)"
            class="glass-button glass-button-sm rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <span v-if="installingApps.has(app.id)" class="flex items-center gap-2">
              <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Installing...
            </span>
            <span v-else>Install</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { FeaturedApp, MarketplaceApp } from './types'
import { handleImageError } from '@/views/apps/appsConfig'
import { IS_DEMO, isDemoApp } from '@/composables/useDemoIntro'

defineProps<{
  featuredApps: FeaturedApp[]
  showStagger: boolean
  containersScanned: boolean
  installingApps: Map<string, { message: string }>
  isInstalled: (id: string) => boolean
  isStartingUp: (id: string) => boolean
  getAppTier: (id: string) => string
}>()

defineEmits<{
  'view-details': [app: MarketplaceApp]
  'launch': [app: MarketplaceApp]
  'install': [app: MarketplaceApp]
}>()
</script>
