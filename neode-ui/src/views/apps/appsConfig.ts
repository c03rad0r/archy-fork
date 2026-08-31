/** Static configuration for the Apps view */

import type { Ref } from 'vue'
import { computed } from 'vue'
import { PackageState, type PackageDataEntry } from '@/types/api'
import { matchPageScheme, resolveAppUrl } from '../appSession/appSessionConfig'
import { portIsGateFronted } from '../discover/curatedApps'
import { isAutoTabApp } from '@/utils/autoTabApps'

export type AppsTab = 'apps' | 'websites' | 'services'

// Service container name patterns (backend/infra, not user-facing)
export const SERVICE_NAMES = new Set([
  'dwn', 'archy-mempool-db', 'archy-btcpay-db', 'archy-nbxplorer', 'archy-tor',
  // Headless backends with no user-facing UI: the Fedimint ecash client daemon,
  // the Nostr relay, and the Meshtastic LoRa daemon (its chat UI lives in the
  // built-in Mesh tab) belong in Services, not My Apps.
  'fedimint-clientd', 'nostr-rs-relay', 'meshtastic',
  'immich_postgres', 'immich_redis',
  // immich is now a manifest-driven stack (app_id-named, hyphen). The server is
  // the launcher app; postgres/redis are backends → Services.
  'immich-postgres', 'immich-redis',
  'mysql-mempool', 'mempool-api', 'archy-mempool-web',
  'archy-bitcoin-ui', 'archy-lnd-ui', 'archy-electrs-ui',
  'bitcoin-ui', 'lnd-ui', 'electrs-ui',
  'indeedhub-postgres', 'indeedhub-redis', 'indeedhub-minio',
  'indeedhub-api', 'indeedhub-ffmpeg',
  'indeedhub-relay', 'indeedhub-build_api_1', 'indeedhub-build_ffmpeg-worker_1',
  'indeedhub-build_postgres_1', 'indeedhub-build_redis_1', 'indeedhub-build_minio_1',
  'indeedhub-build_minio-init_1', 'indeedhub-build_relay_1',
  // Pine voice-assistant stack: the two Wyoming engines are backends (STT/TTS)
  // reached by Home Assistant over host.containers.internal — the user-facing
  // card is "pine" (the setup/status launcher), so the engines go to Services.
  'pine-whisper', 'pine-piper', 'pine-openwakeword',
])

const INTERNAL_TOOLING_NAMES = new Set([
  'buildx_buildkit_default',
])

export function isInternalToolingPackage(id: string, pkg?: PackageDataEntry): boolean {
  const manifestId = pkg?.manifest?.id || ''
  return INTERNAL_TOOLING_NAMES.has(id) || INTERNAL_TOOLING_NAMES.has(manifestId) || id.startsWith('buildx_buildkit') || manifestId.startsWith('buildx_buildkit')
}

export function isServiceContainer(id: string): boolean {
  if (SERVICE_NAMES.has(id)) return true
  if (id.startsWith('indeedhub-build_')) return true
  if (id.startsWith('archy-')) return true
  // Backend naming patterns that never carry a user-facing UI: databases and
  // caches. Safe to classify by suffix (a database is never a launcher).
  if (/-(db|postgres|postgresql|redis|valkey|mariadb|mysql|cache)$/.test(id)) return true
  if (id.endsWith('_db')) return true
  return false
}

export function isServicePackage(id: string, pkg?: PackageDataEntry): boolean {
  if (isServiceContainer(id)) return true
  const manifestId = pkg?.manifest?.id
  return !!manifestId && isServiceContainer(manifestId)
}

// Known app -> category mappings (matches App Store categorisation)
export const APP_CATEGORY_MAP: Record<string, string> = {
  'bitcoin-core': 'money', 'bitcoin-knots': 'money', 'bitcoin-ui': 'money', 'electrumx': 'money', 'electrs': 'money',
  'lnd': 'money', 'mempool': 'money', 'mempool-web': 'money', 'btcpay-server': 'commerce',
  'fedimint': 'money', 'fedimint-gateway': 'money',
  'indeedhub': 'media', 'jellyfin': 'media', 'photoprism': 'media', 'immich': 'media',
  'nextcloud': 'data', 'vaultwarden': 'data', 'filebrowser': 'data', 'cryptpad': 'data',
  'homeassistant': 'home', 'lorabell': 'home', 'endurain': 'home', 'pine': 'home',
  'searxng': 'community', 'ollama': 'community', 'grafana': 'data', 'gitea': 'data',
  'nostrudel': 'nostr',
  'tailscale': 'networking', 'netbird': 'networking', 'nginx-proxy-manager': 'networking', 'portainer': 'networking',
  'uptime-kuma': 'networking',
  'botfights': 'community',
}

export function getAppCategory(id: string, pkg: PackageDataEntry): string {
  if (APP_CATEGORY_MAP[id]) return APP_CATEGORY_MAP[id]
  const cat = (pkg.manifest as unknown as Record<string, unknown>)?.category as string | undefined
  return cat || 'other'
}

export function runtimeLanAddress(pkg: PackageDataEntry): string {
  return pkg.installed?.['interface-addresses']?.main?.['lan-address'] || ''
}

export function isKnownApp(id: string, pkg?: PackageDataEntry): boolean {
  const manifestId = pkg?.manifest?.id
  return !!(APP_CATEGORY_MAP[id] || (manifestId && APP_CATEGORY_MAP[manifestId]) || isWebOnlyApp(id))
}

// True when the package's manifest declares a front-end UI interface. This is
// the authoritative "is this a user-facing app?" signal (#45/#51): apps with a
// UI belong in "My Apps", while headless services (databases, APIs, backends,
// workers) declare no UI and belong in the "Services" tab.
export function hasFrontendUi(pkg?: PackageDataEntry): boolean {
  return !!pkg?.manifest?.interfaces?.main?.ui
}

export function isWebsitePackage(id: string, pkg?: PackageDataEntry): boolean {
  if (isInternalToolingPackage(id, pkg)) return false
  // Headless infra (databases/backends/companions) keyed by container name are
  // services regardless of any stray UI string.
  if (isServicePackage(id, pkg)) return true
  // A declared front-end UI is the deciding factor: it's an app, not a website.
  if (hasFrontendUi(pkg)) return false
  // Curated known apps stay in My Apps even if their manifest predates the UI
  // interface field.
  if (isKnownApp(id, pkg)) return false
  // Anything still here has no declared UI and isn't a known launcher app:
  // databases, APIs, backends, workers. They belong in Services (not My Apps),
  // whether or not they expose a LAN address. (#10 — "anything that isn't the
  // frontend UI launcher".)
  return !!pkg
}

export function filterEntriesForTab(
  entries: Array<[string, PackageDataEntry]>,
  activeTab: AppsTab,
  selectedCategory: string,
): Array<[string, PackageDataEntry]> {
  return entries.filter(([id, pkg]) => {
    if (isInternalToolingPackage(id, pkg)) return false
    const wantsWebsites = activeTab === 'websites' || activeTab === 'services'
    const isWebsite = isWebsitePackage(id, pkg)
    if (wantsWebsites ? !isWebsite : isWebsite) return false
    if (activeTab === 'apps' && selectedCategory !== 'all') {
      return getAppCategory(id, pkg) === selectedCategory
    }
    if (activeTab === 'services' && selectedCategory !== 'all') {
      return getServiceCategory(id, pkg) === selectedCategory
    }
    return true
  })
}

// Group a (non-launcher) service container by type for the Services tab sub-nav
// (#12). Heuristic over the container id + manifest id.
export function getServiceCategory(id: string, pkg?: PackageDataEntry): string {
  const s = `${id} ${pkg?.manifest?.id || ''}`.toLowerCase()
  if (/postgres|mariadb|mysql|(^|[-_])db([-_]|$)/.test(s)) return 'database'
  if (/redis|valkey|(^|[-_])cache([-_]|$)/.test(s)) return 'cache'
  if (/(^|[-_])api([-_]|$)/.test(s)) return 'api'
  return 'backend'
}

export function buildServiceCategories(t: (key: string) => string): Array<{ id: string; name: string }> {
  return [
    { id: 'all', name: t('marketplace.all') },
    { id: 'database', name: 'Databases' },
    { id: 'cache', name: 'Caches' },
    { id: 'api', name: 'APIs' },
    { id: 'backend', name: 'Backends' },
  ]
}

// Web-only app IDs and their URLs
export const WEB_ONLY_APP_URLS: Record<string, string> = {}

export function isWebOnlyApp(id: string): boolean {
  return id in WEB_ONLY_APP_URLS
}

// Web-only apps (no container) -- always show as installed bookmarks
export const WEB_ONLY_APPS: Record<string, PackageDataEntry> = {}

/** Apps that open in a new browser tab (X-Frame-Options blocks iframe) */
export const TAB_LAUNCH_APPS = new Set([
  'btcpay-server', 'grafana', 'photoprism', 'homeassistant',
  'vaultwarden', 'nextcloud', 'uptime-kuma', 'portainer', 'gitea',
  'cryptpad', 'nginx-proxy-manager', 'tailscale',
  // netbird's dashboard needs HTTPS (secure context) so it opens in a new tab
  'netbird',
])

export function opensInTab(id: string): boolean {
  // The launch icon and launch behavior follow the dynamic verdict too:
  // apps observed refusing the iframe (remembered by the app session) show
  // the same tab-launch affordance as the statically-known tab apps.
  return TAB_LAUNCH_APPS.has(id) || isAutoTabApp(id)
}

// Backend services that ship no icon of their own reuse their PARENT app's icon
// (#14) so they render the app's logo instead of a 404 → 📦 placeholder. Paths
// are explicit because icon extensions vary (.png / .webp / .svg).
const APP_ICON_FALLBACKS: Record<string, string> = {
  gitea: '/assets/img/app-icons/gitea.svg',
  // Apps whose icon extension isn't .png: without an explicit entry the
  // default `<id>.png` guess 404s on every render (console spam, and for
  // mempool the .png→.svg fallback chain 404s TWICE before giving up).
  pine: '/assets/img/app-icons/pine.svg',
  mempool: '/assets/img/app-icons/mempool.webp',
  'mempool-web': '/assets/img/app-icons/mempool.webp',
  'fedimint-gateway': '/assets/img/app-icons/fedimint.png',
  'fedimint-clientd': '/assets/img/app-icons/fedimint.png',
  // immich stack
  'immich-postgres': '/assets/img/app-icons/immich.png',
  'immich-redis': '/assets/img/app-icons/immich.png',
  'immich-server': '/assets/img/app-icons/immich.png',
  'immich_postgres': '/assets/img/app-icons/immich.png',
  'immich_redis': '/assets/img/app-icons/immich.png',
  // btcpay stack
  'archy-btcpay-db': '/assets/img/app-icons/btcpay-server.png',
  'archy-nbxplorer': '/assets/img/app-icons/btcpay-server.png',
  // mempool stack
  'archy-mempool-db': '/assets/img/app-icons/mempool.webp',
  'mempool-api': '/assets/img/app-icons/mempool.webp',
  'archy-mempool-web': '/assets/img/app-icons/mempool.webp',
  'mysql-mempool': '/assets/img/app-icons/mempool.webp',
  // bitcoin / lightning companion UIs
  'archy-bitcoin-ui': '/assets/img/app-icons/bitcoin-knots.webp',
  'archy-lnd-ui': '/assets/img/app-icons/lnd.png',
  'archy-electrs-ui': '/assets/img/app-icons/electrumx.png',
  // ElectrumX ships under a few historical ids (the backend was renamed
  // electrs → electrumx). Pin the whole family to the ElectrumX icon so My
  // Apps shows the right logo no matter which id the node has it installed
  // under.
  'electrs': '/assets/img/app-icons/electrumx.png',
  'electrs-ui': '/assets/img/app-icons/electrumx.png',
  'electrumx': '/assets/img/app-icons/electrumx.png',
}

// Parent-app icon by prefix, for stack members not listed explicitly above
// (e.g. every indeedhub-* sub-container → indeedhub).
const SERVICE_ICON_PREFIXES: Array<[string, string]> = [
  ['indeedhub-', '/assets/img/app-icons/indeedhub.png'],
  ['immich-', '/assets/img/app-icons/immich.png'],
  ['immich_', '/assets/img/app-icons/immich.png'],
  ['pine-', '/assets/img/app-icons/pine.svg'],
]

function serviceParentIcon(id: string): string | undefined {
  for (const [prefix, icon] of SERVICE_ICON_PREFIXES) {
    if (id.startsWith(prefix)) return icon
  }
  return undefined
}

export const DEFAULT_APP_ICON = '/assets/icon/favico-black-v2.svg'

export function resolveAppIcon(id: string, pkg: PackageDataEntry, curatedIcon?: string): string {
  const rawIcon = (pkg["static-files"]?.icon || "").trim()
  const icon = rawIcon === '/assets/img/favico.png' ? '' : rawIcon
  if (
    icon.startsWith("/") ||
    icon.startsWith("http://") ||
    icon.startsWith("https://") ||
    icon.startsWith("data:image")
  ) {
    return icon
  }
  return (
    curatedIcon ||
    APP_ICON_FALLBACKS[id] ||
    serviceParentIcon(id) ||
    // Never guess `${id}.png` — an unmapped id 404s (strfry did, 2026-08-07).
    // The A mark is the honest unknown-app tile.
    DEFAULT_APP_ICON
  )
}

export function canLaunch(pkg: PackageDataEntry): boolean {
  if (isWebOnlyApp(pkg.manifest.id)) return true
  // Headless backends never get a Launch button, even with a published port.
  if (isServicePackage(pkg.manifest.id, pkg)) return false
  const hasRuntimeAddress = !!pkg.installed?.['interface-addresses']?.main?.['lan-address']
  const hasKnownLaunchUrl = typeof window !== 'undefined' && !!resolveAppUrl(pkg.manifest.id)
  // A bare runtime address is only a launch signal for curated apps: the
  // backend now sets interfaces.main.ui strictly for confirmed web UIs
  // (manifest declaration or HTTP probe), so an unknown container with an
  // exposed non-UI port must not become launchable just for having one.
  const hasUI = pkg.manifest.interfaces?.main?.ui
    || hasKnownLaunchUrl
    || (hasRuntimeAddress && isKnownApp(pkg.manifest.id, pkg))
  if ((pkg.manifest.id === 'fedimint' || pkg.manifest.id === 'fedimintd') && hasUI) {
    return pkg.state === PackageState.Running || pkg.state === PackageState.Starting
  }
  // A static launch URL (e.g. a host-networked companion UI like
  // archy-electrs-ui) serves independently of the backend's own sync state, so
  // the tile stays launchable while the backend is still 'starting' (ElectrumX
  // indexes for 10m+ on first run). A genuinely 'unhealthy' backend still
  // blocks. Apps that rely on a runtime interface-address keep the strict gate.
  const blockedByHealth =
    pkg.health === 'unhealthy' || (pkg.health === 'starting' && !hasKnownLaunchUrl)
  return !!hasUI && pkg.state === 'running' && !blockedByHealth
}

export function launchBlockedReason(id: string, pkg?: PackageDataEntry | null): string {
  const appId = pkg?.manifest?.id || id
  if (
    (appId === 'fedimint' || appId === 'fedimintd') &&
    (pkg?.state === PackageState.Starting || (pkg?.state === PackageState.Running && pkg?.health === 'starting'))
  ) {
    return 'Guardian opens a wait page until Bitcoin finishes initial sync.'
  }
  return ''
}

export function resolveRuntimeLaunchUrl(pkg: PackageDataEntry): string {
  const addr = runtimeLanAddress(pkg)
  if (!addr || typeof window === 'undefined') return addr
  const local = addr.replace(/^http:\/\/(localhost|127\.0\.0\.1)(?=[:/]|$)/, `http://${window.location.hostname}`)
  // The backend reports runtime URLs as http:// because that is how the app
  // binds locally — on an HTTPS connection that is a cleartext downgrade.
  // Upgrade only when the app gate fronts the port (it serves TLS there);
  // a container-published plain-HTTP port would fail over https outright.
  try {
    const port = new URL(local).port
    if (!portIsGateFronted(pkg.manifest.id, port)) return local
  } catch { /* keep as-is */ }
  return matchPageScheme(local)
}

export function getStatusClass(state: PackageState, health?: string | null, exitCode?: number | null): string {
  if (state === PackageState.Running && health === 'starting') return 'bg-yellow-500/20 text-yellow-200'
  if (state === PackageState.Running && health === 'unhealthy') return 'bg-orange-500/20 text-orange-200'
  switch (state) {
    case PackageState.Running:
      return 'bg-green-500/20 text-green-200'
    case PackageState.Stopped:
      return 'bg-gray-500/20 text-gray-200'
    case PackageState.Exited:
      // Exit code 0 = clean shutdown (gray), non-zero = crash (red)
      return exitCode != null && exitCode !== 0
        ? 'bg-red-500/20 text-red-200'
        : 'bg-gray-500/20 text-gray-200'
    case PackageState.Starting:
    case PackageState.Stopping:
    case PackageState.Restarting:
      return 'bg-yellow-500/20 text-yellow-200'
    case PackageState.Installing:
      return 'bg-blue-500/20 text-blue-200'
    case PackageState.Updating:
      return 'bg-orange-500/20 text-orange-200'
    default:
      return 'bg-gray-500/20 text-gray-200'
  }
}

export function getStatusLabel(state: PackageState, health?: string | null, exitCode?: number | null): string {
  if (state === PackageState.Running && health === 'starting') return 'starting up'
  if (state === PackageState.Running && health === 'unhealthy') return 'unhealthy'
  if (state === PackageState.Running && health === 'healthy') return 'healthy'
  if (state === PackageState.Updating) return 'updating...'
  if (state === PackageState.Running) return 'running'
  if (state === PackageState.Exited || state === PackageState.Stopped) {
    if (exitCode === 137) return 'killed (SIGKILL)'
    if (exitCode != null && exitCode !== 0) return 'crashed'
    return 'stopped'
  }
  return state
}

export function buildAllCategories(t: (key: string) => string) {
  return [
    { id: 'all', name: t('marketplace.all') },
    { id: 'community', name: t('marketplace.community') },
    { id: 'nostr', name: 'Nostr' },
    { id: 'commerce', name: t('marketplace.commerce') },
    { id: 'money', name: t('marketplace.money') },
    { id: 'data', name: t('marketplace.data') },
    { id: 'media', name: 'Media' },
    { id: 'home', name: t('marketplace.homeCategory') },
    { id: 'networking', name: t('marketplace.networking') },
    { id: 'other', name: t('marketplace.other') },
  ]
}

export function useCategoriesWithApps(
  packages: Ref<Record<string, PackageDataEntry>>,
  allCategories: Ref<Array<{ id: string; name: string }>>,
) {
  return computed(() => {
    const entries = Object.entries(packages.value).filter(([id, pkg]) => !isWebsitePackage(id, pkg) && !isInternalToolingPackage(id, pkg))
    return allCategories.value.filter(cat => {
      if (cat.id === 'all') return true
      return entries.some(([id, pkg]) => getAppCategory(id, pkg) === cat.id)
    })
  })
}

// Services-tab equivalent of useCategoriesWithApps: only show a service category
// when at least one installed service belongs to it (#12).
export function useServiceCategories(
  packages: Ref<Record<string, PackageDataEntry>>,
  serviceCategories: Ref<Array<{ id: string; name: string }>>,
) {
  return computed(() => {
    const entries = Object.entries(packages.value).filter(([id, pkg]) => isWebsitePackage(id, pkg) && !isInternalToolingPackage(id, pkg))
    return serviceCategories.value.filter(cat => {
      if (cat.id === 'all') return true
      return entries.some(([id, pkg]) => getServiceCategory(id, pkg) === cat.id)
    })
  })
}

export function handleImageError(e: Event) {
  const target = e.target as HTMLImageElement
  const currentSrc = target.src

  if (target.dataset.fallbackTried !== "1" && currentSrc.endsWith(".png")) {
    target.dataset.fallbackTried = "1"
    target.src = currentSrc.replace(/\.png($|\?)/, ".svg$1")
    return
  }

  if (!currentSrc.includes(DEFAULT_APP_ICON)) {
    target.src = DEFAULT_APP_ICON
    target.dataset.defaultIcon = "1"
  }
}
