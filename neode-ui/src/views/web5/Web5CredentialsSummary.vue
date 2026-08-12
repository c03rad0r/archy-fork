<template>
  <!-- Verifiable Credentials -->
  <div class="glass-card p-6 mb-8">
    <!-- Desktop: side-by-side -->
    <div class="hidden md:flex items-center justify-between mb-4">
      <div class="flex items-center gap-3">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
          </svg>
        </div>
        <div>
          <h2 class="text-lg font-semibold text-white">{{ t('web5.verifiableCredentials') }}</h2>
          <p class="text-xs text-white/60">{{ t('web5.verifiableCredentialsDesc') }}</p>
        </div>
      </div>
      <router-link to="/dashboard/web5/credentials" class="glass-button glass-button-sm px-3 rounded-lg text-sm font-medium flex items-center gap-2">
        Manage &rarr;
      </router-link>
    </div>
    <!-- Mobile: stacked -->
    <div class="md:hidden mb-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
          </svg>
        </div>
        <h2 class="text-lg font-semibold text-white">{{ t('web5.verifiableCredentials') }}</h2>
      </div>
      <p class="text-xs text-white/60 mb-3">{{ t('web5.verifiableCredentialsDesc') }}</p>
      <router-link to="/dashboard/web5/credentials" class="w-full min-h-[44px] glass-button rounded-lg text-sm font-medium flex items-center justify-center gap-2">
        Manage &rarr;
      </router-link>
    </div>

    <!-- Stats -->
    <div class="grid grid-cols-3 gap-3 mb-4">
      <div class="bg-white/5 rounded-lg p-3">
        <div class="text-xs text-white/50 mb-1">Total</div>
        <span class="text-sm text-white font-medium">{{ vcCredentials.length }}</span>
      </div>
      <div class="bg-white/5 rounded-lg p-3">
        <div class="text-xs text-white/50 mb-1">Active</div>
        <span class="text-sm text-green-400 font-medium">{{ vcCredentials.filter(c => c.status === 'active').length }}</span>
      </div>
      <div class="bg-white/5 rounded-lg p-3">
        <div class="text-xs text-white/50 mb-1">Identities</div>
        <span class="text-sm text-white font-medium">{{ identityCount }}</span>
      </div>
    </div>

    <!-- Credentials List (summary) -->
    <div v-if="credentialsLoading && vcCredentials.length === 0" class="text-center text-white/40 text-sm py-4">
      Loading credentials...
    </div>
    <div v-else-if="vcCredentials.length" class="space-y-2">
      <div v-if="credentialsLoading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing credentials...
      </div>
      <div v-else-if="credentialsError" class="p-2 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-xs">
        {{ credentialsError }}
      </div>
      <div v-for="vc in vcCredentials.slice(0, 3)" :key="vc.id" class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="min-w-0 flex-1">
          <div class="text-sm text-white font-medium">{{ vc.type }}</div>
          <div class="text-xs text-white/50 truncate">To: {{ (vc.subject || '').slice(0, 30) }}...</div>
        </div>
        <span :class="{
          'text-green-400': vc.status === 'active',
          'text-red-400': vc.status === 'revoked',
          'text-yellow-400': vc.status === 'expired'
        }" class="text-xs font-medium capitalize">{{ vc.status }}</span>
      </div>
      <router-link v-if="vcCredentials.length > 3" to="/dashboard/web5/credentials" class="block text-center text-xs text-white/50 hover:text-white/70 py-2 transition-colors">
        View all {{ vcCredentials.length }} credentials &rarr;
      </router-link>
    </div>
    <div v-else class="text-center text-white/40 text-sm py-4">
      {{ t('web5.noCredentials') }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import type { VCData } from './types'

const { t } = useI18n()

defineProps<{
  identityCount: number
}>()

const vcCredentials = ref<VCData[]>([])
const credentialsLoading = ref(false)
const credentialsError = ref('')

async function loadCredentials() {
  const hadCredentials = vcCredentials.value.length > 0
  credentialsLoading.value = true
  credentialsError.value = ''
  try {
    const res = await rpcClient.call<{ credentials: VCData[] }>({ method: 'identity.list-credentials' })
    vcCredentials.value = res.credentials || []
  } catch (e: unknown) {
    credentialsError.value = e instanceof Error ? e.message : 'Failed to load credentials'
    if (!hadCredentials) vcCredentials.value = []
  } finally {
    credentialsLoading.value = false
  }
}

defineExpose({ loadCredentials })
</script>
