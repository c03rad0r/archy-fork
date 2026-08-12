<template>
  <BaseModal :show="show" title="" max-width="max-w-lg" @close="emit('close')">
    <!-- Header: app icon + "Install Bitcoin Knots/Core" -->
    <div class="flex items-center gap-4 mb-5 -mt-2">
      <!-- object-contain, not the default fill: app icons are not all square
           (bitcoin-knots is not), so a fixed 56x56 box distorted or cropped the
           mark against the rounded corners. Contain plus a dark plate shows the
           whole icon whatever its aspect ratio. -->
      <img
        v-if="app?.icon"
        :src="app.icon"
        :alt="app?.title || ''"
        class="w-14 h-14 rounded-xl shadow-lg shrink-0 object-contain bg-black/40 p-1"
      />
      <div v-else class="w-14 h-14 rounded-xl bg-white/10 flex items-center justify-center shrink-0">
        <svg class="w-7 h-7 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
        </svg>
      </div>
      <h3 class="text-xl font-semibold text-white leading-snug">
        {{ t('marketplace.installModalTitle', { name: app?.title || appId }) }}
      </h3>
    </div>

    <div v-if="loading" class="py-6 text-center text-white/60 text-sm">{{ t('common.loading') }}</div>

    <div v-else class="space-y-2">
      <label class="block text-white/60 text-sm">{{ t('appDetails.selectVersion') }}</label>
      <select
        v-model="selected"
        class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white pl-3 pr-9 py-2 text-sm focus:outline-none focus:border-blue-400/60"
      >
        <option v-for="v in versions" :key="v.version" :value="v.version">{{ optionLabel(v) }}</option>
      </select>
      <p class="text-white/40 text-xs">{{ t('marketplace.installModalHint') }}</p>
    </div>

    <template #footer>
      <div class="flex gap-2 mt-6">
        <button
          type="button"
          class="flex-1 glass-button glass-button-warning rounded-lg disabled:opacity-50 text-sm font-semibold py-2.5"
          :disabled="loading || !selected"
          @click="confirm"
        >
          {{ t('common.install') }}
        </button>
        <button
          type="button"
          class="rounded-lg bg-white/10 hover:bg-white/20 text-white text-sm font-medium px-5 py-2.5 transition-colors"
          @click="emit('close')"
        >
          {{ t('common.cancel') }}
        </button>
      </div>
    </template>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import BaseModal from './BaseModal.vue'
import { rpcClient, type CatalogVersionInfo } from '../api/rpc-client'
import { displayVersion } from '@/utils/version'

const props = defineProps<{
  show: boolean
  appId: string
  app: { id: string; title?: string; icon?: string } | null
}>()

const emit = defineEmits<{
  close: []
  // Emits the version string the runner chose (e.g. "latest" or "29.3.knots20260508").
  confirm: [version: string]
}>()

const { t } = useI18n()
const loading = ref(false)
const versions = ref<CatalogVersionInfo[]>([])
const selected = ref('')

// Latest reads as a sentence (no "v" prefix); concrete versions are normalized.
function optionLabel(v: CatalogVersionInfo): string {
  if (v.version === 'latest') return t('appDetails.alwaysUseLatestVersion')
  let label = displayVersion(v.version)
  if (v.deprecated) label += ' (deprecated)'
  if (v.eol) label += ` · EOL ${v.eol}`
  return label
}

async function load() {
  loading.value = true
  versions.value = []
  selected.value = ''
  try {
    const info = await rpcClient.getPackageVersions(props.appId)
    // catalog_versions() returns the list default(=latest)-first, so versions[0]
    // is the latest — pre-select it.
    versions.value = info.versions || []
    selected.value = info.default || versions.value.find((v) => v.default)?.version || versions.value[0]?.version || 'latest'
  } catch (err) {
    if (import.meta.env.DEV) console.warn('[InstallVersionModal] getPackageVersions failed:', err)
    // Fall back to the floating "latest" so the install can still proceed.
    selected.value = 'latest'
  } finally {
    loading.value = false
  }
}

function confirm() {
  if (!selected.value) return
  emit('confirm', selected.value)
}

watch(
  () => props.show,
  (open) => {
    if (open) void load()
  },
  { immediate: true },
)
</script>
