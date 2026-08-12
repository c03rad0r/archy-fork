<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="$emit('close')">
    <div class="w-full max-w-lg mx-4 max-h-[80vh] rounded-2xl bg-[#0a0a0a] border border-white/10 flex flex-col overflow-hidden">
      <!-- Header -->
      <div class="p-4 border-b border-white/[0.08] flex items-center justify-between shrink-0">
        <h2 class="text-sm font-bold text-white/90">Settings</h2>
        <button
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/40 hover:text-white/70 hover:bg-white/5 transition-colors"
          @click="$emit('close')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Tabs -->
      <div class="flex gap-1.5 px-4 pt-3 pb-1 shrink-0 overflow-x-auto scrollbar-hide">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="text-xs px-3 min-h-[44px] flex items-center justify-center rounded-md whitespace-nowrap transition-all duration-150"
          :class="activeTab === tab.id
            ? 'nav-tab-active'
            : 'text-white/40 hover:text-white/70 hover:bg-white/5'"
          @click="activeTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-6 space-y-4">
        <!-- Appearance -->
        <template v-if="activeTab === 'appearance'">
          <!-- M15.1 Accent colour -->
          <div class="space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Accent Colour</p>
            <div class="flex gap-2 flex-wrap">
              <button
                v-for="color in accentPresets"
                :key="color.value"
                class="w-8 h-8 rounded-lg border-2 transition-all"
                :style="{ backgroundColor: color.value }"
                :class="store.accentColor === color.value ? 'border-white/60 scale-110' : 'border-white/10'"
                :title="color.label"
                @click="store.accentColor = color.value"
              />
              <label class="w-8 h-8 rounded-lg border-2 border-white/10 flex items-center justify-center cursor-pointer overflow-hidden relative">
                <span class="text-xs text-white/30">+</span>
                <input
                  type="color"
                  :value="store.accentColor"
                  class="absolute inset-0 opacity-0 cursor-pointer"
                  @input="store.accentColor = ($event.target as HTMLInputElement).value"
                />
              </label>
            </div>
          </div>

          <!-- M15.2 Glass intensity -->
          <div class="space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Glass Intensity</p>
            <div class="flex gap-2">
              <button
                v-for="opt in glassOptions"
                :key="opt.value"
                class="flex-1 py-2 rounded-lg text-xs transition-all"
                :class="store.glassIntensity === opt.value
                  ? 'bg-accent/15 text-accent/80'
                  : 'bg-white/5 text-white/40 hover:text-white/60 hover:bg-white/10'"
                @click="store.glassIntensity = opt.value"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>

          <!-- M15.3 Font size -->
          <div class="space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Font Size</p>
            <div class="flex gap-2">
              <button
                v-for="opt in fontOptions"
                :key="opt.value"
                class="flex-1 py-2 rounded-lg text-xs transition-all"
                :class="store.fontSize === opt.value
                  ? 'bg-accent/15 text-accent/80'
                  : 'bg-white/5 text-white/40 hover:text-white/60 hover:bg-white/10'"
                @click="store.fontSize = opt.value"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>
        </template>

        <!-- Content -->
        <template v-else-if="activeTab === 'content'">
          <!-- M15.4 Content type visibility -->
          <div class="space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Content Types</p>
            <p class="text-xs text-white/30">Hidden types still extract but don't show in the panel</p>
            <div class="space-y-1">
              <label
                v-for="ct in contentTypes"
                :key="ct.tab"
                class="flex items-center gap-2 p-2 rounded-lg bg-white/[0.03] border border-white/5 cursor-pointer"
              >
                <input
                  type="checkbox"
                  :checked="store.isTabVisible(ct.tab)"
                  class="rounded accent-[#F7931A]"
                  @change="store.toggleTabVisibility(ct.tab)"
                />
                <span class="text-xs text-white/70">{{ ct.label }}</span>
              </label>
            </div>
          </div>
        </template>

        <!-- Shortcuts -->
        <template v-else-if="activeTab === 'shortcuts'">
          <!-- M15.5 Keyboard shortcuts -->
          <div class="space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Keyboard Shortcuts</p>
            <div class="space-y-1">
              <div
                v-for="(binding, action) in store.settings.shortcuts"
                :key="action"
                class="flex items-center justify-between p-2.5 rounded-lg bg-white/[0.03] border border-white/5"
              >
                <span class="text-xs text-white/60">{{ formatAction(action as string) }}</span>
                <button
                  class="text-xs px-2 py-1 rounded bg-white/5 text-white/40 hover:text-white/70 font-mono transition-colors"
                  :class="recordingAction === action ? 'bg-accent/15 text-accent/80' : ''"
                  @click="startRecording(action as string)"
                >
                  {{ recordingAction === action ? 'Press key...' : binding }}
                </button>
              </div>
            </div>
          </div>
        </template>

        <!-- Notifications -->
        <template v-else-if="activeTab === 'notifications'">
          <!-- M15.6 Push notifications -->
          <div class="space-y-3">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Notifications</p>
            <label class="flex items-center gap-3 p-3 rounded-lg bg-white/[0.03] border border-white/5 cursor-pointer">
              <input
                type="checkbox"
                :checked="store.notificationsEnabled"
                class="rounded accent-[#F7931A]"
                @change="toggleNotifications"
              />
              <div>
                <p class="text-xs text-white/70">Generation complete</p>
                <p class="text-xs text-white/30">Notify when AI finishes while tab is backgrounded</p>
              </div>
            </label>
            <p v-if="notificationStatus" class="text-xs" :class="notificationOk ? 'text-emerald-400/60' : 'text-yellow-400/60'">
              {{ notificationStatus }}
            </p>
          </div>
        </template>

        <!-- Storage -->
        <template v-else-if="activeTab === 'storage'">
          <!-- M15.7 Auto-archive -->
          <div class="space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Auto-Archive</p>
            <p class="text-xs text-white/30">Archive conversations older than:</p>
            <div class="flex gap-2">
              <button
                v-for="opt in archiveOptions"
                :key="opt.value"
                class="flex-1 py-2 rounded-lg text-xs transition-all"
                :class="store.autoArchiveDays === opt.value
                  ? 'bg-accent/15 text-accent/80'
                  : 'bg-white/5 text-white/40 hover:text-white/60 hover:bg-white/10'"
                @click="store.autoArchiveDays = opt.value"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>

          <!-- M15.8 Full data export -->
          <div class="space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Data Export</p>
            <button
              class="w-full py-2.5 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
              @click="exportData"
            >
              {{ isExporting ? 'Exporting...' : 'Export All Data' }}
            </button>
            <p class="text-xs text-white/20">Exports conversations, favorites, settings, tags, and collections as JSON</p>
          </div>

          <!-- M15.9 Data wipe -->
          <div class="space-y-2">
            <p class="text-xs text-red-400/60 uppercase tracking-wider font-bold">Danger Zone</p>
            <button
              v-if="!confirmWipe"
              class="w-full py-2.5 rounded-lg text-xs bg-red-400/10 text-red-400/60 hover:bg-red-400/20 hover:text-red-400/80 transition-colors"
              @click="confirmWipe = true"
            >
              Wipe All Data
            </button>
            <div v-else class="space-y-2 p-3 rounded-lg bg-red-400/5 border border-red-400/20">
              <p class="text-xs text-red-400/80 font-semibold">Are you sure?</p>
              <p class="text-xs text-red-400/50">This will clear all conversations, favorites, settings, and cached data.</p>
              <label class="flex items-center gap-2 text-xs text-red-400/60 cursor-pointer">
                <input v-model="wipeApiKeys" type="checkbox" class="rounded" />
                Also clear API key vault
              </label>
              <div class="flex gap-2">
                <button
                  class="flex-1 py-2 rounded-lg text-xs text-white/40 hover:text-white/70 hover:bg-white/5 transition-colors"
                  @click="confirmWipe = false"
                >
                  Cancel
                </button>
                <button
                  class="flex-1 py-2 rounded-lg text-xs bg-red-400/20 text-red-400/80 hover:bg-red-400/30 transition-colors"
                  @click="wipeData"
                >
                  Confirm Wipe
                </button>
              </div>
            </div>
          </div>
        </template>

        <!-- Chat defaults -->
        <template v-else-if="activeTab === 'chat'">
          <!-- Claude API Key Management -->
          <div class="space-y-3">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Claude API Authentication</p>

            <!-- Status indicator -->
            <div class="flex items-center gap-2 p-3 rounded-xl bg-white/[0.03] border border-white/5">
              <div
                class="w-2 h-2 rounded-full shrink-0"
                :class="store.settings.useOwnApiKey && store.claudeApiKey
                  ? 'bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.5)]'
                  : 'bg-amber-400 shadow-[0_0_6px_rgba(251,191,36,0.4)]'"
              />
              <span class="text-xs text-white/60">
                {{ store.settings.useOwnApiKey && store.claudeApiKey
                  ? 'Using your API key'
                  : 'Using server authentication (OAuth)' }}
              </span>
            </div>

            <!-- Toggle -->
            <label class="flex items-center gap-3 p-2.5 rounded-lg bg-white/[0.03] border border-white/5 cursor-pointer">
              <input
                v-model="store.settings.useOwnApiKey"
                type="checkbox"
                class="rounded accent-[#F7931A]"
              />
              <div>
                <span class="text-xs text-white/70">Use my own API key</span>
                <p class="text-xs text-white/25 mt-0.5">Override server OAuth with your personal Claude API key</p>
              </div>
            </label>

            <!-- API Key input (shown when toggle is on) -->
            <div v-if="store.settings.useOwnApiKey" class="space-y-2">
              <div class="flex gap-2">
                <input
                  :value="apiKeyDisplay"
                  :type="showApiKey ? 'text' : 'password'"
                  placeholder="sk-ant-api03-..."
                  class="flex-1 px-3 py-2 rounded-lg text-xs bg-white/5 text-white/70 placeholder:text-white/20 outline-none focus:bg-white/10 transition-colors font-mono"
                  style="font-size: 16px"
                  @input="onApiKeyInput"
                  @focus="onApiKeyFocus"
                  @blur="onApiKeyBlur"
                />
                <button
                  class="px-2 py-2 rounded-lg text-white/30 hover:text-white/60 hover:bg-white/5 transition-colors"
                  :title="showApiKey ? 'Hide' : 'Show'"
                  @click="showApiKey = !showApiKey"
                >
                  <svg v-if="!showApiKey" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
                  <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.542-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.542 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" /></svg>
                </button>
              </div>
              <button
                v-if="store.claudeApiKey"
                class="text-xs px-2 py-1 rounded-md text-red-400/50 hover:text-red-400 hover:bg-red-500/10 transition-colors"
                @click="store.setClaudeApiKey(''); apiKeyStatus = 'Key removed'"
              >
                Remove key
              </button>
              <p v-if="apiKeyStatus" class="text-xs" :class="apiKeyStatusOk ? 'text-emerald-400/70' : 'text-amber-400/70'">
                {{ apiKeyStatus }}
              </p>
            </div>

            <!-- Help text -->
            <div class="p-3 rounded-xl bg-white/[0.02] border border-white/5 space-y-1.5">
              <p class="text-xs text-white/40 font-medium">How to get a Claude API key</p>
              <ol class="text-xs text-white/25 space-y-1 list-decimal pl-4">
                <li>Go to console.anthropic.com and create an account</li>
                <li>Navigate to API Keys in your dashboard</li>
                <li>Create a new key (starts with sk-ant-api03-)</li>
                <li>Paste it above and enable "Use my own API key"</li>
              </ol>
              <p class="text-xs text-white/20 mt-2">Your key is stored encrypted on this device (AES-256-GCM) once you set an AIUI passphrase; without one it is held in memory for this session only — never written to disk unencrypted. Without a key, the server's OAuth authentication is used.</p>
            </div>
          </div>

          <!-- Default conversation settings -->
          <div class="space-y-3 mt-6 pt-4 border-t border-white/5">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Default Conversation Settings</p>
            <p class="text-xs text-white/30">Applied to all new conversations</p>

            <div class="space-y-2">
              <label class="block">
                <span class="text-xs text-white/40">Default Model</span>
                <input
                  v-model="store.settings.defaultModel"
                  type="text"
                  placeholder="e.g. claude-sonnet-4-6"
                  class="w-full mt-1 px-3 py-2 rounded-lg text-base bg-white/5 text-white/70 placeholder:text-white/20 outline-none focus:bg-white/10 transition-colors font-mono"
                />
              </label>

              <label class="flex items-center gap-3 p-2.5 rounded-lg bg-white/[0.03] border border-white/5 cursor-pointer">
                <input
                  v-model="store.settings.defaultWebSearch"
                  type="checkbox"
                  class="rounded accent-[#F7931A]"
                />
                <span class="text-xs text-white/60">Enable web search by default</span>
              </label>

              <label class="flex items-center gap-3 p-2.5 rounded-lg bg-white/[0.03] border border-white/5 cursor-pointer">
                <input
                  v-model="store.settings.defaultShowTokens"
                  type="checkbox"
                  class="rounded accent-[#F7931A]"
                />
                <span class="text-xs text-white/60">Show token counts by default</span>
              </label>
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import type { ContentTab } from '@/composables/contentFiltering'

defineEmits<{ close: [] }>()

type Tab = 'appearance' | 'content' | 'shortcuts' | 'notifications' | 'storage' | 'chat'

const tabs: { id: Tab; label: string }[] = [
  { id: 'appearance', label: 'Appearance' },
  { id: 'content', label: 'Content' },
  { id: 'shortcuts', label: 'Shortcuts' },
  { id: 'notifications', label: 'Notifications' },
  { id: 'storage', label: 'Storage' },
  { id: 'chat', label: 'Chat' },
]

const activeTab = ref<Tab>('appearance')
const store = useSettingsStore()

// Populate the key from the encrypted vault when a passphrase session is
// already active (standalone). Never touches localStorage (S2).
onMounted(() => { store.initClaudeKey() })

// M15.1 Accent presets
const accentPresets = [
  { value: '#F7931A', label: 'Bitcoin Orange' },
  { value: '#8B5CF6', label: 'Purple' },
  { value: '#3B82F6', label: 'Blue' },
  { value: '#10B981', label: 'Emerald' },
  { value: '#EF4444', label: 'Red' },
  { value: '#EC4899', label: 'Pink' },
  { value: '#F59E0B', label: 'Amber' },
  { value: '#06B6D4', label: 'Cyan' },
]

// M15.2 Glass options
const glassOptions: { value: 'subtle' | 'default' | 'strong'; label: string }[] = [
  { value: 'subtle', label: 'Subtle' },
  { value: 'default', label: 'Default' },
  { value: 'strong', label: 'Strong' },
]

// M15.3 Font options
const fontOptions: { value: 'compact' | 'default' | 'large'; label: string }[] = [
  { value: 'compact', label: 'Compact (13px)' },
  { value: 'default', label: 'Default (15px)' },
  { value: 'large', label: 'Large (17px)' },
]

// M15.4 Content types
const contentTypes: { tab: ContentTab; label: string }[] = [
  { tab: 'film', label: 'Films' },
  { tab: 'song', label: 'Music' },
  { tab: 'podcast', label: 'Podcasts' },
  { tab: 'book', label: 'Books' },
  { tab: 'tvshow', label: 'TV Series' },
  { tab: 'image', label: 'Images' },
  { tab: 'place', label: 'Places' },
  { tab: 'news', label: 'News' },
  { tab: 'websites', label: 'Websites' },
  { tab: 'magazine', label: 'Brief' },
  { tab: 'nostr', label: 'Nostr' },
]

// M15.5 Shortcuts
const recordingAction = ref<string | null>(null)

function formatAction(action: string): string {
  return action.replace(/-/g, ' ').replace(/\b\w/g, l => l.toUpperCase())
}

function startRecording(action: string) {
  recordingAction.value = action
  const handler = (e: KeyboardEvent) => {
    e.preventDefault()
    const parts: string[] = []
    if (e.metaKey || e.ctrlKey) parts.push('Cmd')
    if (e.shiftKey) parts.push('Shift')
    if (e.altKey) parts.push('Alt')
    if (e.key !== 'Meta' && e.key !== 'Control' && e.key !== 'Shift' && e.key !== 'Alt') {
      parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key)
    }
    if (parts.length > 0) {
      store.setShortcut(action, parts.join('+'))
    }
    recordingAction.value = null
    document.removeEventListener('keydown', handler)
  }
  document.addEventListener('keydown', handler)
}

// M15.6 Notifications
const notificationStatus = ref('')
const notificationOk = ref(false)

async function toggleNotifications() {
  if (!store.notificationsEnabled) {
    // Enable
    if (!('Notification' in window)) {
      notificationStatus.value = 'Notifications not supported in this browser'
      notificationOk.value = false
      return
    }
    const permission = await Notification.requestPermission()
    if (permission === 'granted') {
      store.notificationsEnabled = true
      notificationStatus.value = 'Notifications enabled'
      notificationOk.value = true
    } else {
      notificationStatus.value = 'Permission denied'
      notificationOk.value = false
    }
  } else {
    store.notificationsEnabled = false
    notificationStatus.value = ''
  }
}

// M15.7 Archive options
const archiveOptions = [
  { value: 0, label: 'Never' },
  { value: 7, label: '7 days' },
  { value: 30, label: '30 days' },
  { value: 90, label: '90 days' },
]

// M15.8 Export
const isExporting = ref(false)

async function exportData() {
  isExporting.value = true
  try {
    const data: Record<string, unknown> = {}

    // Gather all localStorage items
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i)
      if (key?.startsWith('aiui-')) {
        try { data[key] = JSON.parse(localStorage.getItem(key) ?? '') }
        catch { data[key] = localStorage.getItem(key) }
      }
    }

    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `aiui-export-${new Date().toISOString().slice(0, 10)}.json`
    a.click()
    URL.revokeObjectURL(url)
  } finally {
    isExporting.value = false
  }
}

// API key management
const showApiKey = ref(false)
const apiKeyEditing = ref(false)
const apiKeyStatus = ref('')
const apiKeyStatusOk = ref(true)

const apiKeyDisplay = computed(() => {
  if (apiKeyEditing.value) return store.claudeApiKey
  if (!store.claudeApiKey) return ''
  if (showApiKey.value) return store.claudeApiKey
  const k = store.claudeApiKey
  return k.length > 8 ? k.slice(0, 10) + '...' + k.slice(-4) : '****'
})

async function onApiKeyInput(e: Event) {
  const val = (e.target as HTMLInputElement).value
  const held = await store.setClaudeApiKey(val)
  if (val && val.startsWith('sk-ant-api')) {
    apiKeyStatus.value = held === 'encrypted'
      ? 'Key saved — stored encrypted'
      : 'Key saved for this session only — set a passphrase to store it encrypted'
    apiKeyStatusOk.value = true
  } else if (val && !val.startsWith('sk-ant-')) {
    apiKeyStatus.value = 'Key should start with sk-ant-api03-'
    apiKeyStatusOk.value = false
  } else {
    apiKeyStatus.value = ''
  }
}

function onApiKeyFocus() {
  apiKeyEditing.value = true
}

function onApiKeyBlur() {
  apiKeyEditing.value = false
}

// M15.9 Data wipe
const confirmWipe = ref(false)
const wipeApiKeys = ref(false)

async function wipeData() {
  // Clear localStorage (except API keys if unchecked)
  const keysToKeep: string[] = []
  if (!wipeApiKeys.value) {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i)
      if (key?.includes('api-key') || key?.includes('vault')) {
        keysToKeep.push(key)
      }
    }
  }

  const preserved = new Map<string, string>()
  for (const key of keysToKeep) {
    preserved.set(key, localStorage.getItem(key) ?? '')
  }

  localStorage.clear()

  for (const [key, value] of preserved) {
    localStorage.setItem(key, value)
  }

  // Clear IDB
  const dbs = await indexedDB.databases()
  for (const db of dbs) {
    if (db.name) indexedDB.deleteDatabase(db.name)
  }

  // Clear SW cache
  if ('caches' in window) {
    const names = await caches.keys()
    for (const name of names) {
      await caches.delete(name)
    }
  }

  window.location.reload()
}
</script>
