// Pinia store for container management
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { containerClient, type ContainerStatus } from '@/api/container-client'

// Bundled apps that come pre-loaded with Archipelago
export interface BundledApp {
  id: string
  name: string
  image: string
  description: string
  icon: string
  ports: { host: number; container: number }[]
  volumes: { host: string; container: string }[]
  category: 'bitcoin' | 'lightning' | 'home' | 'other'
  lan_address?: string // Runtime launch URL from backend
}

/** Map bundled app ID to the podman container name(s) used for status matching.
 *  Some apps have a different container name than their app ID, or use a
 *  separate UI container (e.g., bitcoin-knots node → bitcoin-ui web container). */
const CONTAINER_NAME_MAP: Record<string, string[]> = {
  'bitcoin-knots': ['bitcoin-knots', 'bitcoin-ui'],
  'lnd': ['lnd', 'archy-lnd-ui'],
  'btcpay-server': ['btcpay-server'],
  'mempool': ['mempool', 'archy-mempool-web'],
  'electrumx': ['archy-electrs-ui', 'electrumx', 'mempool-electrs'],
}

export const BUNDLED_APPS: BundledApp[] = [
  {
    id: 'bitcoin-knots',
    name: 'Bitcoin Knots',
    image: 'source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest',
    description: 'Full Bitcoin node with additional features',
    icon: '₿',
    ports: [{ host: 8334, container: 80 }],
    volumes: [{ host: '/var/lib/archipelago/bitcoin', container: '/data' }],
    category: 'bitcoin',
  },
  {
    id: 'lnd',
    name: 'Lightning (LND)',
    image: 'docker.io/lightninglabs/lnd:v0.18.4-beta',
    description: 'Lightning Network Daemon for fast Bitcoin payments',
    icon: '⚡',
    ports: [{ host: 18083, container: 80 }],
    volumes: [{ host: '/var/lib/archipelago/lnd', container: '/root/.lnd' }],
    category: 'lightning',
  },
  {
    id: 'homeassistant',
    name: 'Home Assistant',
    image: 'ghcr.io/home-assistant/home-assistant:stable',
    description: 'Open source home automation platform',
    icon: '🏠',
    ports: [{ host: 8123, container: 8123 }],
    volumes: [{ host: '/var/lib/archipelago/homeassistant', container: '/config' }],
    category: 'home',
  },
  {
    id: 'btcpay-server',
    name: 'BTCPay Server',
    image: 'docker.io/btcpayserver/btcpayserver:latest',
    description: 'Self-hosted Bitcoin payment processor',
    icon: '💳',
    ports: [{ host: 23000, container: 49392 }],
    volumes: [{ host: '/var/lib/archipelago/btcpay', container: '/datadir' }],
    category: 'bitcoin',
  },
  {
    id: 'mempool',
    name: 'Mempool Explorer',
    image: 'docker.io/mempool/frontend:latest',
    description: 'Bitcoin blockchain and mempool visualizer',
    icon: '🔍',
    ports: [{ host: 4080, container: 8080 }],
    volumes: [{ host: '/var/lib/archipelago/mempool', container: '/data' }],
    category: 'bitcoin',
  },
  {
    id: 'tailscale',
    name: 'Tailscale VPN',
    image: 'docker.io/tailscale/tailscale:latest',
    description: 'Zero-config VPN mesh network',
    icon: '🔒',
    ports: [],
    volumes: [{ host: '/var/lib/archipelago/tailscale', container: '/var/lib/tailscale' }],
    category: 'other',
  },
]

export const useContainerStore = defineStore('container', () => {
  // State
  const containers = ref<ContainerStatus[]>([])
  const healthStatus = ref<Record<string, string>>({})
  const loading = ref(false)
  /** Whether the container list has been successfully fetched at least
   * once. Without this, an empty list is ambiguous — see `ensureFetched`. */
  const fetched = ref(false)
  let inFlightFetch: Promise<void> | null = null
  const loadingApps = ref<Set<string>>(new Set()) // Track loading state per app
  const error = ref<string | null>(null)

  // Getters
  const runningContainers = computed(() => 
    containers.value.filter(c => c.state === 'running')
  )

  const stoppedContainers = computed(() => 
    containers.value.filter(c => c.state === 'stopped' || c.state === 'exited')
  )

  const getContainerById = computed(() => (id: string) => 
    containers.value.find(c => c.name.includes(id))
  )

  const getHealthStatus = computed(() => (appId: string) => 
    healthStatus.value[appId] || 'unknown'
  )

  // Get container for a bundled app (matches by explicit name map, then by exact name)
  const getContainerForApp = computed(() => (appId: string) => {
    const nameList = CONTAINER_NAME_MAP[appId]
    if (nameList) {
      // Try each known container name in priority order
      for (const n of nameList) {
        const found = containers.value.find(c => c.name === n)
        if (found) return found
      }
    }
    // Fallback: exact match on app ID
    return containers.value.find(c => c.name === appId)
  })

  // Check if an app is currently loading (starting/stopping)
  const isAppLoading = computed(() => (appId: string) => 
    loadingApps.value.has(appId)
  )

  // Get app state: 'running', 'stopped', 'not-installed'
  const getAppState = computed(() => (appId: string) => {
    const container = getContainerForApp.value(appId)
    if (!container) return 'not-installed'
    return container.state
  })

  // Get visual state for UI — collapses backend states into the small set the
  // single-button component needs. Transitional states (stopping/starting/
  // restarting/installing/updating/removing) pass through; resting states
  // (exited/created/paused/installed) collapse to 'stopped' or 'running' so
  // the button logic can stay simple.
  type VisualState =
    | 'not-installed'
    | 'running'
    | 'stopped'
    | 'stopping'
    | 'starting'
    | 'restarting'
    | 'installing'
    | 'updating'
    | 'removing'
    | 'unknown'
  const getAppVisualState = computed(() => (appId: string): VisualState => {
    const container = getContainerForApp.value(appId)
    if (!container) return 'not-installed'
    switch (container.state) {
      case 'running':
        return 'running'
      case 'stopped':
      case 'exited':
      case 'created':
      case 'paused':
      case 'installed':
        return 'stopped'
      case 'stopping':
      case 'starting':
      case 'restarting':
      case 'installing':
      case 'updating':
      case 'removing':
        return container.state
      default:
        return 'unknown'
    }
  })

  // Get enriched bundled apps with runtime data (like lan_address)
  const enrichedBundledApps = computed(() => {
    return BUNDLED_APPS.map(app => {
      const container = getContainerForApp.value(app.id)
      return {
        ...app,
        lan_address: container?.lan_address
      }
    })
  })

  // Actions
  async function fetchContainers() {
    loading.value = true
    error.value = null
    try {
      containers.value = await containerClient.listContainers()
      fetched.value = true
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to fetch containers'
      if (import.meta.env.DEV) console.error('Failed to fetch containers:', e)
    } finally {
      loading.value = false
    }
  }

  /**
   * Resolve the container list ONCE before a decision that depends on
   * whether an app exists. `getAppState` cannot distinguish "not installed"
   * from "not fetched yet" — both look like an empty list — so any caller
   * that would take a DIFFERENT, user-visible path on "not installed"
   * (tx links falling back to a third-party explorer, for one) must await
   * this first. Concurrent callers share the one in-flight request.
   */
  async function ensureFetched(): Promise<void> {
    if (fetched.value) return
    if (!inFlightFetch) {
      inFlightFetch = fetchContainers().finally(() => {
        inFlightFetch = null
      })
    }
    await inFlightFetch
  }

  async function fetchHealthStatus() {
    try {
      healthStatus.value = await containerClient.getHealthStatus()
    } catch (e) {
      if (import.meta.env.DEV) console.error('Failed to fetch health status:', e)
    }
  }

  async function installApp(manifestPath: string) {
    loading.value = true
    error.value = null
    try {
      const containerName = await containerClient.installApp(manifestPath)
      await fetchContainers()
      return containerName
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to install app'
      throw e
    } finally {
      loading.value = false
    }
  }

  async function startContainer(appId: string) {
    loadingApps.value.add(appId)
    error.value = null
    try {
      await containerClient.startContainer(appId)
      await fetchContainers()
      await fetchHealthStatus()
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to start container'
      throw e
    } finally {
      loadingApps.value.delete(appId)
    }
  }

  async function stopContainer(appId: string) {
    loadingApps.value.add(appId)
    error.value = null
    try {
      await containerClient.stopContainer(appId)
      await fetchContainers()
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to stop container'
      throw e
    } finally {
      loadingApps.value.delete(appId)
    }
  }

  async function restartContainer(appId: string) {
    loadingApps.value.add(appId)
    error.value = null
    try {
      await containerClient.restartContainer(appId)
      await fetchContainers()
      await fetchHealthStatus()
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to restart container'
      throw e
    } finally {
      loadingApps.value.delete(appId)
    }
  }

  // Start a bundled app (creates and starts container)
  async function startBundledApp(app: BundledApp) {
    loadingApps.value.add(app.id)
    error.value = null
    try {
      await containerClient.startBundledApp(app)
      await fetchContainers()
      await fetchHealthStatus()
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to start app'
      throw e
    } finally {
      loadingApps.value.delete(app.id)
    }
  }

  // Stop a bundled app
  async function stopBundledApp(appId: string) {
    loadingApps.value.add(appId)
    error.value = null
    try {
      await containerClient.stopBundledApp(appId)
      await fetchContainers()
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to stop app'
      throw e
    } finally {
      loadingApps.value.delete(appId)
    }
  }

  async function removeContainer(appId: string) {
    loading.value = true
    error.value = null
    try {
      await containerClient.removeContainer(appId)
      await fetchContainers()
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to remove container'
      throw e
    } finally {
      loading.value = false
    }
  }

  async function getContainerLogs(appId: string, lines: number = 100) {
    try {
      return await containerClient.getContainerLogs(appId, lines)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to get logs'
      throw e
    }
  }

  async function getContainerStatus(appId: string) {
    try {
      return await containerClient.getContainerStatus(appId)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to get status'
      throw e
    }
  }

  return {
    // State
    containers,
    healthStatus,
    loading,
    loadingApps,
    error,
    // Getters
    runningContainers,
    stoppedContainers,
    getContainerById,
    getHealthStatus,
    getContainerForApp,
    isAppLoading,
    getAppState,
    getAppVisualState,
    enrichedBundledApps,
    // Actions
    fetched,
    ensureFetched,
    fetchContainers,
    fetchHealthStatus,
    installApp,
    startContainer,
    stopContainer,
    restartContainer,
    removeContainer,
    getContainerLogs,
    getContainerStatus,
    startBundledApp,
    stopBundledApp,
  }
})
