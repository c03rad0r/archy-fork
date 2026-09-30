<template>
  <div class="app-icon-grid-wrap">
    <!-- Swipeable pages -->
    <div
      ref="scrollContainer"
      class="app-icon-pages"
      @scroll="onScroll"
    >
      <div
        v-for="(page, pageIndex) in pages"
        :key="pageIndex"
        class="app-icon-page"
      >
        <div
          v-for="([id, pkg]) in page"
          :key="id"
          class="app-icon-item"
          role="button"
          :tabindex="0"
          :aria-label="getTitle(id, pkg)"
          @pointerdown="startLongPress(id)"
          @pointerup="clearLongPress"
          @pointercancel="clearLongPress"
          @pointerleave="clearLongPress"
          @contextmenu.prevent="openAppOptions(id)"
          @click="handleTap(id, pkg)"
          @keydown.enter="handleTap(id, pkg)"
          @keydown.space.prevent="openAppOptions(id)"
        >
          <!-- Icon with status indicator -->
          <div class="app-icon-frame">
            <img
              :src="getIcon(id, pkg)"
              :alt="getTitle(id, pkg)"
              class="app-icon-img archy-app-icon"
              @error="handleImageError"
            />
            <!-- Status dot -->
            <span
              v-if="pkg.state === 'running'"
              class="app-icon-status app-icon-status-running"
            ></span>
            <span
              v-else-if="pkg.state === 'exited'"
              class="app-icon-status app-icon-status-error"
            ></span>
            <span
              v-else-if="pkg.state === 'starting' || pkg.state === 'stopping' || pkg.state === 'installing'"
              class="app-icon-status app-icon-status-transition"
            ></span>
            <!-- Installing overlay -->
            <div
              v-if="serverStore.isInstalling(id) || serverStore.uninstallingApps.has(id)"
              class="app-icon-installing"
            >
              <svg class="animate-spin h-5 w-5 text-white" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
            </div>
            <!-- Launching overlay — instant tap feedback while the app opens -->
            <div v-if="launchingId === id" class="app-icon-installing">
              <svg class="animate-spin h-5 w-5 text-white" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
            </div>
          </div>
          <!-- Label -->
          <span class="app-icon-label">{{ getTitle(id, pkg) }}</span>
          <span
            v-if="serverStore.isInstalling(id) || serverStore.uninstallingApps.has(id)"
            class="app-icon-progress-label"
            :title="progressLabel(id, pkg)"
          >
            {{ progressLabel(id, pkg) }}
          </span>
        </div>
      </div>
    </div>

    <!-- Page dots -->
    <div v-if="pages.length > 1" class="app-icon-dots">
      <button
        v-for="(_, i) in pages"
        :key="i"
        class="app-icon-dot"
        :class="{ 'app-icon-dot-active': i === activePage }"
        :aria-label="`Page ${i + 1}`"
        @click="scrollToPage(i)"
      ></button>
    </div>

    <!-- House modal (BaseModal glass-card), not a hand-rolled panel: the old
         one painted its own rgba(8,10,18,.98) navy card, which read as blue
         against every other modal in the app. -->
    <BaseModal
      :show="credentialModal.show"
      :title="credentialModal.title"
      max-width="max-w-lg"
      z-index="z-[2700]"
      @close="closeCredentialModal"
    >
      <p v-if="credentialModal.description" class="text-sm text-white/55 -mt-1 mb-4">
        {{ credentialModal.description }}
      </p>
      <div class="space-y-3">
        <div v-for="cred in credentialModal.credentials" :key="cred.label" class="rounded-lg border border-white/10 bg-white/[0.04] p-3">
          <div class="flex items-center justify-between gap-3 mb-1">
            <span class="text-white/60 text-xs uppercase tracking-wide">{{ cred.label }}</span>
            <button type="button" class="text-xs text-orange-300 hover:text-orange-200" @click="copyModalCredential(cred.label, cred.value)">{{ credentialModal.copied === cred.label ? 'Copied' : 'Copy' }}</button>
          </div>
          <p class="font-mono text-sm text-white break-all">{{ cred.value }}</p>
        </div>
      </div>
      <template #footer>
        <div class="flex flex-col sm:flex-row gap-3">
          <button type="button" class="w-full sm:flex-1 glass-button px-4 py-3 rounded-lg" @click="closeCredentialModal">Cancel</button>
          <button type="button" class="w-full sm:flex-1 glass-button px-4 py-3 rounded-lg font-semibold" @click="continueCredentialLaunch">Continue to app</button>
        </div>
      </template>
    </BaseModal>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import BaseModal from '@/components/BaseModal.vue'
import { useServerStore } from '@/stores/server'
import { useAppLauncherStore } from '@/stores/appLauncher'
import type { AppCredential, AppCredentialsResponse, PackageDataEntry } from '@/types/api'
import { rpcClient } from '@/api/rpc-client'
import { resolveAppUrl } from '@/views/appSession/appSessionConfig'
import { resolveAppCredentials } from './appCredentials'
import { canLaunch, handleImageError, isWebsitePackage, opensInTab, resolveAppIcon, resolveAppTitle, resolveRuntimeLaunchUrl, WEB_ONLY_APP_URLS } from './appsConfig'
import { getCuratedAppList } from '../discover/curatedApps'

const ITEMS_PER_PAGE = 16 // 4 columns x 4 rows

const serverStore = useServerStore()
const appLauncher = useAppLauncherStore()

const curatedMap = new Map(getCuratedAppList().map(a => [a.id, a]))
const credentialModal = ref({
  show: false,
  appId: '',
  title: '',
  description: '',
  credentials: [] as AppCredential[],
  copied: '',
})

const props = defineProps<{
  apps: [string, PackageDataEntry][]
}>()

const emit = defineEmits<{
  goToApp: [id: string]
}>()

const scrollContainer = ref<HTMLElement | null>(null)
const activePage = ref(0)
const longPressTriggered = ref(false)
let longPressTimer: ReturnType<typeof setTimeout> | null = null

// Per-icon "launching" spinner so a tap is acknowledged instantly even while
// the app session/iframe is still spinning up. Cleared when the launcher
// overlay opens, with a fallback timeout for the open-in-new-tab path.
const launchingId = ref<string | null>(null)
let launchClearTimer: ReturnType<typeof setTimeout> | null = null

function markLaunching(id: string) {
  launchingId.value = id
  if (launchClearTimer) clearTimeout(launchClearTimer)
  launchClearTimer = setTimeout(() => {
    if (launchingId.value === id) launchingId.value = null
  }, 4000)
}

// Clear the spinner as soon as the app overlay actually opens.
watch(() => appLauncher.isOpen, (open) => {
  if (open) {
    launchingId.value = null
    if (launchClearTimer) { clearTimeout(launchClearTimer); launchClearTimer = null }
  }
})

const pages = computed(() => {
  const result: [string, PackageDataEntry][][] = []
  for (let i = 0; i < props.apps.length; i += ITEMS_PER_PAGE) {
    result.push(props.apps.slice(i, i + ITEMS_PER_PAGE))
  }
  return result.length ? result : [[]]
})

function getTitle(id: string, pkg: PackageDataEntry): string {
  return resolveAppTitle(id, pkg, curatedMap.get(id)?.title)
}

function getIcon(id: string, pkg: PackageDataEntry): string {
  return resolveAppIcon(id, pkg, curatedMap.get(id)?.icon)
}

function progressLabel(id: string, pkg: PackageDataEntry): string {
  const install = serverStore.installingApps.get(id)
  if (install) {
    return `${install.message || 'Installing...'} ${Math.round(install.progress || 0)}%`
  }
  if (serverStore.uninstallingApps.has(id)) {
    return pkg['uninstall-stage'] || ((pkg as unknown as Record<string, unknown>).uninstall_stage as string | undefined) || 'Removing...'
  }
  return ''
}

async function handleTap(id: string, pkg: PackageDataEntry) {
  if (longPressTriggered.value) {
    longPressTriggered.value = false
    return
  }
  if (canLaunch(pkg)) {
    const shown = await maybeShowCredentialsBeforeLaunch(id, pkg)
    if (shown) return
    launchNow(id, pkg, true)
  } else {
    emit('goToApp', id)
  }
}

function startLongPress(id: string) {
  clearLongPress()
  longPressTriggered.value = false
  longPressTimer = setTimeout(() => {
    longPressTriggered.value = true
    openAppOptions(id)
  }, 550)
}

function clearLongPress() {
  if (!longPressTimer) return
  clearTimeout(longPressTimer)
  longPressTimer = null
}

function openAppOptions(id: string) {
  clearLongPress()
  emit('goToApp', id)
}

function launchNow(id: string, pkg: PackageDataEntry, credentialsChecked = false) {
  markLaunching(id)
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768
  const webOnlyUrl = WEB_ONLY_APP_URLS[id]
  if (webOnlyUrl) {
    appLauncher.open({ url: webOnlyUrl, title: getTitle(id, pkg), openInNewTab: !isMobile })
    return
  }
  if (isWebsitePackage(id, pkg)) {
    const url = resolveRuntimeLaunchUrl(pkg)
    if (url) {
      appLauncher.open({ url, title: getTitle(id, pkg), openInNewTab: !isMobile })
      return
    }
  }
  if (!isMobile && opensInTab(id)) {
    const appUrl = resolveRuntimeLaunchUrl(pkg) || resolveAppUrl(id)
    if (appUrl) {
      window.open(appUrl, '_blank', 'noopener,noreferrer')
      return
    }
  }
  appLauncher.openSession(id, { skipCredentialPrompt: credentialsChecked })
}

async function maybeShowCredentialsBeforeLaunch(id: string, pkg: PackageDataEntry): Promise<boolean> {
  try {
    const result = await rpcClient.call<AppCredentialsResponse>({ method: 'package.credentials', params: { app_id: id }, timeout: 5000 })
    const credentials = resolveAppCredentials(id, result)
    if (!credentials) return false
    credentialModal.value = {
      show: true,
      appId: id,
      title: credentials.title || `${getTitle(id, pkg)} credentials`,
      description: credentials.description || 'Use these credentials when the app asks you to sign in.',
      credentials: credentials.credentials,
      copied: '',
    }
    return true
  } catch {
    const credentials = resolveAppCredentials(id, null)
    if (!credentials) return false
    credentialModal.value = {
      show: true,
      appId: id,
      title: credentials.title || `${getTitle(id, pkg)} credentials`,
      description: credentials.description || 'Use these credentials when the app asks you to sign in.',
      credentials: credentials.credentials,
      copied: '',
    }
    return true
  }
}

function closeCredentialModal() { credentialModal.value.show = false }

function continueCredentialLaunch() {
  const id = credentialModal.value.appId
  const entry = props.apps.find(([appId]) => appId === id)
  closeCredentialModal()
  if (entry) launchNow(entry[0], entry[1], true)
}

async function copyModalCredential(label: string, value: string) {
  try { await navigator.clipboard.writeText(value) } catch {
    const textarea = document.createElement('textarea')
    textarea.value = value
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
  }
  credentialModal.value.copied = label
}

function onScroll() {
  const el = scrollContainer.value
  if (!el) return
  const pageWidth = el.clientWidth
  if (pageWidth === 0) return
  activePage.value = Math.round(el.scrollLeft / pageWidth)
}

function scrollToPage(index: number) {
  const el = scrollContainer.value
  if (!el) return
  el.scrollTo({ left: index * el.clientWidth, behavior: 'smooth' })
}
</script>

<style scoped>
/* Instant press feedback: the icon scales down the moment it's touched, so the
   tap is acknowledged even before the app finishes launching. */
.app-icon-frame {
  transition: transform 0.12s ease;
}
.app-icon-item:active .app-icon-frame {
  transform: scale(0.88);
}

.sideload-close-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.25rem;
  height: 2.25rem;
  flex-shrink: 0;
  border-radius: 0.75rem;
  color: rgba(255, 255, 255, 0.7);
  background: transparent;
  transition: background-color 0.15s ease, color 0.15s ease;
}
.sideload-close-btn:hover,
.sideload-close-btn:focus-visible {
  background: rgba(255, 255, 255, 0.1);
  color: white;
}
</style>
