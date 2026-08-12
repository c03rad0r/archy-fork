<template>
  <div class="h-full flex flex-col">
    <div class="p-4 border-b border-white/[0.08]">
      <div class="flex items-center justify-between mb-3">
        <h3 class="text-sm font-bold text-white/90">Plugins</h3>
        <div class="flex items-center gap-2">
          <span v-if="store.hasUpdates" class="text-xs px-1.5 py-0.5 rounded-full bg-accent/20 text-accent/80">
            Updates
          </span>
        </div>
      </div>

      <div class="flex gap-1.5 flex-wrap">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="text-xs px-2 py-1 rounded-md transition-all duration-150"
          :class="activeTab === tab.id
            ? 'nav-tab-active'
            : 'text-white/40 hover:text-white/70 hover:bg-white/5'"
          @click="activeTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-3">
      <!-- Discover -->
      <template v-if="activeTab === 'discover'">
        <div v-if="store.isLoadingRegistry" class="flex items-center justify-center py-12">
          <p class="text-xs text-white/30">Loading registry...</p>
        </div>

        <div v-else-if="store.registryError && store.registryPlugins.length === 0" class="py-8 text-center">
          <p class="text-xs text-red-400/60">{{ store.registryError }}</p>
          <button class="mt-2 text-xs text-accent/60 hover:text-accent/80" @click="store.fetchRegistry()">Retry</button>
        </div>

        <div
          v-for="plugin in store.registryPlugins"
          :key="plugin.id"
          class="rounded-xl bg-white/[0.03] border border-white/5 p-3 space-y-2"
        >
          <div class="flex items-center justify-between">
            <div class="flex-1 min-w-0">
              <p class="text-xs font-semibold text-white/80">{{ plugin.name }}</p>
              <p class="text-xs text-white/30">{{ plugin.author }} · v{{ plugin.version }}</p>
            </div>
            <button
              v-if="!store.isInstalled(plugin.id)"
              class="text-xs px-3 py-1.5 rounded-lg bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors shrink-0"
              @click="showPermissionsDialog(plugin)"
            >
              Install
            </button>
            <span v-else class="text-xs text-emerald-400/60 shrink-0">Installed</span>
          </div>
          <p class="text-xs text-white/50">{{ plugin.description }}</p>
          <div class="flex items-center gap-2">
            <span class="text-xs px-1.5 py-0.5 rounded bg-white/5 text-white/30">{{ plugin.type }}</span>
            <div class="flex items-center gap-0.5">
              <span v-for="i in 5" :key="i" class="text-xs" :class="i <= plugin.rating ? 'text-accent/60' : 'text-white/10'">★</span>
            </div>
          </div>
        </div>
      </template>

      <!-- Installed -->
      <template v-else-if="activeTab === 'installed'">
        <div v-if="store.installedPlugins.length === 0" class="flex flex-col items-center justify-center py-12 gap-2">
          <svg class="w-8 h-8 text-white/10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
          </svg>
          <p class="text-xs text-white/30">No plugins installed</p>
        </div>

        <div
          v-for="plugin in store.installedPlugins"
          :key="plugin.id"
          class="rounded-xl bg-white/[0.03] border border-white/5 p-3 space-y-2"
        >
          <div class="flex items-center justify-between">
            <div class="flex-1 min-w-0">
              <p class="text-xs font-semibold text-white/80">{{ plugin.name }}</p>
              <p class="text-xs text-white/30">v{{ plugin.version }} · {{ plugin.author }}</p>
            </div>
            <div class="flex items-center gap-1.5 shrink-0">
              <button
                v-if="store.updatesAvailable.has(plugin.id)"
                class="text-xs px-2 py-1 rounded bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
                @click="store.updatePlugin(plugin.id)"
              >
                Update to v{{ store.updatesAvailable.get(plugin.id) }}
              </button>
              <button
                class="text-xs min-w-[44px] min-h-[44px] flex items-center justify-center rounded text-white/30 hover:text-white/60 hover:bg-white/5 transition-colors"
                @click="editingPlugin = editingPlugin === plugin.id ? null : plugin.id"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
              </button>
              <button
                class="text-xs px-2 py-1 rounded text-red-400/50 hover:text-red-400/80 hover:bg-red-400/10 transition-colors"
                @click="store.uninstallPlugin(plugin.id)"
              >
                Remove
              </button>
            </div>
          </div>

          <!-- Permissions -->
          <div class="flex flex-wrap gap-1">
            <span
              v-for="perm in plugin.permissions"
              :key="perm"
              class="text-xs px-1.5 py-0.5 rounded"
              :class="plugin.grantedPermissions.includes(perm)
                ? 'bg-emerald-400/15 text-emerald-400/60'
                : 'bg-red-400/15 text-red-400/60'"
            >
              {{ permissionLabel(perm) }}
            </span>
          </div>

          <!-- Settings panel (inline) -->
          <PluginSettingsForm
            v-if="editingPlugin === plugin.id"
            :plugin-id="plugin.id"
            :settings="plugin.settings"
            :permissions="plugin.permissions"
            :granted-permissions="plugin.grantedPermissions"
            @update-settings="(s) => store.updatePluginSettings(plugin.id, s)"
            @update-permissions="(p) => store.updatePluginPermissions(plugin.id, p)"
          />
        </div>

        <!-- Update all -->
        <button
          v-if="store.hasUpdates"
          class="w-full py-2.5 rounded-lg text-xs font-medium bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors mt-2"
          @click="store.updateAllPlugins()"
        >
          Update all plugins
        </button>
      </template>

      <!-- Import -->
      <template v-else-if="activeTab === 'import'">
        <div class="space-y-3">
          <p class="text-xs text-white/40">
            Paste a GitHub raw URL or IPFS CID to the plugin's manifest (aiui-plugin.json).
          </p>
          <input
            v-model="importUrl"
            type="text"
            placeholder="https://raw.githubusercontent.com/.../aiui-plugin.json"
            class="w-full px-3 py-2.5 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
          />
          <button
            class="w-full py-2.5 rounded-lg text-xs font-medium bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
            :disabled="!importUrl.trim() || isImporting"
            @click="importPlugin"
          >
            {{ isImporting ? 'Fetching manifest...' : 'Import Plugin' }}
          </button>
          <p v-if="importError" class="text-xs text-red-400/60">{{ importError }}</p>

          <!-- Imported plugin preview -->
          <div v-if="importedManifest" class="rounded-xl bg-white/[0.03] border border-white/5 p-3 space-y-2">
            <p class="text-xs font-semibold text-white/80">{{ importedManifest.name }}</p>
            <p class="text-xs text-white/50">{{ importedManifest.description }}</p>
            <p class="text-xs text-white/30">{{ importedManifest.author }} · v{{ importedManifest.version }}</p>
            <button
              class="w-full py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
              @click="showPermissionsDialog(importedManifest)"
            >
              Install
            </button>
          </div>
        </div>
      </template>

      <!-- Dev Mode -->
      <template v-else-if="activeTab === 'dev'">
        <div v-if="!isDevMode" class="flex flex-col items-center justify-center py-12 gap-2">
          <p class="text-xs text-white/30">Dev mode not enabled</p>
          <p class="text-xs text-white/20">Set VITE_PLUGIN_DEV=true to enable</p>
        </div>

        <template v-else>
          <div class="rounded-xl bg-white/[0.03] border border-white/5 p-3 space-y-2">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Plugin Dev Mode</p>
            <p class="text-xs text-white/40">Hot-reload from src/plugins/dev/</p>

            <div v-if="devErrors.length > 0" class="space-y-1 mt-2">
              <p class="text-xs text-red-400/60 uppercase tracking-wider font-bold">Errors</p>
              <div
                v-for="(err, i) in devErrors"
                :key="i"
                class="text-xs text-red-400/50 font-mono bg-red-400/5 rounded p-2"
              >
                {{ err }}
              </div>
            </div>

            <div v-if="devTimings.length > 0" class="space-y-1 mt-2">
              <p class="text-xs text-white/30 uppercase tracking-wider font-bold">Init Timing</p>
              <div
                v-for="t in devTimings"
                :key="t.id"
                class="flex items-center justify-between text-xs"
              >
                <span class="text-white/50">{{ t.id }}</span>
                <span class="text-white/30 tabular-nums">{{ t.ms }}ms</span>
              </div>
            </div>
          </div>
        </template>
      </template>
    </div>

    <!-- Permissions dialog -->
    <Teleport to="body">
      <div v-if="pendingInstall" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
        <div class="w-full max-w-sm mx-4 rounded-2xl bg-[#0a0a0a] border border-white/10 p-5 space-y-4">
          <h4 class="text-sm font-bold text-white/90">Plugin Permissions</h4>
          <p class="text-xs text-white/40">
            "{{ pendingInstall.name }}" requests the following permissions:
          </p>
          <div class="space-y-2">
            <label
              v-for="perm in pendingInstall.permissions"
              :key="perm"
              class="flex items-center gap-2 p-2 rounded-lg bg-white/[0.03] border border-white/5 cursor-pointer"
            >
              <input
                type="checkbox"
                :checked="pendingPermissions.includes(perm)"
                class="rounded accent-[#F7931A]"
                @change="togglePermission(perm)"
              />
              <div>
                <p class="text-xs text-white/70">{{ permissionLabel(perm) }}</p>
                <p class="text-xs text-white/30">{{ permissionDescription(perm) }}</p>
              </div>
            </label>
          </div>
          <div class="flex gap-2">
            <button
              class="flex-1 py-2 rounded-lg text-xs text-white/40 hover:text-white/70 hover:bg-white/5 transition-colors"
              @click="pendingInstall = null"
            >
              Cancel
            </button>
            <button
              class="flex-1 py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
              @click="confirmInstall"
            >
              Install
            </button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { usePluginMarketplaceStore, type RegistryPlugin, type PluginPermission } from '@/stores/pluginMarketplace'
import PluginSettingsForm from './PluginSettingsForm.vue'

type Tab = 'discover' | 'installed' | 'import' | 'dev'

const tabs: { id: Tab; label: string }[] = [
  { id: 'discover', label: 'Discover' },
  { id: 'installed', label: 'Installed' },
  { id: 'import', label: 'Import' },
  { id: 'dev', label: 'Dev' },
]

const activeTab = ref<Tab>('discover')
const store = usePluginMarketplaceStore()
const editingPlugin = ref<string | null>(null)

// Import
const importUrl = ref('')
const isImporting = ref(false)
const importError = ref('')
const importedManifest = ref<RegistryPlugin | null>(null)

// Permissions dialog
const pendingInstall = ref<RegistryPlugin | null>(null)
const pendingPermissions = ref<PluginPermission[]>([])

// Dev mode
const isDevMode = !!import.meta.env.VITE_PLUGIN_DEV
const devErrors = ref<string[]>([])
const devTimings = ref<{ id: string; ms: number }[]>([])

function showPermissionsDialog(plugin: RegistryPlugin) {
  pendingInstall.value = plugin
  pendingPermissions.value = [...plugin.permissions]
}

function togglePermission(perm: PluginPermission) {
  const idx = pendingPermissions.value.indexOf(perm)
  if (idx >= 0) pendingPermissions.value.splice(idx, 1)
  else pendingPermissions.value.push(perm)
}

function confirmInstall() {
  if (!pendingInstall.value) return
  store.installPlugin(pendingInstall.value, pendingPermissions.value)
  pendingInstall.value = null
  importedManifest.value = null
}

async function importPlugin() {
  isImporting.value = true
  importError.value = ''
  importedManifest.value = null

  const manifest = await store.importFromUrl(importUrl.value.trim())
  if (manifest) {
    importedManifest.value = manifest
  } else {
    importError.value = 'Invalid manifest or failed to fetch'
  }
  isImporting.value = false
}

function permissionLabel(perm: PluginPermission): string {
  const labels: Record<PluginPermission, string> = {
    'chat-messages': 'Chat Messages',
    'network': 'Network Access',
    'favorites': 'Favorites',
    'storage': 'Local Storage',
    'nostr': 'Nostr Identity',
    'wallet': 'Wallet',
  }
  return labels[perm] ?? perm
}

function permissionDescription(perm: PluginPermission): string {
  const descs: Record<PluginPermission, string> = {
    'chat-messages': 'Read and inject content into chat messages',
    'network': 'Make network requests to external APIs',
    'favorites': 'Read and modify your favorites list',
    'storage': 'Store data in local storage',
    'nostr': 'Access your Nostr identity for signing',
    'wallet': 'Interact with your connected wallet',
  }
  return descs[perm] ?? ''
}

onMounted(() => {
  store.fetchRegistry().then(() => store.checkForUpdates())
})
</script>
