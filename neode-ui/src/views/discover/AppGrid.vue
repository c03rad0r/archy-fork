<template>
  <div>
    <!-- Apps Grid -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5 pb-8">
      <template v-if="isLoading && filteredApps.length === 0">
        <div
          v-for="index in 6"
          :key="`loading-${index}`"
          class="glass-card p-5 flex flex-col app-card-skeleton"
          aria-hidden="true"
        >
          <div class="flex items-start gap-4 mb-3">
            <div class="app-card-skeleton-icon"></div>
            <div class="flex-1 min-w-0 pt-1">
              <div class="app-card-skeleton-line w-3/4 mb-3"></div>
              <div class="app-card-skeleton-line w-1/3"></div>
            </div>
          </div>
          <div class="app-card-skeleton-line w-full mb-2"></div>
          <div class="app-card-skeleton-line w-5/6 mb-2"></div>
          <div class="app-card-skeleton-line w-2/3 mb-5"></div>
          <div class="app-card-skeleton-button mt-auto"></div>
        </div>
      </template>
      <div
        v-for="(app, index) in filteredApps"
        :key="app.id"
        data-controller-container
        :data-controller-install="!(isInstalled(app.id) || installingApps.has(app.id)) && (app.source === 'local' || !!app.dockerImage) ? '1' : undefined"
        tabindex="0"
        role="link"
        class="glass-card p-5 transition-all hover:-translate-y-1 cursor-pointer flex flex-col"
        :class="{ 'card-stagger': showStagger }"
        :style="{ '--stagger-index': index + staggerOffset }"
        @click="$emit('view-details', app)"
        @keydown.enter="$emit('view-details', app)"
      >
        <div class="flex items-start gap-4 mb-3">
          <img
            v-if="app.icon"
            :src="app.icon"
            :alt="app.title"
            class="w-14 h-14 rounded-lg archy-app-icon"
            @error="handleImageError"
          />
          <div v-else class="w-14 h-14 rounded-lg bg-white/10 flex items-center justify-center">
            <svg class="w-7 h-7 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 mb-0.5">
              <h3 class="text-lg font-semibold text-white truncate">{{ app.title }}</h3>
              <span
                v-if="getAppTier(app.id) !== 'optional'"
                class="tier-badge"
                :class="getAppTier(app.id) === 'core' ? 'tier-badge-core' : 'tier-badge-recommended'"
              >{{ getAppTier(app.id) }}</span>
            </div>
            <p class="text-sm text-white/50">{{ app.version ? $ver(app.version) : 'latest' }}</p>
            <p v-if="app.author" class="text-xs text-white/40 mt-0.5">{{ app.author }}</p>
          </div>
        </div>

        <!-- Trust badge for Nostr apps -->
        <div v-if="app.trustTier" class="flex items-center gap-2 mb-2">
          <span
            class="text-xs px-2 py-0.5 rounded-full font-medium"
            :class="{
              'bg-green-400/20 text-green-400': app.trustTier === 'verified',
              'bg-yellow-400/20 text-yellow-400': app.trustTier === 'community',
              'bg-orange-400/20 text-orange-400': app.trustTier === 'unverified',
              'bg-red-400/20 text-red-400': app.trustTier === 'untrusted',
            }"
          >{{ app.trustTier }}</span>
          <span class="text-xs text-white/40">Score: {{ app.trustScore }}/100</span>
        </div>

        <p class="text-white/70 text-sm mb-4 line-clamp-3 flex-1">
          {{ typeof app.description === 'object' ? app.description.short : (app.description || 'No description available') }}
        </p>

        <div class="flex gap-2 mt-auto">
          <!-- Installed & starting up -->
          <span
            v-if="isInstalled(app.id) && isStartingUp(app.id)"
            class="flex-1 px-4 py-2 bg-yellow-500/15 border border-yellow-500/30 rounded-lg text-yellow-200 text-sm font-medium text-center cursor-default flex items-center justify-center gap-2"
          >
            <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            {{ getInstalledState(app.id) === 'installing' ? 'Installing...' : 'Starting...' }}
          </span>
          <!-- Installed & ready -->
          <span
            v-else-if="isInstalled(app.id)"
            class="flex-1 px-4 py-2 bg-white/20 rounded-lg text-white/60 text-sm font-medium text-center cursor-default"
          >Installed</span>
          <button
            v-if="isInstalled(app.id) && !isStartingUp(app.id)"
            @click.stop="$emit('launch', app)"
            class="px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium"
          >Launch</button>
          <!-- Scanning (skipped in demo — there are no real containers to scan) -->
          <span
            v-else-if="!IS_DEMO && !containersScanned && (app.source === 'local' || app.dockerImage)"
            class="flex-1 px-4 py-2 rounded-lg text-white/50 text-sm font-medium text-center cursor-default relative overflow-hidden"
          >
            <span class="discover-shimmer-bg"></span>
            <span class="relative flex items-center justify-center gap-2">
              <svg class="animate-spin h-3.5 w-3.5 opacity-60" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Checking...
            </span>
          </span>
          <!-- Demo: app not demoable -->
          <button
            v-else-if="IS_DEMO && !isInstalled(app.id) && !isDemoApp(app.id)"
            disabled
            class="flex-1 px-4 py-2 bg-white/10 rounded-lg text-white/40 text-sm font-medium cursor-not-allowed"
          >Not available in demo</button>
          <!-- Install button -->
          <button
            v-else-if="!isInstalled(app.id) && (app.source === 'local' || app.dockerImage)"
            data-controller-install-btn
            @click.stop="$emit('install', app)"
            :disabled="installingApps.has(app.id)"
            class="flex-1 px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <span v-if="installingApps.has(app.id)" class="flex items-center justify-center gap-2">
              <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              {{ installingApps.get(app.id)?.message || 'Installing...' }}
            </span>
            <span v-else>Install</span>
          </button>
          <!-- Not available -->
          <button
            v-else-if="!isInstalled(app.id)"
            disabled
            class="flex-1 px-4 py-2 bg-white/10 rounded-lg text-white/40 text-sm font-medium cursor-not-allowed"
          >Not Available</button>
        </div>
      </div>
    </div>

    <!-- Empty State -->
    <div v-if="filteredApps.length === 0 && !isLoading" class="text-center py-12">
      <div v-if="nostrError && isNostrCategory" class="flex flex-col items-center gap-4">
        <p class="text-white/70">No community apps found</p>
        <p class="text-white/40 text-sm">{{ nostrError }}</p>
        <button @click="$emit('retry-nostr')" class="px-4 py-2 glass-button rounded-lg text-sm">Retry</button>
      </div>
      <p v-else class="text-white/70">No apps found{{ searchQuery ? ` for "${searchQuery}"` : '' }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { MarketplaceApp } from './types'
import { handleImageError } from '@/views/apps/appsConfig'
import { IS_DEMO, isDemoApp } from '@/composables/useDemoIntro'

defineProps<{
  filteredApps: MarketplaceApp[]
  showStagger: boolean
  staggerOffset: number
  containersScanned: boolean
  installingApps: Map<string, { message: string }>
  isInstalled: (id: string) => boolean
  isStartingUp: (id: string) => boolean
  getInstalledState: (id: string) => string | null
  getAppTier: (id: string) => string
  isLoading: boolean
  loadingMessage: string
  nostrError: string
  isNostrCategory: boolean
  searchQuery: string
}>()

defineEmits<{
  'view-details': [app: MarketplaceApp]
  'launch': [app: MarketplaceApp]
  'install': [app: MarketplaceApp]
  'retry-nostr': []
}>()
</script>

<style scoped>
.line-clamp-3 {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.discover-shimmer-bg {
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, rgba(255,255,255,0.03) 0%, rgba(255,255,255,0.08) 50%, rgba(255,255,255,0.03) 100%);
  background-size: 200% 100%;
  animation: shimmer 2s ease-in-out infinite;
  border-radius: inherit;
}

@keyframes shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

.app-card-skeleton {
  min-height: 245px;
  pointer-events: none;
}

.app-card-skeleton-icon,
.app-card-skeleton-line,
.app-card-skeleton-button {
  position: relative;
  overflow: hidden;
  background: rgba(255, 255, 255, 0.08);
}

.app-card-skeleton-icon::after,
.app-card-skeleton-line::after,
.app-card-skeleton-button::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.12), transparent);
  animation: shimmer 1.8s ease-in-out infinite;
}

.app-card-skeleton-icon {
  width: 3.5rem;
  height: 3.5rem;
  border-radius: 0.5rem;
  flex: 0 0 auto;
}

.app-card-skeleton-line {
  height: 0.75rem;
  border-radius: 999px;
}

.app-card-skeleton-button {
  height: 2.25rem;
  border-radius: 0.5rem;
}
</style>
