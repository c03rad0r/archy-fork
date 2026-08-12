import { ref, computed } from 'vue'
import { defineStore } from 'pinia'

export type PluginPermission = 'chat-messages' | 'network' | 'favorites' | 'storage' | 'nostr' | 'wallet'

export interface RegistryPlugin {
  id: string
  name: string
  description: string
  type: string
  author: string
  version: string
  rating: number
  url: string
  permissions: PluginPermission[]
  changelog?: string
  settingsSchema?: Record<string, unknown>
}

export interface InstalledPlugin {
  id: string
  name: string
  version: string
  type: string
  author: string
  url: string
  permissions: PluginPermission[]
  grantedPermissions: PluginPermission[]
  settings: Record<string, unknown>
  installedAt: number
  updatedAt: number
}

const STORAGE_KEY = 'aiui-installed-plugins'
const REGISTRY_URL = 'https://raw.githubusercontent.com/aiui-app/plugin-registry/main/registry.json'

export const usePluginMarketplaceStore = defineStore('pluginMarketplace', () => {
  const registryPlugins = ref<RegistryPlugin[]>([])
  const installedPlugins = ref<InstalledPlugin[]>([])
  const isLoadingRegistry = ref(false)
  const registryError = ref('')
  const updatesAvailable = ref<Map<string, string>>(new Map())

  // Load installed plugins from localStorage
  function loadInstalled() {
    try {
      const stored = localStorage.getItem(STORAGE_KEY)
      if (stored) installedPlugins.value = JSON.parse(stored)
    } catch { /* ignore */ }
  }

  function saveInstalled() {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(installedPlugins.value))
  }

  loadInstalled()

  const hasUpdates = computed(() => updatesAvailable.value.size > 0)

  async function fetchRegistry() {
    isLoadingRegistry.value = true
    registryError.value = ''
    try {
      const res = await fetch(REGISTRY_URL)
      if (!res.ok) throw new Error('Failed to fetch registry')
      const data = await res.json()
      registryPlugins.value = data.plugins ?? data ?? []
    } catch (e) {
      registryError.value = e instanceof Error ? e.message : 'Failed to load registry'
      // Provide built-in fallback entries
      registryPlugins.value = getBuiltinRegistry()
    } finally {
      isLoadingRegistry.value = false
    }
  }

  function isInstalled(pluginId: string): boolean {
    return installedPlugins.value.some(p => p.id === pluginId)
  }

  function installPlugin(plugin: RegistryPlugin, grantedPermissions: PluginPermission[]) {
    if (isInstalled(plugin.id)) return

    installedPlugins.value.push({
      id: plugin.id,
      name: plugin.name,
      version: plugin.version,
      type: plugin.type,
      author: plugin.author,
      url: plugin.url,
      permissions: plugin.permissions,
      grantedPermissions,
      settings: {},
      installedAt: Date.now(),
      updatedAt: Date.now(),
    })
    saveInstalled()
  }

  function uninstallPlugin(pluginId: string) {
    installedPlugins.value = installedPlugins.value.filter(p => p.id !== pluginId)
    updatesAvailable.value.delete(pluginId)
    saveInstalled()
  }

  function updatePluginSettings(pluginId: string, settings: Record<string, unknown>) {
    const plugin = installedPlugins.value.find(p => p.id === pluginId)
    if (plugin) {
      plugin.settings = settings
      saveInstalled()
    }
  }

  function updatePluginPermissions(pluginId: string, permissions: PluginPermission[]) {
    const plugin = installedPlugins.value.find(p => p.id === pluginId)
    if (plugin) {
      plugin.grantedPermissions = permissions
      saveInstalled()
    }
  }

  function checkForUpdates() {
    updatesAvailable.value.clear()
    for (const installed of installedPlugins.value) {
      const registry = registryPlugins.value.find(r => r.id === installed.id)
      if (registry && registry.version !== installed.version) {
        updatesAvailable.value.set(installed.id, registry.version)
      }
    }
  }

  function updatePlugin(pluginId: string) {
    const registry = registryPlugins.value.find(r => r.id === pluginId)
    const installed = installedPlugins.value.find(p => p.id === pluginId)
    if (!registry || !installed) return

    installed.version = registry.version
    installed.updatedAt = Date.now()
    updatesAvailable.value.delete(pluginId)
    saveInstalled()
  }

  function updateAllPlugins() {
    for (const [id] of updatesAvailable.value) {
      updatePlugin(id)
    }
  }

  async function importFromUrl(url: string): Promise<RegistryPlugin | null> {
    try {
      const manifestUrl = url.endsWith('/') ? `${url}aiui-plugin.json` : url
      const res = await fetch(manifestUrl)
      if (!res.ok) throw new Error('Failed to fetch manifest')
      const manifest = await res.json()

      if (!manifest.id || !manifest.name || !manifest.version) {
        throw new Error('Invalid plugin manifest')
      }

      return {
        id: manifest.id,
        name: manifest.name,
        description: manifest.description ?? '',
        type: manifest.type ?? 'search',
        author: manifest.author ?? 'Unknown',
        version: manifest.version,
        rating: 0,
        url,
        permissions: manifest.permissions ?? [],
        settingsSchema: manifest.settingsSchema,
      }
    } catch {
      return null
    }
  }

  function hasPermission(pluginId: string, permission: PluginPermission): boolean {
    const plugin = installedPlugins.value.find(p => p.id === pluginId)
    return plugin?.grantedPermissions.includes(permission) ?? false
  }

  return {
    registryPlugins,
    installedPlugins,
    isLoadingRegistry,
    registryError,
    updatesAvailable,
    hasUpdates,
    fetchRegistry,
    isInstalled,
    installPlugin,
    uninstallPlugin,
    updatePluginSettings,
    updatePluginPermissions,
    checkForUpdates,
    updatePlugin,
    updateAllPlugins,
    importFromUrl,
    hasPermission,
  }
})

function getBuiltinRegistry(): RegistryPlugin[] {
  return [
    {
      id: 'wikipedia',
      name: 'Wikipedia',
      description: 'Search Wikipedia articles with /wiki command',
      type: 'search',
      author: 'AIUI',
      version: '1.0.0',
      rating: 5,
      url: 'builtin:wikipedia',
      permissions: ['network'],
    },
    {
      id: 'openlibrary',
      name: 'Open Library',
      description: 'Search books from Open Library with /book command',
      type: 'search',
      author: 'AIUI',
      version: '1.0.0',
      rating: 5,
      url: 'builtin:openlibrary',
      permissions: ['network'],
    },
  ]
}
