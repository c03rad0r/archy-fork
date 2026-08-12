<template>
  <div class="space-y-6">
    <!-- App Info Card -->
    <div class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-4">{{ t('appDetails.information') }}</h3>
      <div class="space-y-3">
        <div class="flex items-center justify-between py-2 border-b border-white/10">
          <span class="text-white/60 text-sm">{{ t('common.version') }}</span>
          <div class="text-right">
            <span class="text-white font-medium">{{ $ver(pkg.manifest.version) }}</span>
            <span v-if="pkg['available-update']" class="text-orange-300 text-xs ml-2">
              {{ $ver(pkg['available-update']) }} available
            </span>
          </div>
        </div>
        <div v-if="pkg.manifest.author" class="flex items-center justify-between py-2 border-b border-white/10">
          <span class="text-white/60 text-sm">{{ t('common.developer') }}</span>
          <span class="text-white font-medium">{{ pkg.manifest.author }}</span>
        </div>
        <div class="flex items-center justify-between py-2 border-b border-white/10">
          <span class="text-white/60 text-sm">{{ t('common.status') }}</span>
          <span class="text-white font-medium capitalize">{{ pkg.state }}</span>
        </div>
        <div v-if="pkg.manifest.license" class="flex items-center justify-between py-2 border-b border-white/10">
          <span class="text-white/60 text-sm">{{ t('common.license') }}</span>
          <span class="text-white font-medium">{{ pkg.manifest.license }}</span>
        </div>
        <div class="flex items-center justify-between py-2">
          <span class="text-white/60 text-sm">{{ t('common.category') }}</span>
          <span class="text-white font-medium">App</span>
        </div>
      </div>
    </div>

    <!-- Version & Updates Card (multi-version apps: Bitcoin Core / Knots).
         Lets a runner switch versions, pin, and opt into auto-update. See
         docs/bitcoin-multi-version-design.md §3 Phase 3. -->
    <div v-if="versionInfo && versionInfo.supportsVersions && versionInfo.versions.length" class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-4">{{ t('appDetails.versionUpdates') }}</h3>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-white/60 text-sm">{{ t('appDetails.runningVersion') }}</span>
          <span class="text-white font-medium">{{ versionInfo.installedVersion || $ver(pkg.manifest.version) }}</span>
        </div>

        <div>
          <label class="block text-white/60 text-sm mb-1">{{ t('appDetails.selectVersion') }}</label>
          <select
            v-model="selectedVersion"
            :disabled="versionBusy"
            class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white pl-3 pr-9 py-2 text-sm focus:outline-none focus:border-orange-400/60"
          >
            <option v-for="v in versionInfo.versions" :key="v.version" :value="v.version">{{ versionOptionLabel(v) }}</option>
          </select>
        </div>

        <label class="flex items-center justify-between gap-3 cursor-pointer">
          <span class="text-white/80 text-sm">{{ t('appDetails.autoUpdateLatest') }}</span>
          <input
            type="checkbox"
            v-model="autoUpdate"
            :disabled="versionBusy || isPinned"
            class="h-4 w-4 accent-orange-500"
          />
        </label>
        <p v-if="isPinned" class="text-white/40 text-xs -mt-2">{{ t('appDetails.autoUpdatePinnedNote') }}</p>

        <!-- Downgrade confirmation -->
        <div v-if="downgradeWarning" class="rounded-lg border border-orange-400/40 bg-orange-500/10 p-3">
          <p class="text-orange-200 text-xs leading-relaxed">⚠️ {{ downgradeWarning }}</p>
          <div class="flex gap-2 mt-3">
            <button type="button" class="text-xs px-3 py-1.5 rounded-md bg-orange-500/80 hover:bg-orange-500 text-white" :disabled="versionBusy" @click="applyVersionConfig(true)">
              {{ t('appDetails.confirmDowngrade') }}
            </button>
            <button type="button" class="text-xs px-3 py-1.5 rounded-md bg-white/10 hover:bg-white/20 text-white" :disabled="versionBusy" @click="cancelDowngrade">
              {{ t('common.cancel') }}
            </button>
          </div>
        </div>

        <button
          v-else
          type="button"
          class="w-full glass-button glass-button-warning rounded-lg disabled:opacity-50 text-sm font-medium py-2"
          :disabled="versionBusy || !versionDirty"
          @click="applyVersionConfig(false)"
        >
          {{ versionBusy ? t('appDetails.applyingVersion') : t('appDetails.applyVersion') }}
        </button>
        <p v-if="versionError" class="text-red-300 text-xs">{{ versionError }}</p>
      </div>
    </div>

    <!-- Fedimint Services Card -->
    <div v-if="packageKey === 'fedimint'" class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-4">{{ t('appDetails.services') }}</h3>
      <div class="space-y-3">
        <div class="flex items-center gap-3 py-2 border-b border-white/10">
          <span class="w-2 h-2 rounded-full" :class="pkg.state === 'running' ? 'bg-green-400' : 'bg-yellow-400'"></span>
          <div class="flex-1">
            <p class="text-white/80 font-medium text-sm">{{ t('appDetails.guardian') }}</p>
            <p class="text-white/50 text-xs capitalize">{{ pkg.state }}</p>
          </div>
        </div>
        <div class="flex items-center gap-3 py-2">
          <span class="w-2 h-2 rounded-full" :class="gatewayState === 'running' ? 'bg-green-400' : gatewayState === 'stopped' ? 'bg-yellow-400' : 'bg-red-400'"></span>
          <div class="flex-1">
            <p class="text-white/80 font-medium text-sm">{{ t('appDetails.gateway') }}</p>
            <p class="text-white/50 text-xs capitalize">{{ gatewayState }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Access (LAN + Tor) Card -->
    <div v-if="interfaceAddresses" class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-4">{{ t('appDetails.access') }}</h3>
      <div class="space-y-3">
        <div v-if="interfaceAddresses['lan-address']" class="flex items-start gap-3">
          <svg class="w-5 h-5 text-green-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
          </svg>
          <div class="flex-1 min-w-0">
            <p class="text-white/80 font-medium">{{ t('appDetails.lan') }}</p>
            <a
              :href="lanUrl"
              target="_blank"
              rel="noopener noreferrer"
              class="text-orange-300 hover:text-orange-200 text-sm break-all"
            >
              {{ interfaceAddresses['lan-address'] }}
            </a>
          </div>
        </div>
        <div v-if="showTorAddress" class="flex items-start gap-3">
          <svg class="w-5 h-5 text-amber-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
          </svg>
          <div class="flex-1 min-w-0">
            <p class="text-white/80 font-medium">{{ t('appDetails.tor') }}</p>
            <span class="text-amber-300/90 text-sm font-mono break-all">{{ torUrl }}</span>
            <p class="text-white/50 text-xs mt-1">{{ t('appDetails.requiresTor') }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Setup Instructions -->
    <div v-if="setupInstructions" class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-3">{{ t('appDetails.setupInstructions') }}</h3>
      <p class="text-sm text-white/70 leading-relaxed whitespace-pre-line">
        {{ setupInstructions }}
      </p>
    </div>

    <div v-if="credentialsLoading || credentials?.credentials?.length" class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-2">Credentials</h3>
      <p v-if="credentials?.description" class="text-sm text-white/60 mb-4">{{ credentials.description }}</p>
      <div v-if="credentialsLoading" class="space-y-3" aria-live="polite">
        <div class="rounded-lg border border-white/10 bg-white/[0.04] p-3">
          <div class="h-3 w-28 rounded bg-white/10 mb-3"></div>
          <div class="h-4 w-full rounded bg-white/10"></div>
        </div>
        <p class="text-xs text-white/50">Loading credentials...</p>
      </div>
      <div v-else-if="credentials" class="space-y-3">
        <div v-for="cred in credentials.credentials" :key="cred.label" class="rounded-lg border border-white/10 bg-white/[0.04] p-3">
          <div class="flex items-center justify-between gap-3 mb-1">
            <span class="text-white/60 text-xs uppercase tracking-wide">{{ cred.label }}</span>
            <button type="button" class="text-xs text-orange-300 hover:text-orange-200" @click="copyCredential(cred.label, cred.value)">
              {{ copiedCredential === cred.label ? 'Copied' : 'Copy' }}
            </button>
          </div>
          <p class="font-mono text-sm text-white break-all">{{ cred.value }}</p>
        </div>
      </div>
    </div>

    <!-- Requirements Card (hidden for web-only apps) -->
    <div v-if="!isWebOnly" class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-4">{{ t('appDetails.requirements') }}</h3>
      <div class="space-y-3">
        <div class="flex items-start gap-3">
          <svg class="w-5 h-5 text-orange-300 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z" />
          </svg>
          <div class="flex-1">
            <p class="text-white/80 font-medium">{{ t('appDetails.ram') }}</p>
            <p class="text-white/60 text-sm">{{ t('appDetails.ramDesc') }}</p>
          </div>
        </div>
        <div class="flex items-start gap-3">
          <svg class="w-5 h-5 text-purple-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
          </svg>
          <div class="flex-1">
            <p class="text-white/80 font-medium">{{ t('appDetails.storage') }}</p>
            <p class="text-white/60 text-sm">{{ t('appDetails.storageDesc') }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Links Card -->
    <div v-if="links.length > 0" class="glass-card p-6">
      <h3 class="text-lg font-bold text-white mb-4">{{ t('appDetails.links') }}</h3>
      <div class="space-y-2">
        <a
          v-for="link in links"
          :key="link.kind"
          :href="link.url"
          target="_blank"
          rel="noopener noreferrer"
          class="flex items-center gap-2 text-orange-300 hover:text-orange-200 transition-colors"
        >
          <svg
            v-if="link.kind === 'website'"
            class="w-5 h-5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
          </svg>
          <svg
            v-else-if="link.kind === 'source'"
            class="w-5 h-5"
            fill="currentColor"
            viewBox="0 0 24 24"
          >
            <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.840 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
          </svg>
          <svg
            v-else
            class="w-5 h-5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          {{ link.label }}
        </a>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type { AppCredentialsResponse } from '@/types/api'
import { rpcClient, type PackageVersionsResponse, type CatalogVersionInfo } from '../../api/rpc-client'
import { displayVersion } from '@/utils/version'

const { t } = useI18n()
const copiedCredential = ref('')

async function copyCredential(label: string, value: string) {
  try {
    await navigator.clipboard.writeText(value)
  } catch {
    const textarea = document.createElement('textarea')
    textarea.value = value
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
  }
  copiedCredential.value = label
  setTimeout(() => {
    if (copiedCredential.value === label) copiedCredential.value = ''
  }, 1800)
}

const props = defineProps<{
  pkg: Record<string, any>
  packageKey: string
  isWebOnly: boolean
  gatewayState: string
  interfaceAddresses: { 'tor-address': string; 'lan-address': string | null } | null
  lanUrl: string
  torUrl: string
  showTorAddress: boolean
  credentials: AppCredentialsResponse | null
  credentialsLoading: boolean
}>()

type LinkKind = 'website' | 'source' | 'documentation'

interface SidebarLink {
  kind: LinkKind
  label: string
  url: string
}

function normalizeHttpUrl(value: unknown): string {
  const url = typeof value === 'string' ? value.trim() : ''
  if (!url || url === '#') return ''
  if (/^https?:\/\//i.test(url)) return url
  return ''
}

const links = computed<SidebarLink[]>(() => {
  const manifest = props.pkg.manifest || {}
  const website = normalizeHttpUrl(manifest.website || manifest['marketing-site'])
  const source = normalizeHttpUrl(manifest['upstream-repo'] || manifest['wrapper-repo'])
  const documentation = normalizeHttpUrl(manifest['support-site'])

  return [
    website ? { kind: 'website' as const, label: t('appDetails.website'), url: website } : null,
    source ? { kind: 'source' as const, label: t('appDetails.sourceCode'), url: source } : null,
    documentation ? { kind: 'documentation' as const, label: t('appDetails.documentation'), url: documentation } : null,
  ].filter((link): link is SidebarLink => link !== null)
})

const setupInstructions = computed(() => {
  const raw = props.pkg['static-files']?.instructions
  const instructions = typeof raw === 'string' ? raw.trim() : ''
  return instructions ? instructions : ''
})

// ---- Version & Updates (multi-version support) -----------------------------
const versionInfo = ref<PackageVersionsResponse | null>(null)
const selectedVersion = ref('')
const autoUpdate = ref(false)
const versionBusy = ref(false)
const versionError = ref('')
const downgradeWarning = ref('')

const isPinned = computed(() => !!versionInfo.value?.pinnedVersion)
// "Apply" is enabled when the runner changed the version or the toggle.
const versionDirty = computed(() => {
  const info = versionInfo.value
  if (!info) return false
  return selectedVersion.value !== pickSelection(info) || autoUpdate.value !== info.autoUpdate
})

// Option label: the floating "latest" entry reads as a sentence (no "v"
// prefix); every concrete version is normalized via $ver + status suffixes.
function versionOptionLabel(v: CatalogVersionInfo): string {
  if (v.version === 'latest') return t('appDetails.alwaysUseLatestVersion')
  let label = displayVersion(v.version)
  if (v.deprecated) label += ' (deprecated)'
  if (v.eol) label += ` · EOL ${v.eol}`
  return label
}

// Resolve the dropdown's current value and GUARANTEE it matches a real option,
// otherwise the native <select> renders blank. Prefer the pin, then the catalog
// default, then the running version — but fall back to the default/first option
// when none of those is actually in the list (e.g. a stale installedVersion the
// catalog no longer carries, which is what left the control blank).
function pickSelection(info: PackageVersionsResponse): string {
  const options = info.versions.map((v) => v.version)
  const preferred = info.pinnedVersion || info.default || info.installedVersion || ''
  if (preferred && options.includes(preferred)) return preferred
  return info.default && options.includes(info.default)
    ? info.default
    : info.versions[0]?.version || ''
}

async function loadVersions(appId: string) {
  versionInfo.value = null
  versionError.value = ''
  downgradeWarning.value = ''
  try {
    const info = await rpcClient.getPackageVersions(appId)
    if (!info.supportsVersions || !info.versions.length) return
    versionInfo.value = info
    selectedVersion.value = pickSelection(info)
    autoUpdate.value = info.autoUpdate
  } catch (err) {
    if (import.meta.env.DEV) console.warn('[AppSidebar] getPackageVersions failed:', err)
  }
}

async function applyVersionConfig(confirm: boolean) {
  if (!versionInfo.value) return
  versionBusy.value = true
  versionError.value = ''
  try {
    const res = await rpcClient.setPackageConfig(versionInfo.value.id, {
      version: selectedVersion.value,
      autoUpdate: autoUpdate.value,
      confirm,
    })
    if (res.status === 'confirm_required') {
      downgradeWarning.value = res.warning || t('appDetails.downgradeGeneric')
      return
    }
    downgradeWarning.value = ''
    // Refresh so the card reflects the new pin / running version.
    await loadVersions(versionInfo.value.id)
  } catch (err: unknown) {
    versionError.value = err instanceof Error ? err.message : String(err)
  } finally {
    versionBusy.value = false
  }
}

function cancelDowngrade() {
  downgradeWarning.value = ''
  // Reset the dropdown to the current selection.
  const info = versionInfo.value
  if (info) selectedVersion.value = pickSelection(info)
}

watch(
  () => props.packageKey,
  (key) => {
    if (key) void loadVersions(key)
  },
  { immediate: true },
)
</script>
