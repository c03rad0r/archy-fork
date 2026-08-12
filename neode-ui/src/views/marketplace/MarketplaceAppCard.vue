<template>
  <div
    data-controller-container
    :data-controller-install="!(installed || installing || installBlockedReason) && (app.source === 'local' || !!app.dockerImage) ? '1' : undefined"
    tabindex="0"
    role="link"
    class="glass-card p-6 hover:bg-orange-500/5 hover:border-orange-500/15 transition-all cursor-pointer flex flex-col"
    :class="{ 'card-stagger': stagger }"
    :style="{ '--stagger-index': index }"
    @click="$emit('view', app)"
    @keydown.enter="$emit('view', app)"
  >
    <div class="flex items-start gap-4 mb-4">
      <img
        :src="app.icon || DEFAULT_APP_ICON"
        :alt="app.title"
        class="w-16 h-16 rounded-lg archy-app-icon"
        @error="handleImageError"
      />
      <div class="flex-1">
        <h3 class="text-xl font-semibold text-white mb-1">
          {{ app.title }}
          <span
            v-if="!installed && tierLabel !== 'optional'"
            class="tier-badge"
            :class="tierLabel === 'core' ? 'tier-badge-core' : 'tier-badge-recommended'"
          >{{ tierLabel }}</span>
        </h3>
        <p class="text-sm text-white/60">{{ app.version ? $ver(app.version) : 'latest' }}</p>
        <p v-if="app.author" class="text-xs text-white/50 mt-1">by {{ app.author }}</p>
      </div>
    </div>

    <!-- Trust badge for Nostr community apps -->
    <div v-if="app.trustTier" class="flex items-center flex-wrap gap-2 mb-3">
      <span
        class="text-xs px-2 py-0.5 rounded-full font-medium"
        :class="{
          'bg-green-400/20 text-green-400': app.trustTier === 'verified',
          'bg-yellow-400/20 text-yellow-400': app.trustTier === 'community',
          'bg-orange-400/20 text-orange-400': app.trustTier === 'unverified',
          'bg-red-400/20 text-red-400': app.trustTier === 'untrusted',
        }"
      >{{ app.trustTier }}</span>

      <!--
        Authorship, which is a different question from the trust score: the
        score blends relay count, provenance and policy compliance, while this
        says only whether the author proved control of the key their DID names.
      -->
      <span
        v-if="app.signature"
        class="text-xs px-2 py-0.5 rounded-full font-medium inline-flex items-center gap-1"
        :class="{
          'bg-green-400/20 text-green-400': app.signature.status === 'valid',
          'bg-white/10 text-white/50': app.signature.status === 'missing',
          'bg-red-400/20 text-red-400': app.signature.status === 'invalid',
        }"
        :title="signatureTooltip"
      >
        <svg v-if="app.signature.status === 'valid'" class="w-3 h-3" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
          <path fill-rule="evenodd" d="M5 9V7a5 5 0 0110 0v2a2 2 0 012 2v5a2 2 0 01-2 2H5a2 2 0 01-2-2v-5a2 2 0 012-2zm8-2v2H7V7a3 3 0 016 0z" clip-rule="evenodd" />
        </svg>
        <svg v-else-if="app.signature.status === 'invalid'" class="w-3 h-3" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
          <path fill-rule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clip-rule="evenodd" />
        </svg>
        {{ signatureLabel }}
      </span>

      <span class="text-xs text-white/40">Score: {{ app.trustScore }}/100</span>
      <span v-if="app.relayCount" class="text-xs text-white/40">&middot; {{ app.relayCount }} relay{{ app.relayCount !== 1 ? 's' : '' }}</span>
    </div>

    <p class="text-white/80 text-sm mb-4 line-clamp-3 flex-1">
      {{ typeof app.description === 'object' ? app.description.short : (app.description || 'No description available') }}
    </p>

    <p v-if="!installed && installBlockedReason" class="mb-4 rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-100">
      Requires a full archive Bitcoin node before install.
    </p>

    <div class="flex gap-2 mt-auto">
      <!-- Installed & starting up (transitional state) -->
      <span
        v-if="installed && startingUp"
        class="flex-1 px-4 py-2 bg-yellow-500/15 border border-yellow-500/30 rounded-lg text-yellow-200 text-sm font-medium text-center cursor-default flex items-center justify-center gap-2"
      >
        <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        {{ installedState === 'installing' ? 'Installing...' : 'Starting...' }}
      </span>
      <!-- Installed & ready -->
      <span
        v-else-if="installed"
        class="flex-1 px-4 py-2 bg-white/20 rounded-lg text-white/60 text-sm font-medium text-center cursor-default"
      >
        {{ t('marketplace.alreadyInstalled') }}
      </span>
      <button
        v-if="installed && !startingUp"
        @click.stop="$emit('launch', app)"
        class="px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium"
      >
        {{ t('common.launch') }}
      </button>
      <!-- Not yet scanned — show loading indicator instead of install -->
      <span
        v-else-if="!containersScanned && (app.source === 'local' || app.dockerImage)"
        class="flex-1 px-4 py-2 rounded-lg text-white/50 text-sm font-medium text-center cursor-default relative overflow-hidden"
      >
        <span class="marketplace-shimmer-bg"></span>
        <span class="relative flex items-center justify-center gap-2">
          <svg class="animate-spin h-3.5 w-3.5 opacity-60" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          Checking...
        </span>
      </span>
      <!-- Installing — live progress with bar + message matching the
           update download bar's accuracy. Falls back to a simple
           spinner if no install_progress data is available yet. -->
      <div
        v-else-if="!installed && installing"
        class="flex-1 flex flex-col gap-1.5"
      >
        <div
          class="px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium opacity-90 cursor-wait flex items-center justify-center gap-2 text-center"
        >
          <svg class="animate-spin h-3.5 w-3.5 shrink-0" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          <span class="truncate">{{ installProgressMessage }}</span>
        </div>
        <div
          v-if="(installProgress?.progress ?? 0) > 0"
          class="w-full h-1 bg-white/10 rounded-full overflow-hidden"
        >
          <div
            class="h-full bg-orange-400 transition-all duration-300"
            :style="{ width: (installProgress?.progress ?? 0) + '%' }"
          ></div>
        </div>
      </div>
      <button
        v-else-if="!installed && installBlockedReason"
        class="flex-1 px-4 py-2 bg-yellow-500/15 border border-yellow-500/30 rounded-lg text-yellow-100 text-sm font-medium"
        :title="installBlockedReason"
        @click.stop="$emit('install', app)"
      >
        Bitcoin Pruned
      </button>
      <button
        v-else-if="!installed && (app.source === 'local' || app.dockerImage)"
        data-controller-install-btn
        @click.stop="$emit('install', app)"
        class="flex-1 px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium"
      >
        {{ t('common.install') }}
      </button>
      <button
        v-else-if="!installed"
        disabled
        class="flex-1 px-4 py-2 bg-white/10 rounded-lg text-white/40 text-sm font-medium cursor-not-allowed"
      >
        {{ t('common.notAvailable') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { MarketplaceApp, InstallProgress } from './marketplaceData'
import { DEFAULT_APP_ICON } from '@/views/apps/appsConfig'

const { t } = useI18n()

const props = defineProps<{
  app: MarketplaceApp
  index: number
  stagger: boolean
  installed: boolean
  installing: boolean
  installProgress: InstallProgress | undefined
  installedState: string | null
  startingUp: boolean
  containersScanned: boolean
  tierLabel: string
  installBlockedReason?: string
}>()

defineEmits<{
  view: [app: MarketplaceApp]
  install: [app: MarketplaceApp]
  launch: [app: MarketplaceApp]
}>()

const signatureLabel = computed(() => {
  switch (props.app.signature?.status) {
    case 'valid': return 'signed'
    case 'invalid': return 'bad signature'
    default: return 'unsigned'
  }
})

/**
 * The badge is two words; the tooltip carries the meaning. "signed" is easy to
 * read as "safe", so say what was actually proven — that the author holds the
 * key their DID names — and nothing more.
 */
const signatureTooltip = computed(() => {
  const sig = props.app.signature
  const did = props.app.authorDid
  const shortDid = did && did.length > 24 ? `${did.slice(0, 16)}…${did.slice(-6)}` : did
  switch (sig?.status) {
    case 'valid':
      return `Authorship verified: signed by the key ${shortDid ?? 'in author.did'}. `
        + 'This proves who published it, not that the app is safe.'
    case 'invalid':
      return `Signature did not verify: ${sig.reason}`
    default:
      return 'No author signature — the publisher\'s identity is unproven. '
        + 'The app may still be fine; nothing has been demonstrated about who wrote it.'
  }
})

const installProgressMessage = computed(() => {
  const p = props.installProgress
  if (!p) return 'Installing'
  // The store already formats messages like "Downloading: 50.5 / 200.0 MB (25%)"
  // so we just surface them directly.
  return p.message || 'Installing'
})

function handleImageError(event: Event) {
  const img = event.target as HTMLImageElement
  // Catalog icon extensions drift (.svg vs .png on disk) — try the sibling
  // extension once before giving up on the placeholder
  if (!img.dataset.extRetry && /\.(svg|png)$/.test(img.src)) {
    img.dataset.extRetry = '1'
    img.src = img.src.endsWith('.svg')
      ? img.src.replace(/\.svg$/, '.png')
      : img.src.replace(/\.png$/, '.svg')
    return
  }
  if (!img.src.includes(DEFAULT_APP_ICON)) {
    img.src = DEFAULT_APP_ICON
    img.dataset.defaultIcon = '1'
  }
}
</script>

<style scoped>
.marketplace-shimmer-bg {
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

.line-clamp-3 {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
</style>
