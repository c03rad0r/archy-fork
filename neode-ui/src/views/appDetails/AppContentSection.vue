<template>
  <div class="lg:col-span-2 space-y-6">
    <!-- Screenshots Gallery -->
    <div v-if="screenshots.length > 0" class="glass-card p-6">
      <h2 class="text-2xl font-bold text-white mb-4">{{ t('appDetails.screenshots') }}</h2>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <img
          v-for="screenshot in screenshots"
          :key="screenshot.src"
          :src="screenshot.src"
          :alt="screenshot.alt"
          class="aspect-video w-full rounded-xl border border-white/10 object-cover"
          loading="lazy"
        />
      </div>
    </div>

    <!-- Bitcoin Sync Warning (for dependent apps) -->
    <div v-if="needsBitcoinSync && !bitcoinSynced" class="glass-card p-6 border border-orange-500/30">
      <div class="flex items-start gap-3 mb-4">
        <svg class="w-6 h-6 text-orange-400 flex-shrink-0 mt-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <div class="flex-1">
          <p class="text-orange-300 font-semibold text-xl">Bitcoin is syncing</p>
          <p class="text-white/70 mt-2 leading-relaxed">
            Some features may be unavailable until Bitcoin finishes syncing.
            Wallet connections and block data require a fully synced node.
          </p>
        </div>
      </div>
      <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden">
        <div
          class="h-full rounded-full bg-orange-400 transition-all duration-500"
          :style="{ width: Math.min(bitcoinSyncPercent, 100) + '%' }"
        ></div>
      </div>
      <p class="text-sm text-white/55 mt-2">
        {{ bitcoinSyncPercent.toFixed(1) }}% synced — Block {{ bitcoinBlockHeight.toLocaleString() }}
      </p>
    </div>

    <!-- Description -->
    <div class="glass-card p-6">
      <h2 class="text-2xl font-bold text-white mb-4">{{ t('appDetails.about', { name: pkg.manifest.title }) }}</h2>
      <p class="text-white/80 leading-relaxed whitespace-pre-line">
        {{ pkg.manifest.description.long }}
      </p>
    </div>

    <!-- Features (if available) -->
    <div v-if="features.length > 0" class="glass-card p-6">
      <h2 class="text-2xl font-bold text-white mb-4">{{ t('appDetails.features') }}</h2>
      <ul class="space-y-3">
        <li
          v-for="(feature, index) in features"
          :key="index"
          class="flex items-start gap-3 text-white/80"
        >
          <svg class="w-6 h-6 text-green-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>{{ feature }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { AppScreenshot } from '@/types/api'

const { t } = useI18n()

const props = defineProps<{
  pkg: Record<string, any>
  features: string[]
  needsBitcoinSync: boolean
  bitcoinSynced: boolean
  bitcoinSyncPercent: number
  bitcoinBlockHeight: number
}>()

const screenshots = computed(() => {
  const manifestScreenshots = props.pkg.manifest?.screenshots
  const staticScreenshots = props.pkg['static-files']?.screenshots
  return normalizeScreenshots(Array.isArray(staticScreenshots) ? staticScreenshots : manifestScreenshots)
})

function normalizeScreenshots(items: AppScreenshot[] | undefined) {
  if (!Array.isArray(items)) return []
  return items
    .map((item, index) => {
      if (typeof item === 'string') {
        const src = item.trim()
        return src ? { src, alt: `${props.pkg.manifest?.title || 'App'} screenshot ${index + 1}` } : null
      }
      const src = item.src?.trim()
      if (!src) return null
      return {
        src,
        alt: item.alt?.trim() || `${props.pkg.manifest?.title || 'App'} screenshot ${index + 1}`,
      }
    })
    .filter((item): item is { src: string; alt: string } => item !== null)
}
</script>
