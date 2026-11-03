<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import BackButton from '@/components/BackButton.vue'
import DeviceModel from '@/components/DeviceModel.vue'
import HardwareMarketplace from '@/components/HardwareMarketplace.vue'
import ProvisionProgressPanel from '@/components/ProvisionProgressPanel.vue'

const router = useRouter()
function goBack() {
  router.push({ name: 'server' })
}

interface ReleaseInfo {
  openwrt_release?: string
  openwrt_version?: string
  openwrt_board_name?: string
  openwrt_arch?: string
  openwrt_target?: string
  distrib_description?: string
  distrib_release?: string
  distrib_arch?: string
  distrib_target?: string
}

interface WifiInterface {
  section: string
  ssid: string
  device: string
  encryption: string
  network: string
  disabled: boolean
}

interface WifiGroup {
  ssid: string
  network: string
  encryption: string
  disabled: boolean
  radios: string[]
  sections: string[]
}

interface MintConfig {
  url: string
  price_per_step: number
  purchase_min_steps: number
  price_unit?: string
  min_balance?: number
  balance_tolerance_percent?: number
  payout_interval_seconds?: number
  min_payout_amount?: number
}

interface TollGateStatus {
  installed: boolean
  enabled?: boolean
  metric?: string
  step_size_ms?: number
  price_per_step?: number
  min_steps?: number
  currency?: string
  mint_url?: string
  payout_address?: string
  private_ssid?: string
  version?: string
  version_known?: boolean
  pinned_version?: string
  up_to_date?: boolean
  pkg_manager?: string
  daemon_running?: boolean
  daemon_version?: string
  daemon_uptime?: string
  daemon_error?: string
  daemon_version_matches?: boolean
  advertised_mints?: { url: string; price: number; unit: string; min_steps: number }[]
  configured_mints?: number
  accepted_mints?: MintConfig[]
  installer_feed_tag?: string
  nodogsplash_gateway?: string
  nodogsplash_ok?: boolean
}

interface InstallerStatus {
  available: boolean
  installer_version?: string
  feed_release_tag?: string
  error?: string
}

interface DiscoveredRouter {
  ip: string
  mac?: string
  name?: string
  vendor?: string
  model?: string
  firmware?: string
  identified?: boolean
  ssh_open?: boolean
  ssh_fingerprint?: string
  password_status?: string
  note?: string
  source?: string
  active?: boolean
  link_type?: string
  interface?: string
  subnet?: string
  has_saved_credentials?: boolean
  saved_hostname?: string
  saved_model?: string
  kind?: string
}

interface SelectRouterResult {
  ok: boolean
  host?: string
  address?: string
  username?: string
  hostname?: string
  model?: string
  used_saved_credentials?: boolean
  needs_password?: boolean
  needs_root_password?: boolean
  bad_password?: boolean
  error?: string
}

interface WalletInfo {
  ok: boolean
  degraded?: boolean
  error?: string
  wallet?: {
    total_balance: number
    mint_count: number
    mint_balances: Record<string, number>
  }
}

interface DrainMint {
  url: string
  amount: number
}

interface DrainPreview {
  ok: boolean
  error?: string
  router?: {
    total_balance: number
    wallet_ok?: boolean
    running?: boolean
  }
  node?: {
    network: string
    is_test: boolean
    balance_sats: number
    mint_url: string
    accepted_mints: string[]
  }
  receivable?: DrainMint[]
  not_accepted?: DrainMint[]
  wrong_network?: DrainMint[]
}

interface DrainFailed {
  mint: string
  amount: number
  error: string
  token?: string
}

interface DrainResult {
  ok: boolean
  error?: string
  node?: DrainPreview['node']
  total_received?: number
  received?: { mint: string; amount: number }[]
  failed?: DrainFailed[]
  router_errors?: unknown[]
}

interface ProvisionStep {
  id: string
  label: string
  status: 'pending' | 'running' | 'done' | 'skipped' | 'failed' | string
  detail?: string
}

interface ProvisionProgress {
  provision_id: string
  kind: string
  phase: string
  percent: number
  current: string
  steps: ProvisionStep[]
  done: boolean
  failed: boolean
  error?: string
  host?: string
  result?: Record<string, unknown>
}

interface FlashLink {
  name: string
  carrier: boolean
  wireless: boolean
  physical?: boolean
  preferred?: boolean
  ipv4: string[]
}

interface CleanSlateInfo {
  ok: boolean
  needs_trust?: boolean
  ssh_fingerprint?: string
  host: string
  version: string
  board?: string
  image_url?: string
  sha256?: string
  flash_ready?: boolean
  links?: FlashLink[]
  error?: string
}

interface FlashStep {
  name: string
  status: string
  detail?: string
}

interface CleanSlateJob {
  ip?: string
  status: string
  step?: number
  steps?: FlashStep[]
  error?: string
  note?: string
}

interface UpstreamSTA {
  ssid: string
  status: string
  radio: string
  encryption: string
}

interface KnownTollGate {
  ssid: string
  bssid?: string
  price_per_step?: number
  step_size?: number
  best_signal?: number
  worst_signal?: number
  sample_count?: number
  first_seen?: string
  last_seen?: string
}

interface KnownTollGateGroup {
  ssid: string
  best: KnownTollGate
  entries: KnownTollGate[]
  confirmed: boolean
  stale: boolean
  bestSignal: number
  lastSeen?: string
  sampleCount: number
}

interface MintHealthEntry {
  url: string
  advertised: boolean
  ok: boolean
  error?: string
}

interface MintHealth {
  configured: number
  advertised: number
  mints: MintHealthEntry[]
}

interface UpstreamNetwork {
  ssid: string
  signal: number
  encryption: string
  bssid?: string
  band?: string
  is_tollgate?: boolean
  price_per_step?: number
  step_size?: number
}

interface WanStatus {
  configured: boolean
  ssid: string
  assoc_ssid: string
  encryption: string
  ip: string
  internet: boolean
  radio0_disabled: boolean
  sta_iface: string
  sta_state: string
  wifi_log: string
  lan_ip: string
  lan_netmask: string
  dhcp_start: string
  dhcp_limit: string
  masq: boolean
}

interface RouterStatus {
  host: string
  hostname: string
  uptime_secs: number
  release: ReleaseInfo
  tollgate: TollGateStatus
  wifi_interfaces: WifiInterface[]
  wan: WanStatus
}

interface ScannedNetwork {
  ssid: string
  bssid: string
  signal: number
  channel: number
  encryption: string
  band?: string
  is_tollgate?: boolean
  price_per_step?: number
  step_size?: number
}

interface RankedNetwork {
  ssid: string
  bssid?: string
  signal: number
  encryption: string
  band?: string
  channel?: number
  isTollGate: boolean
  pricePerStep?: number
  stepSize?: number
  radios: number
  lastSeen?: string
}

// Router status is cached via the shared useCachedResource hook so revisits
// paint the last-known state instantly and a mount inside the TTL issues no
// new RPC. No item id in this key — there is exactly one configured gateway
// per node, not a per-item collection (route is the fixed `server/openwrt`,
// no :id param). This fetcher only ever re-fetches with `{}` params (the
// server-persisted config, no credentials needed) — an explicit-params call
// (Connect form submit, or any caller passing connectedParams) never routes
// through this fetcher at all; see load()'s bypass path below (WR-03).
const routerResource = useCachedResource<RouterStatus>({
  key: 'server.openwrt-status',
  fetcher: (signal) => rpcClient.call<RouterStatus>({
    method: 'openwrt.get-status',
    params: {},
    signal,
    timeout: 30000,
    dedup: true,
    maxRetries: 1,
  }),
  ttlMs: 30_000,
  // The selected gateway is explicit. A cached status from a different
  // router must never paint before the operator selects one, and a focus
  // event must not silently reconnect to the last saved router.
  persist: false,
  revalidateOnFocus: false,
  immediate: false,
})
const status = computed(() => routerResource.data.value)
// 'refreshing' deliberately excluded (D-07, keep-last-value): a background
// TTL revalidation must not hide the already-rendered status panels behind
// the full loading skeleton — only a true first-load (no data at all yet)
// blocks on it. Previously this conflated the two, so a cache hit would
// still briefly show the skeleton on every mount instead of painting
// instantly.
const loading = computed(() => {
  if (!selectionConfirmed.value) return false
  const s = routerResource.loadState.value
  return routerResource.data.value === null && (s === 'loading' || s === 'idle')
})
const error = ref('')
const host = ref('')
const sshUser = ref('root')
const sshPassword = ref('')
const showConnectForm = ref(false)
const routerShopOpen = ref(false)
const routerCard = ref<HTMLElement>()
const routerCardHeight = ref<string>()
function openRouterShop() {
  const height = routerCard.value?.getBoundingClientRect().height
  if (height) routerCardHeight.value = `${height}px`
  routerShopOpen.value = true
}
async function closeRouterShop() {
  routerShopOpen.value = false
  await nextTick()
  routerCardHeight.value = undefined
}
const connecting = ref(false)
const connectedParams = ref<Record<string, string> | null>(null)

// Every action below (install/edit TollGate, WiFi scan, WAN configure) needs
// host/ssh_user/ssh_password to reach the router. `connectedParams` only gets
// set when the Connect form was actually submitted this session (WR-03 above)
// — on a normal page load the router reconnects via the server-persisted
// config instead, so `sshPassword`/`sshUser`/`host` (the Connect form's own
// local refs) sit at their untouched defaults ('', 'root', ''). Falling back
// to those refs here used to send an explicit-but-empty ssh_password, which
// the backend treats as "the caller provided this" and never falls back to
// the real saved password — a real router password then fails auth on every
// action even though the status poll (which sends no params at all) keeps
// working fine (archy-x250-pa3, 2026-09-05: dropbear logged one bad-password
// attempt at the exact moment "Install TollGate" was clicked). Omitting the
// fields entirely when there's no explicit connectedParams lets the backend's
// own saved-config fallback do the right thing, same as the status poll.
function authParams(): Record<string, string> {
  return connectedParams.value ?? {}
}

const detecting = ref(false)
const detectError = ref('')
const detectedCandidates = ref<string[]>([])

const provisioning = ref(false)
const provisionError = ref('')
const provisionSuccess = ref(false)
const provisionSkippedMints = ref<string[]>([])
const provisionProgress = ref<ProvisionProgress | null>(null)
let provisionTimer: ReturnType<typeof setInterval> | null = null
const repairingPortal = ref(false)
const portalError = ref('')

const PRODUCTION_MINT_URLS = [
  'https://mint.coinos.io',
  'https://mint.macadamia.cash',
  'https://mint.westernbtc.com',
  'https://kashu.me',
  'https://mint.cubabitcoin.org',
]
const TEST_MINT_URLS = [
  'https://testnut.cashu.space',
  'https://testnut.cashu.exchange',
]

function defaultInstallMints(): MintConfig[] {
  return PRODUCTION_MINT_URLS.map(url => ({
    url,
    price_per_step: 10,
    purchase_min_steps: 1,
    price_unit: 'sat',
  }))
}

// First-install pricing. The Edit form already exposes these after install;
// sending them here too means the operator does not have to install at the
// defaults and immediately edit.
const installPriceSats = ref(10)
const installStepSizeMin = ref(1)
const installStepSizeMb = ref(21)
const installMetric = ref<'milliseconds' | 'bytes'>('milliseconds')
const installMinSteps = ref(1)
const installMints = ref<MintConfig[]>(defaultInstallMints())
const installNewMint = ref('')
const installShowTestMints = ref(false)
const installPayoutAddress = ref('')

const installerStatus = ref<InstallerStatus | null>(null)
const discovered = ref<DiscoveredRouter[]>([])
const discovering = ref(false)
const discoveryError = ref('')
const discoveryActive = ref('')
const showUnverifiedRouters = ref(false)
const showDisabledWifi = ref(false)
const selectedRouterIp = ref('')
const needsPassword = ref(false)
const selectionConfirmed = ref(false)
const selectedRouter = computed(() =>
  discovered.value.find(r => r.ip === selectedRouterIp.value) ?? null,
)
const visibleDiscovered = computed(() =>
  discovered.value.filter(r => showUnverifiedRouters.value || r.ssh_open || r.active),
)
const flashHost = computed(
  () => selectedRouter.value?.ip || status.value?.host || host.value.trim(),
)
const flashCanStart = computed(() => {
  const info = flashInfo.value
  return (
    !!info?.ok &&
    !!info.board &&
    info.flash_ready === true &&
    flashAcknowledge.value &&
    flashConfirmation.value.trim() === info.board
  )
})
const trustRouterPending = ref(false)
const trustRouterError = ref('')
const trustRouterDone = ref(false)

const walletInfo = ref<WalletInfo | null>(null)
const walletLoading = ref(false)
const walletError = ref('')
const daemonStarting = ref(false)
const lastUpdatedAt = ref(0)
const nowTick = ref(Date.now())
const readinessExhausted = ref(false)
let readinessRun: Promise<void> | null = null
let readinessActive = false
let clockTimer: ReturnType<typeof setInterval> | null = null
let gatewayDisposed = false

// A freshly restarted daemon can take well over a minute to fetch keysets and
// advertise every configured mint (observed ~2 minutes on a 5-mint router).
const READINESS_WINDOW_MS = 180_000
const drainPreview = ref<DrainPreview | null>(null)
const drainPreviewLoading = ref(false)
const drainPreviewError = ref('')
const drainModal = ref(false)
const drainExecuting = ref(false)
const drainMintAdding = ref(false)
const drainMintError = ref('')
const drainResult = ref<DrainResult | null>(null)
const drainResultModal = ref(false)
const flashOpen = ref(false)
const flashVersion = ref('25.12.5')
const flashPassword = ref('')
const flashNewPassword = ref('')
const flashInfo = ref<CleanSlateInfo | null>(null)
const flashInfoLoading = ref(false)
const flashInfoError = ref('')
const flashConfirmation = ref('')
const flashAcknowledge = ref(false)
const flashRunning = ref(false)
const flashJobId = ref('')
const flashJob = ref<CleanSlateJob | null>(null)
const flashError = ref('')
const flashTrustPending = ref(false)
const flashRecoveryLoading = ref(false)
const flashRecoveryTrusting = ref(false)
const flashRecoveryError = ref('')
const flashRecoveryInfo = ref<{
  ip: string
  identified?: boolean
  firmware?: string
  model?: string
  name?: string
  ssh_fingerprint?: string
} | null>(null)
let flashTimer: ReturnType<typeof setInterval> | null = null
const upstreamList = ref<UpstreamSTA[]>([])
const upstreamKnown = ref<KnownTollGate[]>([])
const upstreamNetworks = ref<UpstreamNetwork[]>([])
const upstreamLoading = ref(false)
const upstreamError = ref('')
const upstreamPassphrase = ref('')
const upstreamScanned = ref(false)
const showAllUpstreams = ref(false)
const showAllUplinkNetworks = ref(false)
const mintHealth = ref<MintHealth | null>(null)
const mintHealthLoading = ref(false)
const mintHealthError = ref('')
const showMintHealth = ref(false)
const applyingNetworkNames = ref(false)
const networkNamesError = ref('')
const trustRequired = ref(false)
const trustPending = ref(false)
const trustFingerprint = ref('')
const pendingProvision = ref<Record<string, unknown> | null>(null)

// TollGate reconfigure form (shown once installed)
const editingTollgate = ref(false)
const updatingTollgate = ref(false)
const updateTollgateError = ref('')
const editPriceSats = ref(10)
const editStepSizeMin = ref(1)
const editStepSizeMb = ref(21)
const editMetric = ref<'milliseconds' | 'bytes'>('milliseconds')
const editMinSteps = ref(1)
const editMints = ref<MintConfig[]>([])
type MintTestState = { status: 'idle' | 'testing' | 'ok' | 'fail'; detail: string }
const mintTest = ref<Record<number, MintTestState>>({})
const editPayoutAddress = ref('')
const editPrivateSsid = ref('noderunner')
const editPrivateKey = ref('')
const editEnabled = ref(true)

const daemonLabel = computed(() => {
  const t = status.value?.tollgate
  if (!t) return 'unknown'
  if (t.daemon_running) {
    const up = t.daemon_uptime ? ` (up ${t.daemon_uptime})` : ''
    return `${t.daemon_version || 'unknown'}${up}`
  }
  return t.daemon_error || 'unreachable — restart required'
})

const daemonVersionLabel = computed(() => {
  const t = status.value?.tollgate
  return t?.daemon_version || t?.version || 'unknown'
})

const tollgateActive = computed(() => {
  const t = status.value?.tollgate
  return !!t && (t.daemon_running === true || t.enabled === true)
})

const tollgateStateLabel = computed(() => {
  const t = status.value?.tollgate
  if (!t) return 'Unknown'
  if (t.daemon_running) return 'Enabled'
  if (t.enabled) return 'Enabled · daemon down'
  return 'Disabled'
})

const restartRequired = computed(() => {
  const t = status.value?.tollgate
  return !!t && t.installed && (!t.daemon_running || t.daemon_version_matches === false)
})

const installerNeeded = computed(() => {
  const t = status.value?.tollgate
  return !!t && (!t.installed || t.up_to_date === false)
})

const installBlockedNoWan = computed(() => {
  const wan = status.value?.wan
  return !!wan && wan.internet === false
})

const installUplinkMessage = computed(() => {
  const wan = status.value?.wan
  if (!wan) return ''
  return wan.configured
    ? 'Internet uplink is configured but offline. Reconnect it before installing TollGate.'
    : 'No internet uplink is configured. Configure the internet uplink before installing TollGate.'
})

function scrollToUplink() {
  document
    .querySelector('[data-testid="internet-uplink"]')
    ?.scrollIntoView({ behavior: 'smooth', block: 'center' })
}

const adminReady = computed(() => {
  const t = status.value?.tollgate
  return (
    selectionConfirmed.value &&
    !!t &&
    t.installed &&
    t.up_to_date !== false &&
    t.daemon_running !== false
  )
})

// The internet uplink is an ongoing admin control, not just an install step.
// It stays visible after the router is current so an operator can reconnect a
// broken WAN without reinstalling TollGate.
const showUplink = computed(() => selectionConfirmed.value && !!status.value)

const advertisedMintKeys = computed(
  () =>
    new Set(
      (status.value?.tollgate?.advertised_mints ?? []).map(m =>
        normalizeMintUrl(m.url),
      ),
    ),
)

const missingMintRows = computed(() =>
  mintRows.value.filter(
    m => m.url.trim() && !advertisedMintKeys.value.has(normalizeMintUrl(m.url)),
  ),
)

const mintHealthRows = computed(() => mintHealth.value?.mints ?? [])
const unreachableMints = computed(() =>
  mintHealthRows.value.filter(m => !m.ok),
)

function durationToSeconds(value?: string): number {
  if (!value) return 0
  let total = 0
  const pattern = /(\d+(?:\.\d+)?)(h|m|s)/g
  let match: RegExpExecArray | null
  while ((match = pattern.exec(value))) {
    const amount = Number(match[1])
    total +=
      match[2] === 'h' ? amount * 3600 : match[2] === 'm' ? amount * 60 : amount
  }
  return total
}

const daemonWarmingUp = computed(() => {
  const t = status.value?.tollgate
  if (!t?.installed) return false
  if (daemonStarting.value) return true
  return (
    t.daemon_running === true &&
    (t.advertised_mints?.length ?? 0) < (t.configured_mints ?? 0) &&
    durationToSeconds(t.daemon_uptime) < READINESS_WINDOW_MS / 1000
  )
})

const lastUpdatedLabel = computed(() => {
  if (!lastUpdatedAt.value) return ''
  const seconds = Math.max(
    0,
    Math.floor((nowTick.value - lastUpdatedAt.value) / 1000),
  )
  if (seconds < 5) return 'just now'
  if (seconds < 60) return `${seconds}s ago`
  const minutes = Math.floor(seconds / 60)
  return `${minutes}m ago`
})

const walletDegraded = computed(() => {
  if (walletInfo.value?.degraded) return true
  const t = status.value?.tollgate
  if (!t) return false
  return (t.configured_mints ?? 0) > 0 && (t.advertised_mints?.length ?? 0) === 0
})

const walletDegradedReason = computed(() => {
  if (walletInfo.value?.degraded) {
    return walletInfo.value.error || 'the wallet did not return a usable balance'
  }
  const t = status.value?.tollgate
  if (t && (t.configured_mints ?? 0) > 0 && (t.advertised_mints?.length ?? 0) === 0) {
    return 'the daemon is not advertising any configured mint'
  }
  return 'unknown'
})

const walletMints = computed(() =>
  Object.entries(walletInfo.value?.wallet?.mint_balances ?? {}),
)
const walletTotal = computed(
  () => walletInfo.value?.wallet?.total_balance ?? 0,
)
const walletMintCount = computed(
  () => walletInfo.value?.wallet?.mint_count ?? 0,
)

const publicSsid = computed(
  () =>
    status.value?.wifi_interfaces.find(
      i => i.network === 'tollgate' && !i.disabled,
    )?.ssid ?? '',
)
const privateSsid = computed(
  () =>
    status.value?.wifi_interfaces.find(
      i => i.network === 'private' && !i.disabled,
    )?.ssid ?? '',
)
const networkNamesOutdated = computed(() => {
  const pub = publicSsid.value
  const priv = privateSsid.value
  return (
    !pub ||
    !priv ||
    !pub.startsWith('archipelago-tollgate-') ||
    !priv.startsWith('noderunner-')
  )
})

const wifiGroups = computed<WifiGroup[]>(() => {
  const groups = new Map<string, WifiGroup>()
  for (const iface of status.value?.wifi_interfaces ?? []) {
    const key = iface.ssid || '(hidden)'
    const existing = groups.get(key)
    if (existing) {
      existing.disabled = existing.disabled && iface.disabled
      if (!existing.radios.includes(iface.device)) existing.radios.push(iface.device)
      existing.sections.push(iface.section)
      if (existing.encryption === 'none' && iface.encryption !== 'none') {
        existing.encryption = iface.encryption
      }
      continue
    }
    groups.set(key, {
      ssid: key,
      network: iface.network,
      encryption: iface.encryption,
      disabled: iface.disabled,
      radios: [iface.device],
      sections: [iface.section],
    })
  }
  return [...groups.values()]
})

const visibleWifiGroups = computed(() =>
  showDisabledWifi.value
    ? wifiGroups.value
    : wifiGroups.value.filter(g => !g.disabled),
)

const knownGroups = computed<KnownTollGateGroup[]>(() => {
  const groups = new Map<string, KnownTollGate[]>()
  for (const entry of upstreamKnown.value) {
    const key = entry.ssid || '(hidden)'
    const list = groups.get(key) ?? []
    list.push(entry)
    groups.set(key, list)
  }
  const now = Date.now()
  const result: KnownTollGateGroup[] = []
  for (const [ssid, entries] of groups) {
    const best = entries.reduce((a, b) =>
      (b.best_signal ?? -999) > (a.best_signal ?? -999) ? b : a,
    )
    const lastSeenMs = Math.max(
      ...entries.map(e => (e.last_seen ? Date.parse(e.last_seen) : 0)),
      0,
    )
    result.push({
      ssid,
      best,
      entries,
      confirmed: entries.some(e => (e.price_per_step ?? 0) > 0),
      stale: lastSeenMs > 0 && now - lastSeenMs > 30 * 60_000,
      bestSignal: best.best_signal ?? -999,
      lastSeen: lastSeenMs > 0 ? new Date(lastSeenMs).toISOString() : undefined,
      sampleCount: entries.reduce((n, e) => n + (e.sample_count ?? 0), 0),
    })
  }
  return result.sort((a, b) => {
    if (a.confirmed !== b.confirmed) return a.confirmed ? -1 : 1
    return b.bestSignal - a.bestSignal
  })
})

function looksLikeTollGate(ssid: string, flag?: boolean): boolean {
  return (
    flag === true ||
    /^!?(TollGate|Archipelago|Net4sats)(-|$)/i.test((ssid || '').trim())
  )
}

function buildRankedNetworks(
  networks: Array<{
    ssid: string
    signal?: number
    encryption?: string
    bssid?: string
    band?: string
    channel?: number
    is_tollgate?: boolean
    price_per_step?: number
    step_size?: number
  }>,
  ownSsids: Set<string>,
): RankedNetwork[] {
  const groups = new Map<string, RankedNetwork>()
  for (const network of networks) {
    const ssid = (network.ssid || '').trim()
    if (!ssid || ownSsids.has(ssid)) continue
    const signal = Number.isFinite(network.signal)
      ? Number(network.signal)
      : -999
    const isTollGate = looksLikeTollGate(ssid, network.is_tollgate)
    const existing = groups.get(ssid)
    if (!existing) {
      groups.set(ssid, {
        ssid,
        bssid: network.bssid,
        signal,
        encryption: network.encryption || 'unknown',
        band: network.band,
        channel: network.channel,
        isTollGate,
        pricePerStep: network.price_per_step,
        stepSize: network.step_size,
        radios: 1,
      })
      continue
    }
    existing.radios += 1
    existing.isTollGate = existing.isTollGate || isTollGate
    if (signal > existing.signal) {
      existing.signal = signal
      existing.bssid = network.bssid ?? existing.bssid
      existing.encryption = network.encryption || existing.encryption
      existing.band = network.band ?? existing.band
      existing.channel = network.channel ?? existing.channel
    }
    if (existing.pricePerStep === undefined && (network.price_per_step ?? 0) > 0) {
      existing.pricePerStep = network.price_per_step
      existing.stepSize = network.step_size
    }
  }
  return [...groups.values()]
}

function rankNetworks(networks: RankedNetwork[]): RankedNetwork[] {
  return [...networks].sort((a, b) => {
    if (a.isTollGate !== b.isTollGate) return a.isTollGate ? -1 : 1
    return b.signal - a.signal
  })
}

function splitRankedNetworks(networks: RankedNetwork[]) {
  const tollgates = networks.filter(network => network.isTollGate)
  const regular = networks.filter(network => !network.isTollGate)
  return {
    tollgates,
    regular,
    collapsed: [...tollgates, ...regular.slice(0, 3)],
    hidden: Math.max(0, regular.length - 3),
  }
}

const ownSsidSet = computed(
  () =>
    new Set(
      (status.value?.wifi_interfaces ?? [])
        .map(iface => (iface.ssid || '').trim())
        .filter(Boolean),
    ),
)

const rankedLiveUpstreams = computed(() =>
  rankNetworks(buildRankedNetworks(upstreamNetworks.value, ownSsidSet.value)),
)

const rankedKnownUpstreams = computed(() =>
  rankNetworks(
    knownGroups.value.map(group => ({
      ssid: group.ssid,
      bssid: group.best.bssid,
      signal: group.bestSignal,
      encryption: 'unknown',
      isTollGate: true,
      pricePerStep: group.best.price_per_step,
      stepSize: group.best.step_size,
      radios: group.entries.length,
      lastSeen: group.lastSeen,
    })),
  ),
)

const rankedUpstreams = computed(() =>
  upstreamScanned.value ? rankedLiveUpstreams.value : rankedKnownUpstreams.value,
)

const upstreamSplit = computed(() => splitRankedNetworks(rankedUpstreams.value))

const visibleUpstreams = computed(() =>
  showAllUpstreams.value ? rankedUpstreams.value : upstreamSplit.value.collapsed,
)

const hiddenUpstreamCount = computed(() => upstreamSplit.value.hidden)

const provisionButtonLabel = computed(() => {
  const progress = provisionProgress.value
  if (!progress) return 'Starting…'
  const running = progress.steps.find(step => step.status === 'running')
  return `${progress.percent}% · ${running?.label || progress.current || 'Starting…'}`
})

const rankedUplinkNetworks = computed(() =>
  rankNetworks(buildRankedNetworks(scannedNetworks.value, ownSsidSet.value)),
)

const uplinkSplit = computed(() => splitRankedNetworks(rankedUplinkNetworks.value))

const visibleUplinkNetworks = computed(() =>
  showAllUplinkNetworks.value
    ? rankedUplinkNetworks.value
    : uplinkSplit.value.collapsed,
)

const hiddenUplinkCount = computed(() => uplinkSplit.value.hidden)

function advertisedPrice(row: RankedNetwork): string {
  if (!row.pricePerStep) return 'no price advertised'
  return `advertised ${row.pricePerStep} sat / step`
}

function isActiveUpstream(ssid: string): boolean {
  const wanSsid = (status.value?.wan?.ssid || '').trim()
  if (wanSsid && wanSsid === ssid.trim()) return true
  return upstreamList.value.some(
    upstream => upstream.status === 'ACTIVE' && upstream.ssid === ssid,
  )
}

function useAsUplink(row: RankedNetwork) {
  startWanSetup()
  const encryption = row.encryption === 'unknown' ? 'psk2' : row.encryption
  selectedNetwork.value = {
    ssid: row.ssid,
    bssid: row.bssid ?? '',
    signal: row.signal,
    channel: row.channel ?? 0,
    encryption,
  }
  wanStep.value = encryption === 'none' ? 'dhcp' : 'password'
  nextTick(() => {
    document
      .querySelector('[data-testid="internet-uplink"]')
      ?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  })
}

function selectRankedUplink(row: RankedNetwork) {
  selectNetwork({
    ssid: row.ssid,
    bssid: row.bssid ?? '',
    signal: row.signal,
    channel: row.channel ?? 0,
    encryption: row.encryption,
  })
}

async function connectRankedUpstream(row: RankedNetwork) {
  if (!row.isTollGate) {
    useAsUplink(row)
    return
  }
  await connectUpstream({
    ssid: row.ssid,
    signal: row.signal,
    encryption: row.encryption,
    ...(row.bssid ? { bssid: row.bssid } : {}),
    ...(row.band ? { band: row.band } : {}),
    is_tollgate: true,
    ...(row.pricePerStep !== undefined
      ? { price_per_step: row.pricePerStep }
      : {}),
    ...(row.stepSize !== undefined ? { step_size: row.stepSize } : {}),
  })
}

const isDataMetering = computed(
  () => (status.value?.tollgate?.metric ?? '').toLowerCase() === 'bytes',
)

const stepSizeLabel = computed(() => {
  const t = status.value?.tollgate
  const value = t?.step_size_ms ?? 60_000
  return isDataMetering.value ? formatBytes(value) : formatDurationWords(value)
})

const meteringLabel = computed(() => (isDataMetering.value ? 'Data' : 'Time'))

const mintRows = computed<MintConfig[]>(() => {
  const t = status.value?.tollgate
  if (t?.accepted_mints?.length) return t.accepted_mints
  return t?.mint_url
    ? [{ url: t.mint_url, price_per_step: t.price_per_step ?? 0, purchase_min_steps: t.min_steps ?? 1 }]
    : []
})

const advertisedMintCount = computed(() => status.value?.tollgate?.advertised_mints?.length ?? 0)

// WAN setup flow
type WanStep = 'idle' | 'scan' | 'scanning' | 'list' | 'password' | 'dhcp' | 'connecting' | 'done'
const wanStep = ref<WanStep>('idle')
const scannedNetworks = ref<ScannedNetwork[]>([])
const selectedNetwork = ref<ScannedNetwork | null>(null)
const wanPassword = ref('')

// The node's own Wi-Fi uplink, offered as a prefilled WAN source. The
// backend returns only `has_password`; the PSK is resolved server-side when
// the operator chooses this option.
interface UpstreamWifi {
  configured: boolean
  ssid?: string
  band?: string
  security?: string
  has_password?: boolean
  profile?: string
  device?: string
  reason?: string
}
const nodeUpstream = ref<UpstreamWifi | null>(null)
const wanError = ref('')

// DHCP/masq settings (step 3 of wizard)
const dhcpStart = ref(100)
const dhcpLimit = ref(150)
const masqEnabled = ref(true)

// Force-fetch: always refetches regardless of TTL. Used by every explicit
// user action (connect, tollgate provision/save) that must reflect a fresh
// server-side state, and by onMounted below when the cache is missing/stale.
//
// When explicit params are supplied, this bypasses routerResource.refresh()
// and calls rpcClient directly instead (WR-03): resources.ts's
// store.refresh() dedupes concurrent calls for the same key, so routing an
// explicit-params call (e.g. the Connect form's submit) through it could
// silently coalesce into an unrelated refresh already in flight (e.g.
// useCachedResource's own TTL-gated onActivated/focus revalidation) —
// the caller's own params would never actually be sent, and the caller has
// no way to tell. Writing the resolved result directly into
// routerResource.entry keeps the cache/TTL/status-panel rendering
// consistent with what a normal refresh() success does.
async function load(params?: Record<string, string>, resumeReadiness = true) {
  if (!params && !selectionConfirmed.value) return
  error.value = ''
  if (params) {
    try {
      const result = await rpcClient.call<RouterStatus>({
        method: 'openwrt.get-status',
        params,
        timeout: 30000,
        maxRetries: 1,
      })
      routerResource.entry.data = result
      routerResource.entry.error = null
      routerResource.entry.fetchedAt = Date.now()
      routerResource.entry.loadState = 'ready'
      showConnectForm.value = false
      connectedParams.value = params
      syncInstallDefaults(result)
      lastUpdatedAt.value = Date.now()
      nowTick.value = Date.now()
      if (resumeReadiness) maybeResumeReadiness()
      if (
        result.tollgate.installed &&
        result.tollgate.up_to_date !== false &&
        result.tollgate.daemon_running
      ) {
        void loadAdminData()
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      if (message.includes('No router configured')) {
        showConnectForm.value = true
      } else {
        error.value = message
      }
    }
    return
  }
  await routerResource.refresh()
  const err = routerResource.error.value
  if (err) {
    if (err.includes('No router configured')) {
      showConnectForm.value = true
    } else {
      error.value = err
    }
  } else {
    showConnectForm.value = false
    const current = routerResource.data.value
    if (current) syncInstallDefaults(current)
    lastUpdatedAt.value = Date.now()
    nowTick.value = Date.now()
    if (resumeReadiness) maybeResumeReadiness()
    if (
      current?.tollgate.installed &&
      current.tollgate.up_to_date !== false &&
      current.tollgate.daemon_running
    ) {
      void loadAdminData()
    }
  }
}

function syncInstallDefaults(s: RouterStatus) {
  if (!s.tollgate.installed) return
  const metric = s.tollgate.metric === 'bytes' ? 'bytes' : 'milliseconds'
  installMetric.value = metric
  installPriceSats.value = s.tollgate.price_per_step ?? 10
  if (metric === 'bytes') {
    installStepSizeMb.value = Math.max(
      1,
      Math.round((s.tollgate.step_size_ms ?? 21 * 1024 * 1024) / (1024 * 1024)),
    )
  } else {
    installStepSizeMin.value = Math.max(
      1,
      Math.round((s.tollgate.step_size_ms ?? 60_000) / 60_000),
    )
  }
  installMinSteps.value = s.tollgate.min_steps ?? 1
  if (s.tollgate.accepted_mints?.length) {
    installMints.value = s.tollgate.accepted_mints.map(mint => ({ ...mint }))
    installShowTestMints.value = installMints.value.some(mint =>
      TEST_MINT_URLS.includes(mint.url),
    )
  }
  installPayoutAddress.value = s.tollgate.payout_address ?? ''
  editPrivateSsid.value = s.tollgate.private_ssid || 'noderunner'
}

function addInstallMint() {
  const url = installNewMint.value.trim()
  if (!url) return
  if (installMints.value.some(mint => mint.url === url)) {
    installNewMint.value = ''
    return
  }
  installMints.value.push({
    url,
    price_per_step: installPriceSats.value,
    purchase_min_steps: installMinSteps.value,
    price_unit: 'sat',
  })
  installNewMint.value = ''
}

function removeInstallMint(index: number) {
  if (installMints.value.length <= 1) return
  installMints.value.splice(index, 1)
}

function syncInstallTestMints() {
  if (installShowTestMints.value) {
    for (const url of TEST_MINT_URLS) {
      if (!installMints.value.some(mint => mint.url === url)) {
        installMints.value.push({
          url,
          price_per_step: installPriceSats.value,
          purchase_min_steps: installMinSteps.value,
          price_unit: 'sat',
        })
      }
    }
  } else {
    installMints.value = installMints.value.filter(
      mint => !TEST_MINT_URLS.includes(mint.url),
    )
  }
}

async function connect() {
  const ip = host.value.trim()
  if (!ip) return
  const router: DiscoveredRouter =
    selectedRouter.value && selectedRouter.value.ip === ip
      ? selectedRouter.value
      : { ip }
  await trySelectRouter(router, sshPassword.value, true)
}

interface WiredInterface { name: string; type: string; state: string; ipv4: string[] }

async function detectRouter() {
  detecting.value = true
  detectError.value = ''
  detectedCandidates.value = []
  try {
    const { interfaces } = await rpcClient.call<{ interfaces: WiredInterface[] }>({
      method: 'network.list-interfaces',
      timeout: 10000,
    })
    const wired = interfaces.find(i => i.type === 'ethernet' && i.state === 'up' && i.ipv4.length > 0)
    if (!wired) {
      detectError.value = 'No active wired ethernet connection found on this node.'
      return
    }
    const [ip, prefixStr] = wired.ipv4[0]!.split('/')
    const prefix = Number(prefixStr) || 24

    const { routers } = await rpcClient.call<{ routers: string[] }>({
      method: 'openwrt.scan',
      params: { subnet: ip, prefix, ssh_user: sshUser.value, ssh_password: sshPassword.value },
      timeout: 120000,
    })

    if (routers.length === 0) {
      detectError.value = `No OpenWrt router found on ${wired.name}'s network (/${prefix}).`
    } else if (routers.length === 1) {
      host.value = routers[0]!
    } else {
      detectedCandidates.value = routers
    }
  } catch (e) {
    detectError.value = e instanceof Error ? e.message : String(e)
  } finally {
    detecting.value = false
  }
}

function pickDetectedRouter(ip: string) {
  host.value = ip
  detectedCandidates.value = []
}

// The router config now persists server-side (see handle_openwrt_get_status),
// so onMounted's no-args load() always reconnects automatically — without this,
// there'd be no way back to the connect form (and its Detect button) at all.
function disconnectRouter() {
  connectedParams.value = null
  selectionConfirmed.value = false
  error.value = ''
  detectError.value = ''
  detectedCandidates.value = []
  showConnectForm.value = false
  walletInfo.value = null
  upstreamKnown.value = []
  upstreamList.value = []
  upstreamNetworks.value = []
  upstreamScanned.value = false
  mintHealth.value = null
}

async function loadInstallerStatus() {
  try {
    installerStatus.value = await rpcClient.call<InstallerStatus>({
      method: 'openwrt.installer.status',
      timeout: 15000,
    })
  } catch {
    installerStatus.value = { available: false, error: 'installer status unavailable' }
  }
}

async function discoverRouters() {
  discovering.value = true
  discoveryError.value = ''
  try {
    const result = await rpcClient.call<{
      routers: DiscoveredRouter[]
      active?: string
    }>({
      method: 'openwrt.discover',
      timeout: 120000,
    })
    discovered.value = result.routers ?? []
    discoveryActive.value = result.active ?? ''
    if (
      selectionConfirmed.value &&
      !discovered.value.some(r => r.ip === selectedRouterIp.value)
    ) {
      selectionConfirmed.value = false
    }
    if (
      !selectedRouterIp.value ||
      !discovered.value.some(r => r.ip === selectedRouterIp.value)
    ) {
      selectedRouterIp.value =
        result.active || discovered.value[0]?.ip || ''
    }
  } catch (e) {
    discoveryError.value = e instanceof Error ? e.message : String(e)
  } finally {
    discovering.value = false
  }
}

function onDiscoveryChange() {
  const router = selectedRouter.value
  if (router) void selectDiscoveredRouter(router)
}

function linkLabel(r: DiscoveredRouter): string {
  const type =
    r.link_type === 'wifi'
      ? 'WiFi'
      : r.link_type === 'ethernet'
        ? 'Ethernet'
        : 'link unknown'
  return r.interface ? `${type} · ${r.interface}` : type
}

function discoveryLabel(r: DiscoveredRouter): string {
  const name = r.saved_hostname || r.name || r.model || 'OpenWrt router'
  const link =
    r.link_type === 'wifi'
      ? 'WiFi'
      : r.link_type === 'ethernet'
        ? 'Ethernet'
        : 'link unknown'
  const model = r.saved_model || r.model || r.vendor || ''
  const credential = r.has_saved_credentials
    ? 'password saved'
    : 'password required'
  return `${name} · ${r.ip} · ${link}${model ? ` · ${model}` : ''} · ${credential}${r.active ? ' · active' : ''}`
}

async function selectDiscoveredRouter(r: DiscoveredRouter) {
  selectedRouterIp.value = r.ip
  host.value = r.ip
  sshUser.value = 'root'
  sshPassword.value = ''
  detectedCandidates.value = []
  detectError.value = ''
  error.value = ''
  showConnectForm.value = false
  needsPassword.value = false
  trustRouterError.value = ''
  trustRouterDone.value = false
  walletInfo.value = null
  upstreamKnown.value = []
  upstreamList.value = []
  upstreamNetworks.value = []
  upstreamScanned.value = false
  mintHealth.value = null
  showMintHealth.value = false
  await trySelectRouter(r, '')
}

async function trySelectRouter(
  r: DiscoveredRouter,
  password: string,
  passwordProvided = false,
) {
  connecting.value = true
  selectionConfirmed.value = false
  error.value = ''
  try {
    const result = await rpcClient.call<SelectRouterResult>({
      method: 'openwrt.select-router',
      params: {
        host: r.ip,
        ssh_user: 'root',
        ssh_password: password,
        password_provided: passwordProvided,
        ...(r.mac ? { mac: r.mac } : {}),
        ...(r.model ? { model: r.model } : {}),
        ...(r.name ? { name: r.name } : {}),
      },
      timeout: 30000,
    })
    if (!result.ok) {
      needsPassword.value = true
      showConnectForm.value = true
      error.value = result.needs_root_password
        ? result.error ||
          'Fresh OpenWrt has no root password. Set one on the router or in the flash form.'
        : result.bad_password
          ? 'That root password was refused. Enter the router root password.'
          : result.error || 'This router needs a root password.'
      return
    }
    needsPassword.value = false
    showConnectForm.value = false
    selectionConfirmed.value = true
    if (password) {
      await load({
        host: r.ip,
        ssh_user: result.username || 'root',
        ssh_password: password,
      })
    } else {
      await load()
    }
    await discoverRouters()
  } catch (e) {
    selectionConfirmed.value = false
    error.value = e instanceof Error ? e.message : String(e)
    showConnectForm.value = true
    needsPassword.value = true
  } finally {
    connecting.value = false
  }
}

async function loadAdminData() {
  if (!adminReady.value) return
  await Promise.all([loadWallet(), loadUpstreams()])
}

async function loadWallet() {
  walletLoading.value = true
  walletError.value = ''
  try {
    walletInfo.value = await rpcClient.call<WalletInfo>({
      method: 'openwrt.wallet-info',
      timeout: 30000,
    })
  } catch (e) {
    walletError.value = e instanceof Error ? e.message : String(e)
    walletInfo.value = null
  } finally {
    walletLoading.value = false
  }
}

async function checkDaemonReady(): Promise<boolean> {
  try {
    await load(undefined, false)
    await loadWallet()
  } catch {
    return false
  }
  lastUpdatedAt.value = Date.now()
  nowTick.value = Date.now()
  const t = status.value?.tollgate
  const advertised = t?.advertised_mints?.length ?? 0
  const configured = t?.configured_mints ?? 0
  return (
    !!t?.installed &&
    t.daemon_running === true &&
    advertised > 0 &&
    advertised >= configured &&
    walletInfo.value?.ok === true &&
    walletInfo.value?.degraded !== true
  )
}

/// Resume the readiness poll after a reload or a router reconnect when the
/// daemon is still advertising its mints. Without this the page keeps a
/// snapshot from a 1-minute-old daemon forever and reports "wallet degraded".
function maybeResumeReadiness(): void {
  if (readinessActive || readinessExhausted.value) return
  const t = status.value?.tollgate
  if (!t?.installed || t.daemon_running !== true) return
  const configured = t.configured_mints ?? 0
  const advertised = t.advertised_mints?.length ?? 0
  if (configured === 0 || advertised >= configured) return
  void waitForDaemonReady()
}

/// A just-restarted daemon needs a few seconds to probe and advertise its
/// mints. Without this poll the first status snapshot after install looks
/// like a permanent "wallet degraded" state until the operator reloads.
///
/// `force` restarts a poll after the 3-minute window was exhausted (manual
/// Refresh). Automatic callers never force, so a daemon that genuinely cannot
/// advertise its mints falls back to the real mint-health warning instead of
/// refreshing forever.
async function waitForDaemonReady(force = false) {
  if (readinessActive) return readinessRun ?? Promise.resolve()
  if (readinessExhausted.value && !force) return
  if (force) readinessExhausted.value = false

  const startedAt = Date.now()
  daemonStarting.value = true
  readinessActive = true
  readinessRun = (async () => {
    let attempt = 0
    try {
      while (!gatewayDisposed && Date.now() - startedAt < READINESS_WINDOW_MS) {
        if (await checkDaemonReady()) return
        attempt += 1
        const remaining = READINESS_WINDOW_MS - (Date.now() - startedAt)
        if (remaining <= 0) break
        await new Promise(resolve =>
          setTimeout(resolve, Math.min(attempt < 15 ? 2000 : 5000, remaining)),
        )
      }
      readinessExhausted.value = true
    } finally {
      daemonStarting.value = false
      readinessActive = false
      readinessRun = null
    }
  })()
  return readinessRun
}

/// Manual refresh: fetch once, then let the readiness poll continue if the
/// daemon still has mints to advertise.
async function refreshGateway() {
  readinessExhausted.value = false
  await load(undefined, false)
  if (adminReady.value) await loadWallet()
  lastUpdatedAt.value = Date.now()
  nowTick.value = Date.now()
  maybeResumeReadiness()
}

async function previewDrain() {
  drainPreviewLoading.value = true
  drainPreviewError.value = ''
  try {
    drainPreview.value = await rpcClient.call<DrainPreview>({
      method: 'openwrt.wallet-drain-preview',
      timeout: 60000,
    })
    drainModal.value = true
  } catch (e) {
    drainPreviewError.value = e instanceof Error ? e.message : String(e)
  } finally {
    drainPreviewLoading.value = false
  }
}

async function addMissingMints() {
  const preview = drainPreview.value
  if (!preview?.node) return
  const missing = (preview.not_accepted ?? []).map(mint => mint.url)
  if (missing.length === 0) return
  if (
    !window.confirm(
      `Add ${missing.length} mint${missing.length === 1 ? '' : 's'} to the Archy wallet's accepted list?\n\n${missing.join(
        '\n',
      )}\n\nThis changes which Cashu tokens the node will accept.`,
    )
  ) {
    return
  }
  drainMintAdding.value = true
  drainMintError.value = ''
  try {
    const union = Array.from(
      new Set([...(preview.node.accepted_mints ?? []), ...missing]),
    )
    await rpcClient.call({
      method: 'streaming.configure-mints',
      params: { mints: union },
      timeout: 30000,
    })
    await previewDrain()
  } catch (e) {
    drainMintError.value = e instanceof Error ? e.message : String(e)
  } finally {
    drainMintAdding.value = false
  }
}

async function confirmDrain() {
  if (
    !window.confirm(
      'Drain ALL router wallet funds to this Archy wallet?\n\nThis removes the funds from the router and is irreversible. Tokens the node cannot accept remain in the router drain journal.',
    )
  ) {
    return
  }
  drainExecuting.value = true
  drainPreviewError.value = ''
  try {
    const result = await rpcClient.call<DrainResult>({
      method: 'openwrt.wallet-drain-to-node',
      params: { confirm: true },
      timeout: 300000,
    })
    drainResult.value = result
    drainModal.value = false
    drainResultModal.value = true
    await loadWallet()
    await load()
  } catch (e) {
    drainPreviewError.value = e instanceof Error ? e.message : String(e)
  } finally {
    drainExecuting.value = false
  }
}

async function copyText(value: string) {
  try {
    await navigator.clipboard.writeText(value)
  } catch {
    // Clipboard permissions are optional; the text is still selectable.
  }
}

function hasFlashAddress(link: FlashLink): boolean {
  return link.ipv4.some(addr => addr.startsWith('192.168.1.'))
}

const preferredFlashLink = computed(
  () =>
    flashInfo.value?.links?.find(
      link => link.preferred && link.physical && !link.wireless,
    ) ?? null,
)

function canPrepareFlashLink(link: FlashLink): boolean {
  if (!link.physical || link.wireless || !link.carrier || hasFlashAddress(link)) {
    return false
  }
  const preferred = preferredFlashLink.value
  return preferred ? link.name === preferred.name : true
}

const flashBlockReason = computed(() => {
  const info = flashInfo.value
  if (!info) return 'Inspect the image first.'
  if (!info.ok) return info.error || 'Could not inspect the router image.'
  if (!info.board) return 'Detect the router board first.'
  if (!info.flash_ready) {
    return preferredFlashLink.value
      ? `Add 192.168.1.2/24 to ${preferredFlashLink.value.name} (the router link) to enable flashing.`
      : 'Add a 192.168.1.x address to the physical wired router link to enable flashing.'
  }
  if (flashConfirmation.value.trim() !== info.board) {
    return `Type ${info.board} to confirm.`
  }
  if (!flashAcknowledge.value) {
    return 'Acknowledge that the router will be erased.'
  }
  return ''
})

function clearFlashTimer() {
  if (flashTimer !== null) {
    clearInterval(flashTimer)
    flashTimer = null
  }
}

async function openFlashModal() {
  flashOpen.value = true
  flashVersion.value = '25.12.5'
  flashPassword.value = sshPassword.value || ''
  flashNewPassword.value = ''
  flashInfo.value = null
  flashInfoError.value = ''
  flashConfirmation.value = ''
  flashAcknowledge.value = false
  flashJob.value = null
  flashJobId.value = ''
  flashError.value = ''
  await inspectFlashImage()
}

async function inspectFlashImage() {
  const ip = flashHost.value
  if (!ip) {
    flashInfoError.value = 'No router is selected.'
    return
  }
  flashInfoLoading.value = true
  flashInfoError.value = ''
  flashInfo.value = null
  try {
    const params: Record<string, unknown> = {
      host: ip,
      version: flashVersion.value,
    }
    if (flashPassword.value.trim()) params.ssh_password = flashPassword.value.trim()
    const info = await rpcClient.call<CleanSlateInfo>({
      method: 'openwrt.installer.clean-slate-info',
      params,
      timeout: 90000,
    })
    flashInfo.value = info
    if (!info.ok && !info.needs_trust) {
      flashInfoError.value = info.error || 'Could not inspect the router.'
    }
  } catch (e) {
    flashInfoError.value = e instanceof Error ? e.message : String(e)
  } finally {
    flashInfoLoading.value = false
  }
}

async function trustFlashHost() {
  const info = flashInfo.value
  if (!info?.ssh_fingerprint) return
  flashTrustPending.value = true
  flashError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.installer.trust',
      params: { ip: info.host, fingerprint: info.ssh_fingerprint },
      timeout: 30000,
    })
    await inspectFlashImage()
  } catch (e) {
    flashError.value = e instanceof Error ? e.message : String(e)
  } finally {
    flashTrustPending.value = false
  }
}

function flashRecoveryTarget(): string {
  return flashJob.value?.ip || '192.168.1.1'
}

async function recheckFlashedRouter() {
  const ip = flashRecoveryTarget()
  flashRecoveryLoading.value = true
  flashRecoveryError.value = ''
  flashRecoveryInfo.value = null
  try {
    const params: Record<string, unknown> = { ip }
    if (flashPassword.value.trim()) params.password = flashPassword.value.trim()
    const info = await rpcClient.call<{
      identified?: boolean
      firmware?: string
      model?: string
      name?: string
      ssh_fingerprint?: string
    }>({
      method: 'openwrt.installer.identify',
      params,
      timeout: 30000,
    })
    flashRecoveryInfo.value = { ip, ...info }
  } catch (e) {
    flashRecoveryError.value = e instanceof Error ? e.message : String(e)
  } finally {
    flashRecoveryLoading.value = false
  }
}

async function trustRecoveredRouter() {
  const info = flashRecoveryInfo.value
  if (!info?.ssh_fingerprint) return
  flashRecoveryTrusting.value = true
  flashRecoveryError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.installer.trust',
      params: { ip: info.ip, fingerprint: info.ssh_fingerprint },
      timeout: 30000,
    })
    await recheckFlashedRouter()
  } catch (e) {
    flashRecoveryError.value = e instanceof Error ? e.message : String(e)
  } finally {
    flashRecoveryTrusting.value = false
  }
}

async function prepareFlashLink(link: FlashLink) {
  if (
    !window.confirm(
      `Add 192.168.1.2/24 to ${link.name} so the flashed router at 192.168.1.1 is reachable?`,
    )
  ) {
    return
  }
  flashInfoError.value = ''
  try {
    const result = await rpcClient.call<{
      ok: boolean
      error?: string
      command?: string
    }>({
      method: 'openwrt.installer.prepare-flash-link',
      params: { interface: link.name },
      timeout: 30000,
    })
    if (!result.ok) {
      flashInfoError.value = result.error || 'Could not add the address.'
      if (result.command) {
        flashInfoError.value += ` Run: ${result.command}`
      }
      return
    }
    await inspectFlashImage()
  } catch (e) {
    flashInfoError.value = e instanceof Error ? e.message : String(e)
  }
}

async function startFlash() {
  const info = flashInfo.value
  if (!info?.ok || !info.board || !flashCanStart.value) return
  if (
    !window.confirm(
      `Flash vanilla OpenWrt ${flashVersion.value} onto ${info.host}?\n\nThis erases ALL configuration. The router will only answer on 192.168.1.1 on its LAN.`,
    )
  ) {
    return
  }
  flashRunning.value = true
  flashError.value = ''
  flashJob.value = null
  flashJobId.value = ''
  try {
    const params: Record<string, unknown> = {
      confirm: true,
      host: info.host,
      version: flashVersion.value,
      confirmation: flashConfirmation.value.trim(),
    }
    if (flashNewPassword.value.trim()) {
      params.new_password = flashNewPassword.value.trim()
    }
    if (flashPassword.value.trim()) params.ssh_password = flashPassword.value.trim()
    const result = await rpcClient.call<{
      ok: boolean
      job_id?: string
      error?: string
      message?: string
      needs_wired_link?: boolean
    }>({
      method: 'openwrt.installer.clean-slate-flash',
      params,
      timeout: 60000,
    })
    if (!result.ok || !result.job_id) {
      flashError.value = result.message || result.error || 'Could not start the flash.'
      flashRunning.value = false
      return
    }
    flashJobId.value = result.job_id
    flashJob.value = { status: 'running', steps: [] }
    clearFlashTimer()
    flashTimer = setInterval(async () => {
      if (!flashJobId.value) return
      try {
        const job = await rpcClient.call<CleanSlateJob>({
          method: 'openwrt.installer.clean-slate-status',
          params: { job_id: flashJobId.value },
          timeout: 30000,
        })
        flashJob.value = job
        if (['done', 'failed', 'pending'].includes(job.status)) {
          clearFlashTimer()
          flashRunning.value = false
          if (job.status === 'failed') {
            flashError.value = job.error || 'The flash failed.'
          }
        }
      } catch (e) {
        clearFlashTimer()
        flashRunning.value = false
        flashError.value = e instanceof Error ? e.message : String(e)
      }
    }, 2000)
  } catch (e) {
    flashError.value = e instanceof Error ? e.message : String(e)
    flashRunning.value = false
  }
}

function closeFlashModal() {
  if (flashRunning.value) return
  clearFlashTimer()
  flashOpen.value = false
}

async function useFlashedRouter() {
  const ip = flashRecoveryInfo.value?.ip || flashJob.value?.ip || '192.168.1.1'
  const newPassword = flashNewPassword.value.trim()
  clearFlashTimer()
  flashOpen.value = false
  selectionConfirmed.value = false
  connectedParams.value = null
  selectedRouterIp.value = ''
  showConnectForm.value = true
  host.value = ip
  error.value = ''
  // A clean-slate flash wipes the root password, so the pre-flash credential
  // must not be reused for the reconnect. An explicit new password is
  // prefilled; a blank one clears the stale value.
  sshPassword.value = newPassword
  await rpcClient
    .call({
      method: 'openwrt.forget-router',
      params: {
        host: ip,
        ...(selectedRouter.value?.mac ? { mac: selectedRouter.value.mac } : {}),
      },
      timeout: 10000,
    })
    .catch(() => {})
  await discoverRouters()
}

onUnmounted(clearFlashTimer)
onUnmounted(stopProvisionPolling)
onUnmounted(() => {
  gatewayDisposed = true
  if (clockTimer !== null) {
    clearInterval(clockTimer)
    clockTimer = null
  }
})

async function loadUpstreams() {
  upstreamLoading.value = true
  upstreamError.value = ''
  try {
    const [list, known] = await Promise.all([
      rpcClient.call<{ upstreams: UpstreamSTA[] }>({
        method: 'openwrt.upstream-list',
        timeout: 30000,
      }),
      rpcClient.call<{ known: { known_tollgates?: KnownTollGate[] } }>({
        method: 'openwrt.upstream-known',
        timeout: 30000,
      }),
    ])
    upstreamList.value = list.upstreams ?? []
    const allKnown = known.known?.known_tollgates ?? []
    // The discovery registry carries regular APs too; only show names that
    // look like TollGates or carry a price hint.
    upstreamKnown.value = allKnown.filter(
      k =>
        (k.price_per_step ?? 0) > 0 ||
        /^!?(TollGate|Archipelago|Net4sats)(-|$)/i.test(k.ssid || ''),
    )
  } catch (e) {
    upstreamError.value = e instanceof Error ? e.message : String(e)
  } finally {
    upstreamLoading.value = false
  }
}

async function scanUpstreams() {
  upstreamLoading.value = true
  upstreamError.value = ''
  showAllUpstreams.value = false
  try {
    const result = await rpcClient.call<{ networks: UpstreamNetwork[] }>({
      method: 'openwrt.upstream-scan',
      timeout: 120000,
    })
    upstreamNetworks.value = result.networks ?? []
    upstreamScanned.value = true
  } catch (e) {
    upstreamError.value = e instanceof Error ? e.message : String(e)
  } finally {
    upstreamLoading.value = false
  }
}

async function connectUpstream(
  net: UpstreamNetwork,
  opts?: { skipConfirm?: boolean },
) {
  if (
    !opts?.skipConfirm &&
    !window.confirm(
      `Connect the router's upstream to "${net.ssid}"? This changes the router's upstream and may interrupt active sessions.`,
    )
  ) {
    return
  }
  upstreamLoading.value = true
  upstreamError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.upstream-connect',
      params: {
        ssid: net.ssid,
        passphrase: net.encryption === 'none' ? '' : upstreamPassphrase.value,
      },
      timeout: 180000,
    })
    upstreamPassphrase.value = ''
    await loadUpstreams()
  } catch (e) {
    upstreamError.value = e instanceof Error ? e.message : String(e)
  } finally {
    upstreamLoading.value = false
  }
}

async function removeUpstream(ssid: string) {
  if (!window.confirm(`Remove disabled upstream "${ssid}"?`)) return
  upstreamLoading.value = true
  upstreamError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.upstream-remove',
      params: { ssid },
      timeout: 30000,
    })
    await loadUpstreams()
  } catch (e) {
    upstreamError.value = e instanceof Error ? e.message : String(e)
  } finally {
    upstreamLoading.value = false
  }
}

function scrollToUpstream() {
  const el = document.getElementById('upstream-gateways')
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  } else {
    error.value = 'Install or upgrade TollGate first to manage its upstream.'
  }
}

async function trustSelectedRouter() {
  const r = selectedRouter.value
  if (!r?.ssh_fingerprint) return
  trustRouterPending.value = true
  trustRouterError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.installer.trust',
      params: { ip: r.ip, fingerprint: r.ssh_fingerprint },
      timeout: 30000,
    })
    trustRouterDone.value = true
    await discoverRouters()
    if (selectionConfirmed.value) {
      await load()
      if (status.value?.tollgate.installed) void waitForDaemonReady()
    }
  } catch (e) {
    trustRouterError.value = e instanceof Error ? e.message : String(e)
  } finally {
    trustRouterPending.value = false
  }
}

function normalizeMintUrl(url: string): string {
  return url.trim().replace(/\/+$/, '').toLowerCase()
}

function mintHost(url: string): string {
  const match = url.match(/^[a-z]+:\/\/([^/]+)/i)
  return match?.[1] ?? url
}

function formatLastSeen(iso?: string): string {
  if (!iso) return 'unknown'
  const parsed = Date.parse(iso)
  if (!Number.isFinite(parsed)) return 'unknown'
  const minutes = Math.max(0, Math.floor((Date.now() - parsed) / 60_000))
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  return `${Math.floor(hours / 24)}d ago`
}

async function checkMints() {
  mintHealthLoading.value = true
  mintHealthError.value = ''
  try {
    mintHealth.value = await rpcClient.call<MintHealth>({
      method: 'openwrt.mint-health',
      params: authParams(),
      timeout: 120000,
    })
    showMintHealth.value = true
  } catch (e) {
    mintHealthError.value = e instanceof Error ? e.message : String(e)
  } finally {
    mintHealthLoading.value = false
  }
}

async function pruneUnreachableMints() {
  const urls = unreachableMints.value.map(m => m.url)
  if (urls.length === 0) return
  if (urls.length >= mintRows.value.length) {
    mintHealthError.value =
      'Refusing to remove every mint; keep at least one working mint.'
    return
  }
  if (
    !window.confirm(
      `Remove ${urls.length} unreachable mint${urls.length === 1 ? '' : 's'}?\n\n${urls.join(
        '\n',
      )}\n\nThis does not delete wallet.db or any existing ecash.`,
    )
  ) {
    return
  }
  mintHealthLoading.value = true
  mintHealthError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.prune-mints',
      params: { ...authParams(), urls },
      timeout: 120000,
    })
    mintHealth.value = null
    await load()
  } catch (e) {
    mintHealthError.value = e instanceof Error ? e.message : String(e)
  } finally {
    mintHealthLoading.value = false
  }
}

async function repairPortal() {
  repairingPortal.value = true
  portalError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.repair-portal',
      params: authParams(),
      timeout: 120000,
    })
    await load()
  } catch (e) {
    portalError.value = e instanceof Error ? e.message : String(e)
  } finally {
    repairingPortal.value = false
  }
}

async function applyNetworkNames() {
  if (
    !window.confirm(
      'Apply archipelago-tollgate-<code> and noderunner-<code> to this router?\n\nThis briefly restarts Wi-Fi and the TollGate daemon.',
    )
  ) {
    return
  }
  applyingNetworkNames.value = true
  networkNamesError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.apply-network-names',
      params: authParams(),
      timeout: 180000,
    })
    await load()
  } catch (e) {
    networkNamesError.value = e instanceof Error ? e.message : String(e)
  } finally {
    applyingNetworkNames.value = false
  }
}

async function provisionTollgate() {
  provisionError.value = ''
  provisionSuccess.value = false
  provisionSkippedMints.value = []
  if (installBlockedNoWan.value) {
    provisionError.value = installUplinkMessage.value
    scrollToUplink()
    return
  }
  if (!installPayoutAddress.value.trim()) {
    provisionError.value =
      'A Lightning payout address is required by the official installer (where TollGate sends your share).'
    return
  }
  if (installMints.value.length === 0) {
    provisionError.value = 'At least one accepted mint is required.'
    return
  }
  const stepSize =
    installMetric.value === 'bytes'
      ? Math.max(1, installStepSizeMb.value) * 1024 * 1024
      : Math.max(1, installStepSizeMin.value) * 60_000
  const params: Record<string, unknown> = {
    ...authParams(),
    price_sats: installPriceSats.value,
    step_size_ms: stepSize,
    metric: installMetric.value,
    min_steps: installMinSteps.value,
    accepted_mints: installMints.value.map(mint => ({
      url: mint.url.trim(),
      price_per_step: Math.max(1, installPriceSats.value),
      price_unit: 'sat',
      purchase_min_steps: Math.max(1, installMinSteps.value),
    })),
    payout_address: installPayoutAddress.value.trim(),
    private_ssid: editPrivateSsid.value || 'noderunner',
  }

  provisioning.value = true
  try {
    // The installer owns host-key trust. Ask it to identify the router first;
    // an untrusted key comes back as `ssh_fingerprint` and the operator must
    // confirm it on the router's console before we retry.
    const ip = host.value.trim() || status.value?.host || ''
    const pw = sshPassword.value || String(authParams().ssh_password ?? '')
    if (ip) {
      const identity = await rpcClient
        .call<{ ssh_fingerprint?: string }>({
          method: 'openwrt.installer.identify',
          params: { ip, password: pw },
          timeout: 30000,
        })
        .catch(() => null)
      if (identity?.ssh_fingerprint) {
        trustFingerprint.value = identity.ssh_fingerprint
        pendingProvision.value = params
        trustRequired.value = true
        return
      }
    }
    await doProvision(params)
  } catch (e) {
    provisionError.value = e instanceof Error ? e.message : String(e)
  } finally {
    if (!trustRequired.value) provisioning.value = false
  }
}

function newProvisionId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return `provision-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

function stopProvisionPolling() {
  if (provisionTimer !== null) {
    clearInterval(provisionTimer)
    provisionTimer = null
  }
}

async function refreshProvisionProgress(id: string) {
  try {
    const result = await rpcClient.call<{
      ok: boolean
      progress?: ProvisionProgress | null
    }>({
      method: 'openwrt.provision-progress',
      params: { provision_id: id },
      timeout: 10000,
    })
    if (result?.progress) {
      provisionProgress.value = result.progress
      if (result.progress.done || result.progress.failed) {
        provisioning.value = false
        stopProvisionPolling()
      }
    } else if (provisionProgress.value?.phase === 'starting') {
      // The backend refused before it created a progress entry (for example
      // the no-WAN gate). Hide the placeholder; the error explains why.
      provisionProgress.value = null
    }
  } catch {
    // The long provision request carries the authoritative error. A missed
    // poll must not surface a second, confusing failure.
  }
}

function startProvisionPolling(id: string) {
  stopProvisionPolling()
  void refreshProvisionProgress(id)
  provisionTimer = setInterval(() => {
    void refreshProvisionProgress(id)
  }, 1000)
}

/// Restore a running install after a page reload. Without this, the progress
/// panel and polling only exist while the original install RPC is open in the
/// same page.
async function resumeProvisionProgress() {
  try {
    const result = await rpcClient.call<{
      ok: boolean
      progress?: ProvisionProgress | null
    }>({
      method: 'openwrt.provision-progress',
      params: {},
      timeout: 10000,
    })
    const progress = result?.progress
    if (!progress) return
    provisionProgress.value = progress
    if (!progress.done && !progress.failed && progress.provision_id) {
      provisioning.value = true
      startProvisionPolling(progress.provision_id)
    }
  } catch {
    // No active install to resume.
  }
}

function syncActiveHost(resultHost?: string) {
  if (!resultHost) return
  host.value = resultHost
  if (connectedParams.value) {
    connectedParams.value = { ...connectedParams.value, host: resultHost }
  }
}

async function runProvisionRequest(
  method: 'openwrt.provision-tollgate' | 'openwrt.apply-config',
  params: Record<string, unknown>,
  timeout: number,
) {
  const provisionId = newProvisionId()
  // Show the panel immediately; the first poll replaces this placeholder with
  // the installer's real step list.
  provisionProgress.value = {
    provision_id: provisionId,
    kind: method === 'openwrt.apply-config' ? 'apply' : 'install',
    phase: 'starting',
    percent: 0,
    current: 'Starting…',
    steps: [],
    done: false,
    failed: false,
  }
  startProvisionPolling(provisionId)
  try {
    const result = await rpcClient.call<{
      ok?: boolean
      host?: string
      host_key_untrusted?: boolean
      ssh_fingerprint?: string
      skipped_mints?: string[]
      needs_uplink?: boolean
      message?: string
      error?: string
    }>({
      method,
      params: { ...params, provision_id: provisionId },
      timeout,
    })
    if (result?.host_key_untrusted && result.ssh_fingerprint) {
      trustFingerprint.value = result.ssh_fingerprint
      pendingProvision.value = params
      trustRequired.value = true
      return
    }
    if (result?.needs_uplink) {
      provisionError.value =
        result.message || 'Configure the internet uplink before installing TollGate.'
      scrollToUplink()
      return
    }
    if (result?.ok === false) {
      provisionError.value =
        result.message || result.error || 'TollGate provisioning failed.'
      return
    }
    provisionSkippedMints.value = result?.skipped_mints ?? []
    provisionSuccess.value = true
    syncActiveHost(result?.host)
    await load(connectedParams.value ?? undefined)
    await loadInstallerStatus()
    void waitForDaemonReady()
  } finally {
    await refreshProvisionProgress(provisionId)
    stopProvisionPolling()
  }
}

async function doProvision(params: Record<string, unknown>) {
  await runProvisionRequest('openwrt.provision-tollgate', params, 900000)
}

async function retryProvision() {
  if (status.value?.tollgate.installed) {
    await reapplyConfig()
  } else {
    await provisionTollgate()
  }
}

function currentTollgateParams(): Record<string, unknown> | null {
  const tg = status.value?.tollgate
  if (!tg) return null
  const mints = (tg.accepted_mints ?? []).map(mint => ({
    url: mint.url,
    price_per_step: mint.price_per_step,
    price_unit: mint.price_unit || 'sat',
    purchase_min_steps: mint.purchase_min_steps,
    ...(mint.min_balance !== undefined ? { min_balance: mint.min_balance } : {}),
    ...(mint.balance_tolerance_percent !== undefined
      ? { balance_tolerance_percent: mint.balance_tolerance_percent }
      : {}),
    ...(mint.payout_interval_seconds !== undefined
      ? { payout_interval_seconds: mint.payout_interval_seconds }
      : {}),
    ...(mint.min_payout_amount !== undefined
      ? { min_payout_amount: mint.min_payout_amount }
      : {}),
  }))
  return {
    ...authParams(),
    price_sats: tg.price_per_step ?? 10,
    step_size_ms: tg.step_size_ms ?? 60_000,
    metric: tg.metric === 'bytes' ? 'bytes' : 'milliseconds',
    min_steps: tg.min_steps ?? 1,
    ...(mints.length ? { accepted_mints: mints } : {}),
    payout_address: tg.payout_address ?? '',
    private_ssid: tg.private_ssid || 'noderunner',
    enabled: tg.enabled ?? true,
  }
}

async function reapplyConfig() {
  const params = currentTollgateParams()
  if (!params) return
  updatingTollgate.value = true
  updateTollgateError.value = ''
  provisionError.value = ''
  try {
    await runProvisionRequest('openwrt.apply-config', params, 300000)
  } catch (e) {
    updateTollgateError.value = e instanceof Error ? e.message : String(e)
  } finally {
    updatingTollgate.value = false
  }
}

async function trustAndInstall() {
  const params = pendingProvision.value
  if (!params) return
  trustPending.value = true
  provisionError.value = ''
  try {
    const ip = host.value.trim() || status.value?.host || ''
    await rpcClient.call({
      method: 'openwrt.installer.trust',
      params: { ip, fingerprint: trustFingerprint.value },
      timeout: 30000,
    })
    trustRequired.value = false
    await doProvision({ ...params, trust_fingerprint: trustFingerprint.value })
    pendingProvision.value = null
  } catch (e) {
    provisionError.value = e instanceof Error ? e.message : String(e)
  } finally {
    trustPending.value = false
    provisioning.value = false
  }
}

function cancelTrust() {
  trustRequired.value = false
  pendingProvision.value = null
  trustFingerprint.value = ''
  provisioning.value = false
}

function startEditTollgate() {
  const tg = status.value?.tollgate
  const metric = tg?.metric === 'bytes' ? 'bytes' : 'milliseconds'
  editMetric.value = metric
  editPriceSats.value = tg?.price_per_step ?? 10
  if (metric === 'bytes') {
    editStepSizeMb.value = Math.max(
      1,
      Math.round((tg?.step_size_ms ?? 21 * 1024 * 1024) / (1024 * 1024)),
    )
  } else {
    editStepSizeMin.value = Math.max(1, Math.round((tg?.step_size_ms ?? 60000) / 60000))
  }
  editMinSteps.value = tg?.min_steps ?? 1
  const listed: MintConfig[] = tg?.accepted_mints?.length
    ? tg.accepted_mints
    : [{
        url: tg?.mint_url ?? '',
        price_per_step: tg?.price_per_step ?? 10,
        purchase_min_steps: tg?.min_steps ?? 1,
        price_unit: 'sat',
      }]
  editMints.value = listed.map(m => ({ ...m }))
  mintTest.value = {}
  editPayoutAddress.value = tg?.payout_address ?? ''
  editPrivateSsid.value = tg?.private_ssid || 'noderunner'
  editPrivateKey.value = ''
  editEnabled.value = tg?.enabled ?? true
  updateTollgateError.value = ''
  editingTollgate.value = true
}

async function saveTollgateConfig() {
  if (editMints.value.length === 0 || editMints.value.some(m => !m.url.trim())) {
    updateTollgateError.value = 'At least one mint with a URL is required'
    return
  }
  updatingTollgate.value = true
  updateTollgateError.value = ''
  try {
    const stepSize =
      editMetric.value === 'bytes'
        ? Math.max(1, editStepSizeMb.value) * 1024 * 1024
        : Math.max(1, editStepSizeMin.value) * 60_000
    const params: Record<string, unknown> = {
      ...authParams(),
      price_sats: editPriceSats.value,
      step_size_ms: stepSize,
      metric: editMetric.value,
      min_steps: editMinSteps.value,
      mint_url: editMints.value[0]?.url.trim() ?? '',
      accepted_mints: editMints.value.map(m => ({
        url: m.url.trim(),
        price_per_step: m.price_per_step,
        price_unit: m.price_unit || 'sat',
        purchase_min_steps: m.purchase_min_steps,
        ...(m.min_balance !== undefined ? { min_balance: m.min_balance } : {}),
        ...(m.balance_tolerance_percent !== undefined ? { balance_tolerance_percent: m.balance_tolerance_percent } : {}),
        ...(m.payout_interval_seconds !== undefined ? { payout_interval_seconds: m.payout_interval_seconds } : {}),
        ...(m.min_payout_amount !== undefined ? { min_payout_amount: m.min_payout_amount } : {}),
      })),
      payout_address: editPayoutAddress.value,
      private_ssid: editPrivateSsid.value,
      ...(editPrivateKey.value ? { private_key: editPrivateKey.value } : {}),
      enabled: editEnabled.value,
    }
    await runProvisionRequest('openwrt.apply-config', params, 300000)
    editingTollgate.value = false
    mintHealth.value = null
    showMintHealth.value = false
    await load(connectedParams.value ?? undefined)
  } catch (e) {
    updateTollgateError.value = e instanceof Error ? e.message : String(e)
  } finally {
    updatingTollgate.value = false
  }
}

function addMint() {
  editMints.value.push({
    url: '',
    price_per_step: editPriceSats.value,
    purchase_min_steps: editMinSteps.value,
    price_unit: 'sat',
  })
}

function removeMint(index: number) {
  if (editMints.value.length <= 1) return
  editMints.value.splice(index, 1)
  delete mintTest.value[index]
}

async function testMint(index: number) {
  const mint = editMints.value[index]
  if (!mint?.url.trim()) {
    mintTest.value[index] = { status: 'fail', detail: 'URL is empty' }
    return
  }
  mintTest.value[index] = { status: 'testing', detail: 'Probing from the router…' }
  try {
    const res = await rpcClient.call<{
      ok: boolean
      name?: string
      version?: string
      active_sat_keyset?: boolean
      input_fee_ppk?: number
    }>({
      method: 'openwrt.test-mint',
      params: { ...authParams(), url: mint.url.trim() },
      timeout: 30000,
    })
    mintTest.value[index] = res.ok
      ? {
          status: 'ok',
          detail: `${res.name || 'mint'}${res.version ? ` ${res.version}` : ''} · sat keyset · fee ${res.input_fee_ppk ?? 0} ppk`,
        }
      : { status: 'fail', detail: 'No active sat keyset' }
  } catch (e) {
    mintTest.value[index] = {
      status: 'fail',
      detail: e instanceof Error ? e.message : String(e),
    }
  }
}

/// Re-apply the current values, which also runs the stop/kill/start lifecycle
/// on the backend. Used when the running daemon is stale (unreachable or a
/// different version from the installed package).
async function restartTollgate() {
  const params = currentTollgateParams()
  if (!params) return
  updatingTollgate.value = true
  updateTollgateError.value = ''
  try {
    await runProvisionRequest('openwrt.apply-config', params, 300000)
    mintHealth.value = null
    showMintHealth.value = false
    await load(connectedParams.value ?? undefined)
  } catch (e) {
    updateTollgateError.value = e instanceof Error ? e.message : String(e)
  } finally {
    updatingTollgate.value = false
  }
}

function startWanSetup() {
  wanStep.value = 'scan'
  wanError.value = ''
  showAllUplinkNetworks.value = false
  scannedNetworks.value = []
  selectedNetwork.value = null
  wanPassword.value = ''
  nodeUpstream.value = null
  dhcpStart.value = Number(status.value?.wan?.dhcp_start) || 100
  dhcpLimit.value = Number(status.value?.wan?.dhcp_limit) || 150
  masqEnabled.value = true
  void loadNodeUpstream()
}

async function loadNodeUpstream() {
  try {
    const result = await rpcClient.call<UpstreamWifi>({
      method: 'network.upstream-wifi',
      timeout: 10000,
    })
    nodeUpstream.value =
      result?.configured && result.ssid && result.has_password ? result : null
  } catch {
    // A wired node, an enterprise uplink or an unreadable secret all land
    // here on purpose: the manual scan/password path is still available.
    nodeUpstream.value = null
  }
}

function dismissNodeUpstream() {
  nodeUpstream.value = null
}

async function connectUsingNodeCredentials() {
  const up = nodeUpstream.value
  if (!up?.ssid) return
  wanStep.value = 'connecting'
  wanError.value = ''
  try {
    await rpcClient.call({
      method: 'openwrt.configure-wan',
      params: {
        ...authParams(),
        ssid: up.ssid,
        password: '',
        use_node_credentials: true,
        encryption: up.security || 'psk2',
        dhcp_start: dhcpStart.value,
        dhcp_limit: dhcpLimit.value,
        masq: masqEnabled.value,
      },
      timeout: 30000,
    })
    wanStep.value = 'done'
    setTimeout(() => {
      wanStep.value = 'idle'
      load(connectedParams.value ?? undefined)
    }, 8000)
  } catch (e) {
    wanError.value = e instanceof Error ? e.message : String(e)
    wanStep.value = 'scan'
  }
}

async function scanWifi() {
  wanStep.value = 'scanning'
  wanError.value = ''
  showAllUplinkNetworks.value = false
  try {
    const params: Record<string, unknown> = { ...authParams() }
    const result = await rpcClient.call<{ networks: ScannedNetwork[] }>({
      method: 'openwrt.scan-wifi',
      params,
      timeout: 30000,
    })
    scannedNetworks.value = result.networks
    wanStep.value = 'list'
  } catch (e) {
    wanError.value = e instanceof Error ? e.message : String(e)
    wanStep.value = 'scan'
  }
}

function selectNetwork(net: ScannedNetwork) {
  selectedNetwork.value = net
  wanPassword.value = ''
  wanError.value = ''
  wanStep.value = 'password'
}

async function configureWan() {
  if (!selectedNetwork.value) return
  wanStep.value = 'connecting'
  wanError.value = ''
  try {
    const params: Record<string, unknown> = {
      ...authParams(),
      ssid: selectedNetwork.value.ssid,
      password: wanPassword.value,
      encryption: selectedNetwork.value.encryption,
      dhcp_start: dhcpStart.value,
      dhcp_limit: dhcpLimit.value,
      masq: masqEnabled.value,
    }
    await rpcClient.call({ method: 'openwrt.configure-wan', params, timeout: 30000 })
    wanStep.value = 'done'
    // Give the router ~8s to associate before reloading status
    setTimeout(() => {
      wanStep.value = 'idle'
      load(connectedParams.value ?? undefined)
    }, 8000)
  } catch (e) {
    wanError.value = e instanceof Error ? e.message : String(e)
    wanStep.value = 'dhcp'
  }
}

function signalBars(dbm: number): number {
  if (dbm >= -50) return 4
  if (dbm >= -65) return 3
  if (dbm >= -75) return 2
  return 1
}

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const parts = []
  if (d) parts.push(`${d}d`)
  if (h) parts.push(`${h}h`)
  parts.push(`${m}m`)
  return parts.join(' ')
}

// Human spellings for the summary. "m" alone is ambiguous (minute vs
// millisecond), which is exactly how "1 sat / 1m" next to "Metric:
// milliseconds" was read two ways.
function formatDurationWords(ms: number): string {
  if (ms >= 3_600_000 && ms % 3_600_000 === 0) {
    const hours = ms / 3_600_000
    return `${hours} hour${hours === 1 ? '' : 's'}`
  }
  if (ms >= 60_000 && ms % 60_000 === 0) {
    const minutes = ms / 60_000
    return `${minutes} minute${minutes === 1 ? '' : 's'}`
  }
  if (ms >= 1_000 && ms % 1_000 === 0) {
    const seconds = ms / 1_000
    return `${seconds} second${seconds === 1 ? '' : 's'}`
  }
  return `${ms} milliseconds`
}

function trimNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1)
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return '0 B'
  const mib = bytes / (1024 * 1024)
  if (mib >= 1024) return `${trimNumber(mib / 1024)} GB`
  if (mib >= 1) return `${trimNumber(mib)} MB`
  const kib = bytes / 1024
  if (kib >= 1) return `${trimNumber(kib)} KB`
  return `${bytes} B`
}

const openwrtVersion = computed(() => {
  if (!status.value) return ''
  const r = status.value.release
  return (
    r.openwrt_release ||
    r.distrib_description ||
    r.openwrt_version ||
    r.distrib_release ||
    ''
  )
})

const boardName = computed(() => {
  if (!status.value) return ''
  return status.value.release.openwrt_board_name || ''
})

// Discovery only. The saved active router is preselected but not connected:
// status, wallet, upstream and schema RPCs must wait for an explicit
// `openwrt.select-router` success.
onMounted(() => {
  clockTimer = setInterval(() => {
    nowTick.value = Date.now()
  }, 5000)
  void loadInstallerStatus()
  void discoverRouters()
  void resumeProvisionProgress()
})
</script>

<template>
  <div class="pb-32 sm:pb-12">
    <!-- mb-0 overrides the pill's default mb-4, which pushed it above the
         title's centerline in this inline header row. -->
    <div class="flex items-center gap-3 mb-6">
      <BackButton desktop-margin="mb-0" @click="goBack" />
      <h1 class="text-lg font-semibold text-white">OpenWrt Gateway</h1>
    </div>

    <!-- Discovered gateways -->
    <div class="glass-card p-6 mb-6" data-testid="discovered-gateways">
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-sm font-semibold text-white/80">Discovered gateways</h2>
        <div class="flex items-center gap-3">
          <label class="flex items-center gap-1 text-[11px] text-white/40">
            <input v-model="showUnverifiedRouters" type="checkbox" />
            Show unverified
          </label>
          <button
            class="text-xs text-white/40 hover:text-white"
            :disabled="discovering"
            @click="discoverRouters"
          >
            {{ discovering ? 'Scanning…' : 'Rescan' }}
          </button>
        </div>
      </div>
      <p v-if="discoveryError" class="text-xs text-red-400">{{ discoveryError }}</p>
      <div
        v-if="discovering && visibleDiscovered.length === 0"
        class="text-sm text-white/50"
      >
        Scanning the node's LAN interfaces…
      </div>
      <div
        v-else-if="visibleDiscovered.length === 0 && !discoveryError"
        class="text-sm text-white/50"
      >
        No manageable gateways found.
      </div>
      <template v-else>
        <select
          v-model="selectedRouterIp"
          data-testid="gateway-select"
          class="w-full px-3 py-2.5 bg-transparent border border-white/20 rounded-lg text-white text-sm focus:outline-none focus:border-white/40"
          @change="onDiscoveryChange"
        >
          <option value="">Select a gateway…</option>
          <option v-for="r in visibleDiscovered" :key="r.ip" :value="r.ip">
            {{ discoveryLabel(r) }}
          </option>
        </select>
        <div v-if="selectedRouter" class="mt-3 text-xs text-white/50 space-y-1">
          <div>
            <span class="text-white/30">Address:</span>
            <span class="font-mono">{{ selectedRouter.ip }}</span>
            <span v-if="selectedRouter.mac" class="font-mono text-white/30"> · {{ selectedRouter.mac }}</span>
          </div>
          <div>
            <span class="text-white/30">Link:</span> {{ linkLabel(selectedRouter) }}
          </div>
          <div>
            <span class="text-white/30">Model:</span>
            {{ selectedRouter.saved_model || selectedRouter.model || selectedRouter.vendor || 'unknown (password required)' }}
            <span
              v-if="selectedRouter.firmware && selectedRouter.firmware.toLowerCase() !== 'unknown'"
            >
              · {{ selectedRouter.firmware }}
            </span>
          </div>
          <div v-if="selectedRouter.has_saved_credentials" class="text-green-300">
            Root password saved for this router.
          </div>
          <div v-else class="text-white/40">
            Root password required.
          </div>
          <div v-if="!selectedRouter.ssh_open" class="text-white/40">
            No SSH — this host cannot be managed.
          </div>

          <div
            v-if="selectedRouter.ssh_open && selectedRouter.ssh_fingerprint"
            class="mt-2 rounded-lg border border-yellow-400/20 bg-yellow-400/5 p-3 space-y-2"
            data-testid="trust-host-key"
          >
            <div class="text-yellow-300">
              TollGate installer host key is not trusted yet.
            </div>
            <pre class="text-[11px] font-mono bg-black/30 rounded p-2 text-green-300 break-all whitespace-pre-wrap">{{ selectedRouter.ssh_fingerprint }}</pre>
            <div class="text-white/40">Verify on the router console:</div>
            <code class="block text-[11px] font-mono bg-black/30 rounded p-2 text-white/70 break-all whitespace-pre-wrap">dropbearkey -y -f /etc/dropbear/dropbear_ed25519_host_key</code>
            <button
              class="glass-button glass-button-warning text-xs px-3 py-1.5"
              :disabled="trustRouterPending"
              :class="trustRouterPending ? 'opacity-40 cursor-not-allowed' : ''"
              data-testid="trust-host-key-button"
              @click="trustSelectedRouter"
            >
              {{ trustRouterPending ? 'Trusting…' : 'Trust host key' }}
            </button>
            <p v-if="trustRouterError" class="text-red-400">{{ trustRouterError }}</p>
          </div>
          <div v-else-if="selectedRouter.ssh_open" class="text-green-300">
            Host key trusted by the TollGate installer.
          </div>
          <div v-if="trustRouterDone && !selectedRouter.ssh_fingerprint" class="text-green-300">
            Host key trusted.
          </div>

          <button
            v-if="!selectionConfirmed"
            class="glass-button glass-button-success mt-2 text-xs px-3 py-1.5"
            :disabled="connecting"
            :class="connecting ? 'opacity-40 cursor-not-allowed' : ''"
            data-testid="connect-gateway"
            @click="selectDiscoveredRouter(selectedRouter)"
          >
            {{ connecting ? 'Connecting…' : 'Connect to this gateway' }}
          </button>
        </div>
      </template>
    </div>

    <!-- Connect form -->
    <template v-if="showConnectForm && !selectionConfirmed">
    <div class="glass-card p-6 mb-6">
      <h2 class="text-sm font-semibold text-white/80 mb-4">Connect to Router</h2>
      <div class="space-y-3">
        <div>
          <label class="block text-xs text-white/40 mb-1">Router IP</label>
          <div class="flex items-center gap-2">
            <input
              v-model="host"
              type="text"
              placeholder="192.168.1.1"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 focus:ring-1 focus:ring-white/20 transition-colors"
            />
            <button
              :disabled="detecting"
              class="glass-button px-4 py-3 text-sm font-medium whitespace-nowrap flex-shrink-0"
              :class="detecting ? 'opacity-40 cursor-not-allowed' : ''"
              @click="detectRouter"
            >
              {{ detecting ? 'Detecting…' : 'Detect' }}
            </button>
          </div>
          <p v-if="detectError" class="mt-2 text-xs text-red-400">{{ detectError }}</p>
          <div v-if="detectedCandidates.length > 0" class="mt-2 space-y-1">
            <p class="text-xs text-white/40">Multiple routers found — pick one:</p>
            <button
              v-for="ip in detectedCandidates"
              :key="ip"
              class="w-full text-left px-3 py-2 bg-white/5 hover:bg-white/10 rounded-lg text-sm text-white font-mono transition-colors"
              @click="pickDetectedRouter(ip)"
            >
              {{ ip }}
            </button>
          </div>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block text-xs text-white/40 mb-1">SSH User</label>
            <input
              v-model="sshUser"
              type="text"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40 focus:ring-1 focus:ring-white/20 transition-colors"
            />
          </div>
          <div v-if="needsPassword || !selectedRouter?.has_saved_credentials">
            <label class="block text-xs text-white/40 mb-1">Password</label>
            <input
              v-model="sshPassword"
              type="password"
              placeholder="(blank for none)"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 focus:ring-1 focus:ring-white/20 transition-colors"
            />
            <p class="mt-1 text-[11px] text-white/30">
              A fresh OpenWrt flash has no root password. Set one on the router
              or in the flash form before connecting.
            </p>
          </div>
        </div>
        <p
          v-if="selectedRouter?.has_saved_credentials && !needsPassword"
          class="text-xs text-green-300"
        >
          Using the saved root password. If it is refused, the password field appears.
        </p>
        <button
          :disabled="connecting || !host.trim()"
          class="glass-button w-full text-sm font-medium"
          :class="connecting || !host.trim() ? 'opacity-40 cursor-not-allowed' : 'glass-button-warning'"
          @click="connect"
        >
          {{ connecting ? 'Connecting…' : 'Connect' }}
        </button>
      </div>
      <p v-if="error" class="mt-3 text-xs text-red-400">{{ error }}</p>
    </div>

    <div ref="routerCard" class="glass-card p-6 mb-6 overflow-hidden router-model-card" :style="routerCardHeight ? { height: routerCardHeight } : undefined">
      <div class="grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_minmax(280px,360px)] gap-6 items-start">
        <DeviceModel v-if="!routerShopOpen" model="beryl-ax" label="Beryl AX (GL-MT3000) OpenWrt router" fill-viewport :controls="false" />
        <HardwareMarketplace :expanded="routerShopOpen" @open="openRouterShop" @close="closeRouterShop" />
      </div>
    </div>
    <div class="h-16" aria-hidden="true"></div>

    </template>

    <!-- Loading skeleton -->
    <template v-else-if="loading">
      <div v-for="i in 3" :key="i" class="glass-card p-6 mb-4 animate-pulse">
        <div class="h-4 bg-white/10 rounded w-1/3 mb-4"></div>
        <div class="space-y-2">
          <div class="h-3 bg-white/10 rounded w-2/3"></div>
          <div class="h-3 bg-white/10 rounded w-1/2"></div>
        </div>
      </div>
    </template>

    <!-- Error state -->
    <div v-else-if="error" class="glass-card p-6 mb-4">
      <p class="text-sm text-red-300">{{ error }}</p>
      <div class="mt-3 flex items-center gap-4">
        <button class="text-xs text-white/50 hover:text-white transition-colors underline" @click="refreshGateway">Retry</button>
        <button class="text-xs text-orange-300/80 hover:text-orange-200 transition-colors underline" @click="disconnectRouter">Reconfigure router</button>
      </div>
    </div>

    <!-- No selection yet -->
    <div
      v-else-if="!selectionConfirmed"
      class="glass-card p-6 mb-4 text-sm text-white/50"
      data-testid="select-gateway-prompt"
    >
      Select a gateway above to load its status, installer and admin panel.
    </div>

    <!-- Status panels -->
    <template v-else-if="status">

      <!-- System info -->
      <div class="glass-card p-6 mb-4">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-sm font-semibold text-white/80">System</h2>
          <span class="flex items-center gap-1.5 text-xs text-green-400">
            <span class="w-1.5 h-1.5 rounded-full bg-green-400 inline-block"></span>
            Connected
          </span>
        </div>
        <dl class="grid grid-cols-2 gap-x-6 gap-y-3 text-sm">
          <div>
            <dt class="text-xs text-white/40 mb-0.5">Hostname</dt>
            <dd class="text-white font-medium">{{ status.hostname }}</dd>
          </div>
          <div>
            <dt class="text-xs text-white/40 mb-0.5">Address</dt>
            <dd class="text-white font-medium font-mono">{{ status.host }}</dd>
          </div>
          <div v-if="openwrtVersion">
            <dt class="text-xs text-white/40 mb-0.5">OpenWrt</dt>
            <dd class="text-white/70">{{ openwrtVersion }}</dd>
          </div>
          <div v-if="boardName">
            <dt class="text-xs text-white/40 mb-0.5">Board</dt>
            <dd class="text-white/70">{{ boardName }}</dd>
          </div>
          <div>
            <dt class="text-xs text-white/40 mb-0.5">Uptime</dt>
            <dd class="text-white/70">{{ formatUptime(status.uptime_secs) }}</dd>
          </div>
        </dl>
        <div class="mt-4 flex items-center gap-4">
          <button class="text-xs text-white/40 hover:text-white/70 transition-colors" @click="refreshGateway">
            ↻ Refresh
          </button>
          <span
            v-if="lastUpdatedLabel"
            class="text-xs text-white/30"
            data-testid="gateway-updated"
          >
            Updated {{ lastUpdatedLabel }}
          </span>
          <button
            v-if="adminReady"
            class="text-xs text-white/40 hover:text-white/70 transition-colors"
            @click="scrollToUpstream"
          >
            Router upstream →
          </button>
        </div>
      </div>

      <!-- Internet uplink (regular Wi-Fi/WAN, separate from TollGate upstreams) -->
      <div v-if="showUplink" class="glass-card p-6 mb-4" data-testid="internet-uplink">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-sm font-semibold text-white/80">Internet uplink</h2>
          <button
            v-if="wanStep === 'idle' || wanStep === 'done'"
            class="text-xs text-white/40 hover:text-white transition-colors"
            data-testid="configure-uplink"
            @click="startWanSetup"
          >
            {{ status.wan?.configured ? 'Reconfigure →' : 'Set up →' }}
          </button>
        </div>

        <!-- Current status (idle) -->
        <template v-if="wanStep === 'idle' || wanStep === 'done'">
          <div v-if="status.wan?.configured" class="flex items-center gap-3 mb-3">
            <span class="w-2 h-2 rounded-full inline-block flex-shrink-0"
              :class="status.wan.internet ? 'bg-green-400' : 'bg-yellow-400 animate-pulse'"></span>
            <div>
              <div class="text-sm font-medium text-white">
                {{ status.wan.ssid || (status.wan.configured ? 'Ethernet' : 'Not configured') }}
              </div>
              <div class="text-xs text-white/40">
                {{ status.wan.internet ? status.wan.ip : 'Connecting…' }}
              </div>
            </div>
          </div>
          <div v-else class="text-sm text-white/50 mb-3">
            Not configured — router has no internet access.
          </div>

          <!-- Quick summary of DHCP + masq when connected -->
          <dl v-if="status.wan?.configured && status.wan.internet"
              class="grid grid-cols-3 gap-x-4 gap-y-1 text-xs text-white/40 mb-3">
            <div>
              <dt class="text-white/25">LAN</dt>
              <dd class="text-white/60 font-mono">{{ status.wan.lan_ip }}/24</dd>
            </div>
            <div>
              <dt class="text-white/25">DHCP</dt>
              <dd class="text-white/60 font-mono">.{{ status.wan.dhcp_start }}+{{ status.wan.dhcp_limit }}</dd>
            </div>
            <div>
              <dt class="text-white/25">NAT</dt>
              <dd :class="status.wan.masq ? 'text-green-400' : 'text-red-400'">
                {{ status.wan.masq ? 'on' : 'off' }}
              </dd>
            </div>
          </dl>

          <!-- Diagnostics (shown when not connected) -->
          <dl v-if="status.wan?.configured && !status.wan.internet"
              class="grid grid-cols-2 gap-x-4 gap-y-2 text-xs mt-1 mb-3 border-t border-white/10 pt-3">
            <div>
              <dt class="text-white/30 mb-0.5">Radio</dt>
              <dd :class="status.wan.radio0_disabled ? 'text-red-400' : 'text-green-400'">
                {{ status.wan.radio0_disabled ? 'disabled' : 'enabled' }}
              </dd>
            </div>
            <div>
              <dt class="text-white/30 mb-0.5">Interface</dt>
              <dd class="text-white/70 font-mono">
                {{ status.wan.sta_iface || 'none' }}
                <span v-if="status.wan.sta_iface" class="ml-1"
                  :class="status.wan.sta_state === 'up' ? 'text-green-400' : 'text-yellow-400'">
                  ({{ status.wan.sta_state }})
                </span>
              </dd>
            </div>
            <div>
              <dt class="text-white/30 mb-0.5">Associated to</dt>
              <dd class="text-white/70">{{ status.wan.assoc_ssid || 'none' }}</dd>
            </div>
            <div>
              <dt class="text-white/30 mb-0.5">Configured SSID</dt>
              <dd class="text-white/70">{{ status.wan.ssid || '—' }}</dd>
            </div>
            <div v-if="status.wan.wifi_log" class="col-span-2">
              <dt class="text-white/30 mb-1">Recent wifi log</dt>
              <dd class="font-mono text-white/50 text-[10px] leading-relaxed whitespace-pre-wrap break-all bg-black/20 rounded p-2">{{ status.wan.wifi_log }}</dd>
            </div>
          </dl>

          <p v-if="wanStep === 'done'" class="mt-2 text-xs text-green-400">
            WAN configured. Router is connecting…
          </p>
        </template>

        <!-- Step: scan prompt -->
        <template v-else-if="wanStep === 'scan'">
          <div
            v-if="nodeUpstream"
            class="mb-3 p-3 rounded-lg border border-white/10 bg-white/5"
          >
            <div class="text-xs text-white/40 mb-1">This node's Wi-Fi uplink</div>
            <div class="text-sm text-white font-medium">{{ nodeUpstream.ssid }}</div>
            <div class="text-xs text-white/40 mt-0.5">
              Saved password on this node
              <span v-if="nodeUpstream.band">· {{ nodeUpstream.band }} GHz</span>
            </div>
            <div class="flex items-center gap-3 mt-2">
              <button
                class="glass-button glass-button-warning text-xs px-3 py-1.5"
                @click="connectUsingNodeCredentials"
              >
                Use these credentials
              </button>
              <button class="text-xs text-white/40 hover:text-white" @click="dismissNodeUpstream">
                Change
              </button>
            </div>
          </div>
          <button
            class="glass-button glass-button-warning w-full text-sm font-medium"
            data-testid="scan-uplink"
            @click="scanWifi"
          >
            Scan for Networks
          </button>
          <p v-if="wanError" class="mt-2 text-xs text-red-400">{{ wanError }}</p>
          <button class="mt-3 text-xs text-white/40 hover:text-white" @click="wanStep = 'idle'">Cancel</button>
        </template>

        <!-- Step: scanning -->
        <template v-else-if="wanStep === 'scanning'">
          <div class="flex items-center gap-3 py-2">
            <svg class="animate-spin w-4 h-4 text-white/50" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8z"/>
            </svg>
            <span class="text-sm text-white/50">Scanning for networks…</span>
          </div>
        </template>

        <!-- Step: network list -->
        <template v-else-if="wanStep === 'list'">
          <p class="text-xs text-white/40 mb-3">
            {{ rankedUplinkNetworks.length }} network{{ rankedUplinkNetworks.length !== 1 ? 's' : '' }} found. Select one:
          </p>
          <div v-if="rankedUplinkNetworks.length === 0" class="text-sm text-white/50">
            No networks found. Try scanning again.
          </div>
          <div class="space-y-1.5">
            <button
              v-for="net in visibleUplinkNetworks"
              :key="net.ssid"
              class="w-full flex items-center justify-between px-3 py-2.5 rounded-lg border border-white/10 hover:border-white/30 hover:bg-white/5 transition-colors text-left"
              data-testid="uplink-network"
              :data-ssid="net.ssid"
              :data-tollgate="net.isTollGate ? 'true' : 'false'"
              @click="selectRankedUplink(net)"
            >
              <div class="flex items-center gap-2.5">
                <!-- Signal bars -->
                <div class="flex items-end gap-0.5 h-4">
                  <div v-for="b in 4" :key="b"
                    class="w-1 rounded-sm"
                    :class="[
                      b * 1 <= signalBars(net.signal) ? 'bg-white/70' : 'bg-white/15',
                      b === 1 ? 'h-1' : b === 2 ? 'h-2' : b === 3 ? 'h-3' : 'h-4'
                    ]"
                  ></div>
                </div>
                <span class="text-white text-sm">{{ net.ssid }}</span>
                <span v-if="net.isTollGate"
                  class="text-xs px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-300">TollGate</span>
                <span v-if="net.encryption !== 'none'" class="text-white/30 text-xs">🔒</span>
              </div>
              <span class="text-xs text-white/30 font-mono flex-shrink-0">
                {{ net.radios > 1 ? `${net.radios} radios` : `ch${net.channel ?? 0}` }}
              </span>
            </button>
          </div>
          <button
            v-if="hiddenUplinkCount > 0"
            class="mt-2 text-xs text-white/40 hover:text-white"
            data-testid="uplink-show-more"
            @click="showAllUplinkNetworks = !showAllUplinkNetworks"
          >
            {{ showAllUplinkNetworks ? 'Show less' : `Show ${hiddenUplinkCount} more` }}
          </button>
          <button class="mt-3 text-xs text-white/40 hover:text-white" @click="wanStep = 'scan'">← Scan again</button>
        </template>

        <!-- Step: password entry -->
        <template v-else-if="wanStep === 'password'">
          <p class="text-sm text-white mb-3">
            Connect to <span class="font-semibold">{{ selectedNetwork?.ssid }}</span>
          </p>
          <input
            v-if="selectedNetwork?.encryption !== 'none'"
            v-model="wanPassword"
            type="password"
            data-testid="uplink-password"
            placeholder="WiFi password"
            class="w-full px-4 py-3 mb-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            @keyup.enter="wanStep = 'dhcp'"
          />
          <div class="flex items-center gap-2">
            <button
              class="glass-button glass-button-success flex-1 text-sm font-medium"
              :disabled="selectedNetwork?.encryption !== 'none' && !wanPassword"
              :class="selectedNetwork?.encryption !== 'none' && !wanPassword ? 'opacity-40 cursor-not-allowed' : ''"
              @click="wanStep = 'dhcp'"
            >
              Next →
            </button>
            <button
              class="text-xs text-white/40 hover:text-white px-3 py-2"
              data-testid="uplink-back"
              @click="wanStep = 'list'"
            >
              ←
            </button>
          </div>
          <p v-if="wanError" class="mt-2 text-xs text-red-400">{{ wanError }}</p>
        </template>

        <!-- Step: DHCP + masquerade config -->
        <template v-else-if="wanStep === 'dhcp'">
          <p class="text-xs text-white/50 mb-4">
            Configure how this router hands out addresses to WiFi clients.
          </p>

          <!-- LAN info -->
          <div class="mb-4 text-xs font-mono bg-black/20 rounded-lg px-3 py-2 text-white/50">
            <span class="text-white/30">LAN: </span>{{ status?.wan?.lan_ip || '192.168.1.1' }}/24
          </div>

          <!-- DHCP range -->
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">DHCP range for clients</label>
            <div class="flex items-center gap-2 text-sm">
              <span class="text-white/40 font-mono text-xs">{{ status?.wan?.lan_ip?.split('.').slice(0,3).join('.') || '192.168.1' }}.</span>
              <input
                v-model.number="dhcpStart"
                type="number"
                min="2" max="250"
                class="w-20 px-2 py-1.5 bg-transparent border border-white/20 rounded text-white text-sm text-center focus:outline-none focus:border-white/40"
              />
              <span class="text-white/30">–</span>
              <span class="text-white/60 font-mono text-xs">{{ Math.min(254, dhcpStart + dhcpLimit - 1) }}</span>
            </div>
            <p class="text-xs text-white/30 mt-1">{{ dhcpLimit }} addresses</p>
          </div>

          <!-- Masquerade -->
          <div class="flex items-center justify-between mb-4 py-3 border-t border-white/10">
            <div>
              <div class="text-sm text-white">Enable NAT masquerade</div>
              <div class="text-xs text-white/40">Routes LAN traffic through the WiFi uplink</div>
            </div>
            <button
              class="relative w-11 h-6 rounded-full transition-colors flex-shrink-0"
              :class="masqEnabled ? 'bg-green-500/60' : 'bg-white/15'"
              @click="masqEnabled = !masqEnabled"
            >
              <span
                class="absolute top-0.5 w-5 h-5 rounded-full bg-white transition-transform"
                :class="masqEnabled ? 'translate-x-5' : 'translate-x-0.5'"
              ></span>
            </button>
          </div>

          <p v-if="wanError" class="mb-3 text-xs text-red-400">{{ wanError }}</p>
          <div class="flex items-center gap-2">
            <button
              class="glass-button glass-button-success flex-1 text-sm font-medium"
              @click="configureWan"
            >
              Apply Settings
            </button>
            <button class="text-xs text-white/40 hover:text-white px-3 py-2" @click="wanStep = 'password'">←</button>
          </div>
        </template>

        <!-- Step: applying -->
        <template v-else-if="wanStep === 'connecting'">
          <div class="flex items-center gap-3 py-2">
            <svg class="animate-spin w-4 h-4 text-white/50" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8z"/>
            </svg>
            <span class="text-sm text-white/50">Applying WAN config…</span>
          </div>
        </template>
      </div>

      <!-- TollGate -->
      <div class="glass-card p-6 mb-4" data-testid="tollgate-card">
        <h2 class="text-sm font-semibold text-white/80 mb-4">TollGate</h2>

        <div class="mb-3 text-xs">
          <span class="text-white/40">Engine: </span>
          <span v-if="installerStatus?.available" class="text-green-300">
            official {{ installerStatus.installer_version }}
            <span v-if="installerStatus.feed_release_tag" class="text-white/40">
              · {{ installerStatus.feed_release_tag }}
            </span>
          </span>
          <span v-else class="text-yellow-300">
            tollgate-installer unavailable
            <span v-if="installerStatus?.error" class="text-white/40"> — {{ installerStatus.error }}</span>
          </span>
        </div>

        <ProvisionProgressPanel
          v-if="provisionProgress && provisionProgress.kind === 'apply'"
          :progress="provisionProgress"
          @retry="retryProvision"
        />

        <div v-if="installerNeeded">
          <div class="flex items-center gap-3 mb-4">
            <span class="w-2 h-2 rounded-full bg-white/20 inline-block"></span>
            <span class="text-sm text-white/50">
              {{ status.tollgate.installed ? 'Update available' : 'Not installed' }}
            </span>
          </div>
          <div v-if="status.tollgate.installed" class="mb-3 text-xs text-yellow-400/80 bg-yellow-400/10 rounded-lg px-3 py-2">
            Installed <span class="font-mono">{{ status.tollgate.version || 'unknown' }}</span>,
            latest <span class="font-mono">{{ status.tollgate.pinned_version || 'unknown' }}</span>.
          </div>
          <div
            v-if="installBlockedNoWan"
            class="mb-3 text-xs text-yellow-400/80 bg-yellow-400/10 rounded-lg px-3 py-2"
            data-testid="install-blocked-no-wan"
          >
            {{ installUplinkMessage }}
            <button class="ml-1 underline text-yellow-200 hover:text-yellow-100" @click="scrollToUplink">
              Configure uplink
            </button>
          </div>
          <div class="grid grid-cols-2 lg:grid-cols-4 gap-3 mb-3">
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">Metering</span>
              <select v-model="installMetric"
                class="w-full px-3 py-2 bg-black/30 border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40">
                <option value="milliseconds">Time</option>
                <option value="bytes">Data</option>
              </select>
            </label>
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">
                {{ installMetric === 'bytes' ? 'Price (sats / step)' : 'Price (sats / min)' }}
              </span>
              <input v-model.number="installPriceSats" type="number" min="1"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40" />
            </label>
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">
                {{ installMetric === 'bytes' ? 'Step (MB)' : 'Step (minutes)' }}
              </span>
              <input v-if="installMetric === 'bytes'" v-model.number="installStepSizeMb" type="number" min="1"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40" />
              <input v-else v-model.number="installStepSizeMin" type="number" min="1"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40" />
            </label>
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">Min steps</span>
              <input v-model.number="installMinSteps" type="number" min="1"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40" />
            </label>
          </div>
          <div class="mb-3 rounded-lg border border-white/10 bg-white/[0.02] p-3">
            <div class="flex items-center justify-between mb-2">
              <span class="block text-xs text-white/40">Accepted mints</span>
              <label class="flex items-center gap-1 text-[11px] text-white/40">
                <input
                  v-model="installShowTestMints"
                  type="checkbox"
                  @change="syncInstallTestMints"
                />
                Include test mints
              </label>
            </div>
            <div
              v-for="(mint, index) in installMints"
              :key="mint.url"
              class="flex items-center justify-between gap-2 py-0.5 text-xs"
            >
              <span class="font-mono break-all text-white/70">{{ mint.url }}</span>
              <button
                class="text-red-300/80 hover:text-red-200 whitespace-nowrap"
                :disabled="installMints.length <= 1"
                @click="removeInstallMint(index)"
              >
                Remove
              </button>
            </div>
            <div class="flex gap-2 mt-2">
              <input
                v-model="installNewMint"
                type="text"
                placeholder="https://mint.example"
                class="flex-1 px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 text-xs focus:outline-none focus:border-white/40"
                @keydown.enter.prevent="addInstallMint"
              />
              <button class="glass-button px-3 py-2 text-xs" @click="addInstallMint">Add</button>
            </div>
            <p class="mt-1 text-[11px] text-white/30">
              Unreachable mints are skipped before the daemon starts; the five defaults are the production set.
            </p>
          </div>
          <label class="block mb-3">
            <span class="block text-xs text-white/40 mb-1">Payout Lightning address (required)</span>
            <input v-model="installPayoutAddress" type="text" placeholder="you@wallet.example"
              class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40" />
            <span class="block text-[11px] text-white/30 mt-1">
              Where TollGate sends the operator share. The official installer requires it.
            </span>
          </label>
          <ProvisionProgressPanel
            v-if="provisionProgress && provisionProgress.kind === 'install'"
            :progress="provisionProgress"
            @retry="retryProvision"
          />
          <button
            :disabled="provisioning || installBlockedNoWan"
            class="glass-button glass-button-success w-full text-sm font-medium"
            :class="provisioning || installBlockedNoWan ? 'opacity-40 cursor-not-allowed' : ''"
            data-testid="install-tollgate"
            @click="provisionTollgate"
          >
            {{
              provisioning
                ? provisionButtonLabel
                : status.tollgate.installed
                  ? 'Upgrade TollGate'
                  : 'Install TollGate'
            }}
          </button>
          <p v-if="provisionError" class="mt-3 text-xs text-red-400">{{ provisionError }}</p>
          <p v-if="provisionSuccess && !provisioning" class="mt-3 text-xs text-green-400">TollGate provisioned successfully.</p>
          <p v-if="provisionSkippedMints.length" class="mt-2 text-xs text-yellow-300">
            Skipped unreachable mints:
            <span class="font-mono">{{ provisionSkippedMints.join(', ') }}</span>
          </p>
        </div>

        <template v-else-if="!editingTollgate">
          <div class="flex items-center justify-between mb-4">
            <div class="flex items-center gap-2" data-testid="tollgate-enabled">
              <span class="w-2 h-2 rounded-full inline-block"
                :class="tollgateActive ? 'bg-green-400' : 'bg-yellow-400'"></span>
              <span class="text-sm" :class="tollgateActive ? 'text-green-300' : 'text-yellow-300'">
                {{ tollgateStateLabel }}
              </span>
            </div>
            <div class="flex items-center gap-3">
              <button
                v-if="networkNamesOutdated && status.wifi_interfaces.length > 0"
                class="text-xs text-yellow-300 hover:text-yellow-200"
                :disabled="applyingNetworkNames"
                @click="applyNetworkNames"
              >
                {{ applyingNetworkNames ? 'Renaming…' : 'Apply network names' }}
              </button>
              <button
                v-if="restartRequired"
                class="text-xs text-yellow-300 hover:text-yellow-200"
                :disabled="updatingTollgate"
                @click="restartTollgate"
              >
                Restart TollGate
              </button>
              <button
                class="text-xs text-white/40 hover:text-white"
                :disabled="updatingTollgate"
                data-testid="reapply-config"
                @click="reapplyConfig"
              >
                {{ updatingTollgate ? 'Applying…' : 'Reapply config' }}
              </button>
              <button class="text-xs text-white/40 hover:text-white" @click="startEditTollgate">Edit</button>
            </div>
          </div>
          <p v-if="networkNamesError" class="mb-3 text-xs text-red-400">{{ networkNamesError }}</p>
          <div
            v-if="status.tollgate.nodogsplash_ok === false"
            class="mb-3 rounded-lg border border-red-400/20 bg-red-400/5 p-3 text-xs text-red-300"
          >
            The paid-network portal is not gating <span class="font-mono">br-tollgate</span>
            (current: <span class="font-mono">{{ status.tollgate.nodogsplash_gateway || 'none' }}</span>).
            <button
              class="ml-2 text-white/60 hover:text-white underline"
              :disabled="repairingPortal"
              data-testid="repair-portal"
              @click="repairPortal"
            >
              {{ repairingPortal ? 'Repairing…' : 'Repair paid-network gating' }}
            </button>
            <p v-if="portalError" class="mt-1 text-red-300">{{ portalError }}</p>
          </div>

          <div class="text-sm text-white/70" data-testid="mint-summary">
            <span v-if="daemonWarmingUp" class="text-white/50">Starting —</span>
            Advertising {{ advertisedMintCount }} of {{ mintRows.length }} mints
            <span class="text-white/30">·</span>
            <span class="font-mono">{{ daemonVersionLabel }}</span>
          </div>
          <div class="mt-1 text-xs text-white/50">
            {{ status.tollgate.price_per_step }} {{ status.tollgate.currency || 'sat' }} / {{ stepSizeLabel }}
            <span class="text-white/30">·</span>
            {{ meteringLabel }}
            <span v-if="!isDataMetering">({{ status.tollgate.step_size_ms ?? 60000 }} ms)</span>
          </div>
          <div
            v-if="lastUpdatedLabel"
            class="mt-1 text-[11px] text-white/30"
            data-testid="tollgate-updated"
          >
            Updated {{ lastUpdatedLabel }}
            <span v-if="daemonWarmingUp">· the daemon is still advertising mints</span>
          </div>

          <details class="mt-3" data-testid="tollgate-details">
            <summary class="cursor-pointer text-xs text-white/40 hover:text-white/70" data-testid="details-summary">
              Details
            </summary>
            <dl class="grid grid-cols-2 gap-x-6 gap-y-3 text-sm mt-3">
              <div>
                <dt class="text-xs text-white/40 mb-0.5">Price</dt>
                <dd class="text-white font-medium">
                  {{ status.tollgate.price_per_step }} {{ status.tollgate.currency || 'sat' }} per {{ stepSizeLabel }}
                </dd>
              </div>
              <div>
                <dt class="text-xs text-white/40 mb-0.5">Metering</dt>
                <dd class="text-white/70">
                  {{ meteringLabel }} · step {{ stepSizeLabel }}
                  <span v-if="!isDataMetering">({{ status.tollgate.step_size_ms ?? 60000 }} ms)</span>
                </dd>
              </div>
              <div class="col-span-2">
                <dt class="text-xs text-white/40 mb-0.5">Mints</dt>
                <dd class="text-white/70 text-xs">
                  <div>
                    {{ advertisedMintCount }} of {{ mintRows.length }} configured mints advertised.
                  </div>
                  <ul class="font-mono break-all mt-1 space-y-0.5">
                    <li v-for="mint in mintRows" :key="mint.url">
                      <span
                        :class="advertisedMintKeys.has(normalizeMintUrl(mint.url)) ? 'text-green-300' : 'text-yellow-300'"
                      >
                        {{ advertisedMintKeys.has(normalizeMintUrl(mint.url)) ? '●' : '○' }}
                      </span>
                      {{ mint.url }}
                    </li>
                  </ul>
                  <div v-if="missingMintRows.length" class="text-yellow-300 mt-1">
                    Not advertised by the daemon:
                    {{ missingMintRows.map(m => mintHost(m.url)).join(', ') }}
                  </div>
                  <div class="flex items-center gap-3 mt-2">
                    <button
                      v-if="missingMintRows.length"
                      class="text-xs text-white/50 hover:text-white"
                      :disabled="mintHealthLoading"
                      data-testid="check-mints"
                      @click="checkMints"
                    >
                      {{ mintHealthLoading ? 'Checking…' : 'Check mint health' }}
                    </button>
                    <button
                      v-if="unreachableMints.length > 0 && unreachableMints.length < mintRows.length"
                      class="text-xs text-red-300/80 hover:text-red-200"
                      :disabled="mintHealthLoading"
                      data-testid="prune-mints"
                      @click="pruneUnreachableMints"
                    >
                      Remove unreachable
                    </button>
                  </div>
                  <div v-if="mintHealthError" class="text-red-400 mt-1">{{ mintHealthError }}</div>
                  <div v-if="showMintHealth && mintHealth" class="mt-2 space-y-1">
                    <div v-for="m in mintHealthRows" :key="m.url" class="text-[11px]">
                      <span
                        :class="m.advertised ? 'text-green-300' : m.ok ? 'text-yellow-300' : 'text-red-300'"
                      >
                        {{ m.advertised ? 'advertised' : m.ok ? 'reachable, not advertised' : 'unreachable' }}
                      </span>
                      <span class="font-mono break-all text-white/50"> {{ m.url }}</span>
                      <span v-if="m.error" class="text-red-300"> — {{ m.error }}</span>
                    </div>
                  </div>
                </dd>
              </div>
              <div class="col-span-2 border-t border-white/10 pt-3 mt-1">
                <dt class="text-xs text-white/40 mb-1">Version</dt>
                <dd class="text-white/70 text-xs space-y-1">
                  <div>
                    Package:
                    <span class="font-mono">{{ status.tollgate.version || 'unknown' }}</span>
                    <span v-if="status.tollgate.pkg_manager">({{ status.tollgate.pkg_manager }})</span>
                  </div>
                  <div>
                    Running daemon:
                    <span class="font-mono" :class="status.tollgate.daemon_running ? 'text-green-300' : 'text-yellow-300'">
                      {{ daemonLabel }}
                    </span>
                  </div>
                  <div>
                    Pinned:
                    <span class="font-mono">{{ status.tollgate.pinned_version || 'unknown' }}</span>
                    <span v-if="status.tollgate.up_to_date" class="text-green-300">up to date</span>
                  </div>
                  <div>
                    Advertised mints: {{ advertisedMintCount }} of {{ mintRows.length }}
                  </div>
                </dd>
              </div>
            </dl>
          </details>

          <div v-if="adminReady" class="mt-4 border-t border-white/10 pt-4 space-y-5">
            <!-- Wallet -->
            <div data-testid="wallet-card">
              <div class="flex items-center justify-between mb-2">
                <h3 class="text-sm font-semibold text-white/80">Wallet</h3>
                <button class="text-xs text-white/40 hover:text-white" :disabled="walletLoading" @click="refreshGateway">
                  {{ walletLoading ? 'Loading…' : 'Refresh' }}
                </button>
              </div>
              <div v-if="walletError" class="text-xs text-red-400">{{ walletError }}</div>
              <div
                v-if="daemonWarmingUp"
                class="text-xs text-white/50 mb-2"
                data-testid="daemon-starting"
              >
                Daemon starting — waiting for mints to advertise…
              </div>
              <div v-else-if="walletDegraded" class="text-xs text-yellow-300 mb-2">
                Wallet degraded: {{ walletDegradedReason }}
              </div>
              <div v-else-if="walletInfo?.ok" class="text-xs text-green-300 mb-1">
                Wallet ready
              </div>
              <div class="text-sm text-white/70">
                {{ walletTotal }} sats · {{ walletMintCount }} mint{{ walletMintCount === 1 ? '' : 's' }}
              </div>
              <ul class="mt-1 text-xs font-mono text-white/50 break-all space-y-0.5">
                <li v-for="mint in walletMints" :key="mint[0]">
                  {{ mint[0] }} · {{ mint[1] }} sats
                </li>
              </ul>
              <div v-if="walletMints.length === 0" class="text-xs text-white/40 mt-1">
                No payments received yet.
              </div>
              <div v-if="missingMintRows.length" class="text-xs text-yellow-300/80 mt-1">
                Some configured mints are not advertised. Their proofs are preserved; check
                mint health above.
              </div>
              <div class="mt-3 flex flex-wrap items-center gap-3">
                <button
                  class="glass-button glass-button-warning text-xs px-3 py-1.5"
                  :disabled="drainPreviewLoading || !walletInfo?.ok || walletTotal === 0"
                  :class="drainPreviewLoading || !walletInfo?.ok || walletTotal === 0 ? 'opacity-40 cursor-not-allowed' : ''"
                  data-testid="wallet-drain"
                  @click="previewDrain"
                >
                  {{ drainPreviewLoading ? 'Loading preview…' : 'Drain to Archy wallet' }}
                </button>
                <span class="text-[11px] text-white/30">
                  Moves router Cashu balances into this node's wallet.
                </span>
              </div>
              <p v-if="drainPreviewError" class="mt-2 text-xs text-red-400">{{ drainPreviewError }}</p>
            </div>

            <!-- Upstream gateways -->
            <div id="upstream-gateways" data-testid="upstream-gateways">
              <div class="flex items-center justify-between mb-2">
                <h3 class="text-sm font-semibold text-white/80">Upstream gateways</h3>
                <button
                  class="text-xs text-white/40 hover:text-white"
                  :disabled="upstreamLoading"
                  data-testid="scan-upstreams"
                  @click="scanUpstreams"
                >
                  {{ upstreamLoading ? 'Scanning…' : 'Scan' }}
                </button>
              </div>
              <div v-if="upstreamError" class="text-xs text-red-400 mb-2">{{ upstreamError }}</div>
              <div class="text-xs text-white/40 mb-1">Configured</div>
              <div v-if="upstreamList.length === 0" class="text-xs text-white/40">None</div>
              <div
                v-for="u in upstreamList"
                :key="u.ssid"
                class="flex items-center justify-between text-xs py-1"
              >
                <span class="font-mono">{{ u.ssid }}</span>
                <span :class="u.status === 'ACTIVE' ? 'text-green-300' : 'text-white/40'">
                  {{ u.status }} · {{ u.radio }} · {{ u.encryption }}
                </span>
                <button
                  v-if="u.status !== 'ACTIVE'"
                  class="text-red-300/80 hover:text-red-200"
                  @click="removeUpstream(u.ssid)"
                >
                  Remove
                </button>
              </div>

              <div class="mt-3" data-testid="upstream-ranked-list">
                <div class="flex items-center justify-between mb-1">
                  <div class="text-xs text-white/40">
                    {{ upstreamScanned ? 'Nearby upstreams' : 'Previously seen' }}
                  </div>
                  <span v-if="!upstreamScanned" class="text-[11px] text-white/30">
                    Scan for live signal
                  </span>
                </div>
                <div v-if="visibleUpstreams.length === 0" class="text-xs text-white/40">
                  {{ upstreamScanned ? 'No upstreams found. Scan again.' : 'No upstreams seen yet. Scan to look for nearby APs.' }}
                </div>
                <div
                  v-for="row in visibleUpstreams"
                  :key="row.ssid"
                  class="py-2 border-b border-white/5 last:border-0"
                  data-testid="upstream-row"
                  :data-ssid="row.ssid"
                  :data-tollgate="row.isTollGate ? 'true' : 'false'"
                >
                  <div class="flex items-start justify-between gap-3 text-xs">
                    <div class="min-w-0">
                      <div class="font-mono text-white/80">
                        {{ row.ssid }}
                        <span v-if="row.isTollGate" class="ml-1 text-blue-300" data-testid="upstream-tollgate-badge">TollGate</span>
                        <span v-if="isActiveUpstream(row.ssid)" class="ml-1 text-green-300">active</span>
                      </div>
                      <div class="text-white/40">
                        {{ row.signal }} dBm · {{ row.encryption === 'none' ? 'Open' : row.encryption }}
                        <span v-if="row.radios > 1"> · {{ row.radios }} radios</span>
                        <span v-if="row.lastSeen"> · {{ formatLastSeen(row.lastSeen) }}</span>
                      </div>
                      <div v-if="row.isTollGate" class="text-[11px] text-white/40" data-testid="upstream-price">
                        {{ advertisedPrice(row) }}
                      </div>
                    </div>
                    <div class="flex items-center gap-2 whitespace-nowrap">
                      <button v-if="isActiveUpstream(row.ssid)" class="text-green-300" disabled>
                        Active
                      </button>
                      <button
                        v-else-if="row.isTollGate"
                        class="text-green-300 hover:text-green-200"
                        data-testid="upstream-connect"
                        @click="connectRankedUpstream(row)"
                      >
                        Connect
                      </button>
                      <button
                        v-else
                        class="text-green-300 hover:text-green-200"
                        data-testid="upstream-use-as-uplink"
                        @click="useAsUplink(row)"
                      >
                        Use as uplink
                      </button>
                    </div>
                  </div>
                </div>
                <button
                  v-if="hiddenUpstreamCount > 0"
                  class="mt-2 text-xs text-white/40 hover:text-white"
                  data-testid="upstream-show-more"
                  @click="showAllUpstreams = !showAllUpstreams"
                >
                  {{ showAllUpstreams ? 'Show less' : `Show ${hiddenUpstreamCount} more` }}
                </button>
                <p class="mt-2 text-[11px] text-white/30" data-testid="upstream-caveat">
                  Prices are self-reported by the seller; Archy cannot verify they will be honored.
                </p>
              </div>

              <input
                v-model="upstreamPassphrase"
                type="password"
                placeholder="Upstream WiFi password (leave blank if open)"
                class="mt-3 w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 text-xs focus:outline-none focus:border-white/40"
              />
              <p class="mt-2 text-[11px] text-yellow-300/80">
                Connecting changes the router's upstream and may interrupt active sessions.
              </p>
            </div>
          </div>
        </template>

        <!-- Reconfigure form -->
        <template v-else>
          <div class="flex items-center justify-between mb-4 py-3 border-b border-white/10">
            <div>
              <div class="text-sm text-white">Enable TollGate</div>
              <div class="text-xs text-white/40">Stops the service and the SSID broadcast when off</div>
            </div>
            <button
              class="relative w-11 h-6 rounded-full transition-colors flex-shrink-0"
              :class="editEnabled ? 'bg-green-500/60' : 'bg-white/15'"
              @click="editEnabled = !editEnabled"
            >
              <span
                class="absolute top-0.5 w-5 h-5 rounded-full bg-white transition-transform"
                :class="editEnabled ? 'translate-x-5' : 'translate-x-0.5'"
              ></span>
            </button>
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">Metering</label>
            <select
              v-model="editMetric"
              class="w-full px-4 py-3 bg-black/30 border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40 transition-colors"
            >
              <option value="milliseconds">Time</option>
              <option value="bytes">Data</option>
            </select>
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">
              {{ editMetric === 'bytes' ? 'Price per step (sats)' : 'Price per minute (sats)' }}
            </label>
            <input
              v-model.number="editPriceSats"
              type="number"
              min="1"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            />
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">
              {{ editMetric === 'bytes' ? 'Step (MB)' : 'Step (minutes)' }}
            </label>
            <input
              v-if="editMetric === 'bytes'"
              v-model.number="editStepSizeMb"
              type="number"
              min="1"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            />
            <input
              v-else
              v-model.number="editStepSizeMin"
              type="number"
              min="1"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            />
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">Minimum steps per purchase</label>
            <input
              v-model.number="editMinSteps"
              type="number"
              min="1"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            />
          </div>
          <div class="mb-4">
            <div class="flex items-center justify-between mb-2">
              <label class="block text-xs text-white/40">Accepted mints</label>
              <button class="text-xs text-white/40 hover:text-white" @click="addMint">+ Add mint</button>
            </div>
            <div
              v-for="(mint, index) in editMints"
              :key="index"
              class="mb-3 p-3 border border-white/10 rounded-lg"
            >
              <div class="flex items-center gap-2 mb-2">
                <input
                  v-model="mint.url"
                  type="text"
                  placeholder="https://mint.example"
                  class="flex-1 px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors font-mono text-xs"
                />
                <button
                  class="text-xs text-red-300/80 hover:text-red-200"
                  :disabled="editMints.length <= 1"
                  @click="removeMint(index)"
                >
                  Remove
                </button>
              </div>
              <div class="grid grid-cols-2 gap-2 mb-2">
                <label class="block">
                  <span class="block text-xs text-white/40 mb-1">Price per step (sats)</span>
                  <input
                    v-model.number="mint.price_per_step"
                    type="number"
                    min="1"
                    class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40"
                  />
                </label>
                <label class="block">
                  <span class="block text-xs text-white/40 mb-1">Min steps</span>
                  <input
                    v-model.number="mint.purchase_min_steps"
                    type="number"
                    min="1"
                    class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40"
                  />
                </label>
              </div>
              <div class="flex items-center gap-2">
                <button class="text-xs text-white/40 hover:text-white" @click="testMint(index)">Test</button>
                <span
                  v-if="mintTest[index]"
                  class="text-xs"
                  :class="mintTest[index]?.status === 'ok' ? 'text-green-300' : mintTest[index]?.status === 'fail' ? 'text-red-300' : 'text-white/40'"
                >
                  {{ mintTest[index]?.detail }}
                </span>
              </div>
            </div>
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">Payout Lightning address</label>
            <input
              v-model="editPayoutAddress"
              type="text"
              placeholder="you@yourwallet.example"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors font-mono text-xs"
            />
            <p class="mt-2 text-xs text-white/40">
              Where TollGate's built-in auto-payout sends your share once the on-router balance
              crosses its threshold. Leave blank to keep whatever's already set on the router —
              on a router not originally provisioned here, that may be an upstream default you
              don't control.
            </p>
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">Private management SSID</label>
            <input
              v-model="editPrivateSsid"
              type="text"
              placeholder="noderunner"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            />
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">Private Wi-Fi password</label>
            <input
              v-model="editPrivateKey"
              type="password"
              autocomplete="new-password"
              placeholder="leave blank to keep the current passphrase"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            />
            <p class="mt-2 text-xs text-white/40">
              8–63 characters. The package default is <span class="font-mono">TollGate</span>;
              set a real passphrase here.
            </p>
          </div>
          <div class="flex items-center gap-2">
            <button
              :disabled="updatingTollgate"
              class="glass-button glass-button-success flex-1 text-sm font-medium"
              :class="updatingTollgate ? 'opacity-40 cursor-not-allowed' : ''"
              @click="saveTollgateConfig"
            >
              {{ updatingTollgate ? 'Saving…' : 'Save' }}
            </button>
            <button class="text-xs text-white/40 hover:text-white px-3 py-2" :disabled="updatingTollgate" @click="editingTollgate = false">
              Cancel
            </button>
          </div>
          <p v-if="updateTollgateError" class="mt-3 text-xs text-red-400">{{ updateTollgateError }}</p>
        </template>
      </div>

      <!-- WiFi interfaces -->
      <div class="glass-card p-6" data-testid="wifi-interfaces">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-sm font-semibold text-white/80">
            WiFi Interfaces
            <span class="ml-2 text-xs font-normal text-white/40">(AP mode)</span>
          </h2>
          <label class="flex items-center gap-1 text-[11px] text-white/40">
            <input
              v-model="showDisabledWifi"
              type="checkbox"
              data-testid="show-legacy-wifi"
            />
            Show disabled/legacy
          </label>
        </div>

        <div v-if="visibleWifiGroups.length === 0" class="text-sm text-white/50">
          No active AP interfaces.
        </div>

        <div
          v-for="group in visibleWifiGroups"
          :key="group.ssid"
          class="flex items-start justify-between py-3 border-b border-white/10 last:border-0"
          data-testid="wifi-group"
          :data-ssid="group.ssid"
          :data-disabled="group.disabled"
        >
          <div class="min-w-0">
            <div class="flex items-center gap-2 mb-0.5">
              <span class="w-1.5 h-1.5 rounded-full inline-block flex-shrink-0"
                :class="group.disabled ? 'bg-white/20' : 'bg-green-400'"></span>
              <span class="text-sm font-medium text-white truncate">{{ group.ssid }}</span>
              <span v-if="group.network === 'tollgate'"
                class="text-xs px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-300">Paid</span>
              <span v-if="group.disabled"
                class="text-xs px-1.5 py-0.5 rounded bg-white/5 text-white/30">Disabled</span>
            </div>
            <div class="text-xs text-white/40 ml-3.5">
              {{ group.encryption === 'none' ? 'Open' : group.encryption }}
            </div>
          </div>
          <span class="text-xs text-white/50 font-mono text-right ml-3">
            {{ group.radios.join(', ') }}
          </span>
        </div>
      </div>

      <!-- Advanced: destructive vanilla-OpenWrt flash -->
      <div class="glass-card p-6 mt-4" data-testid="advanced-flash">
        <div class="flex items-start justify-between gap-4">
          <div>
            <h2 class="text-sm font-semibold text-white/80">Advanced</h2>
            <p class="mt-1 text-xs text-white/40">
              Flash vanilla OpenWrt and erase all router configuration. Use this to
              start from a clean install; it is separate from Install TollGate.
            </p>
          </div>
          <button
            class="glass-button glass-button-warning text-xs px-3 py-1.5 whitespace-nowrap"
            :disabled="!installerStatus?.available"
            :class="!installerStatus?.available ? 'opacity-40 cursor-not-allowed' : ''"
            data-testid="flash-open"
            @click="openFlashModal"
          >
            Flash vanilla OpenWrt…
          </button>
        </div>
      </div>

    </template>

    <!-- Destructive vanilla-OpenWrt flash -->
    <div
      v-if="flashOpen"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
    >
      <div
        class="glass-card max-w-2xl w-full p-6 max-h-[90vh] overflow-y-auto"
        data-testid="flash-modal"
      >
        <div class="flex items-start justify-between gap-4 mb-3">
          <div>
            <h3 class="text-sm font-semibold text-white/90">Flash vanilla OpenWrt</h3>
            <p class="mt-1 text-xs text-red-300">
              This erases all configuration. The router returns at 192.168.1.1 on
              its LAN and its SSH host key changes.
            </p>
          </div>
          <button
            class="text-xs text-white/40 hover:text-white"
            :disabled="flashRunning"
            @click="closeFlashModal"
          >
            Close
          </button>
        </div>

        <div v-if="flashInfo?.needs_trust" class="rounded-lg border border-yellow-400/20 bg-yellow-400/5 p-3">
          <div class="text-xs text-yellow-300">
            The installer does not trust this router's SSH key yet.
          </div>
          <pre class="mt-2 text-[11px] font-mono bg-black/30 rounded p-2 text-green-300 break-all whitespace-pre-wrap">{{ flashInfo.ssh_fingerprint }}</pre>
          <code class="block mt-2 text-[11px] font-mono bg-black/30 rounded p-2 text-white/60 break-all whitespace-pre-wrap">dropbearkey -y -f /etc/dropbear/dropbear_ed25519_host_key</code>
          <button
            class="glass-button glass-button-warning text-xs px-3 py-1.5 mt-2"
            :disabled="flashTrustPending"
            @click="trustFlashHost"
          >
            {{ flashTrustPending ? 'Trusting…' : 'Trust host key and inspect' }}
          </button>
        </div>

        <template v-else>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">OpenWrt release</span>
              <select
                v-model="flashVersion"
                class="w-full px-3 py-2 bg-black/30 border border-white/20 rounded-lg text-white text-sm focus:outline-none focus:border-white/40"
                data-testid="flash-version"
                @change="inspectFlashImage"
              >
                <option value="25.12.5">25.12.5 (apk)</option>
                <option value="24.10.8">24.10.8 (opkg, if supported)</option>
              </select>
            </label>
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">Current root password (before flash)</span>
              <input
                v-model="flashPassword"
                type="password"
                placeholder="blank uses saved credentials / no password"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 text-sm focus:outline-none focus:border-white/40"
                data-testid="flash-password"
              />
            </label>
          </div>
          <label class="block mt-3">
            <span class="block text-xs text-white/40 mb-1">New root password after flash (optional)</span>
            <input
              v-model="flashNewPassword"
              type="password"
              autocomplete="new-password"
              placeholder="set a password so Archy can reconnect"
              class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 text-sm focus:outline-none focus:border-white/40"
              data-testid="flash-new-password"
            />
            <span class="block text-[11px] text-white/30 mt-1">
              The flash wipes the current root password. This is applied after the router reboots.
            </span>
          </label>
          <button
            class="glass-button text-xs px-3 py-1.5 mt-2"
            :disabled="flashInfoLoading"
            @click="inspectFlashImage"
          >
            {{ flashInfoLoading ? 'Inspecting…' : 'Inspect image' }}
          </button>

          <p v-if="flashInfoError" class="mt-2 text-xs text-red-400">{{ flashInfoError }}</p>

          <div v-if="flashInfo?.ok" class="mt-4 space-y-3">
            <div class="text-xs text-white/60">
              Board:
              <span class="font-mono text-white/90" data-testid="flash-board">{{ flashInfo.board }}</span>
            </div>
            <div class="text-xs text-white/40 break-all">
              Image: <span class="font-mono">{{ flashInfo.image_url }}</span>
            </div>
            <div class="text-xs text-white/40 break-all">
              SHA256: <span class="font-mono">{{ flashInfo.sha256 }}</span>
            </div>

            <div class="rounded-lg border border-white/10 bg-white/[0.02] p-3">
              <div class="text-xs text-white/40 mb-1">Post-flash wired link</div>
              <div
                v-for="link in flashInfo.links"
                :key="link.name"
                class="flex flex-wrap items-center justify-between gap-2 py-1 text-xs"
                data-testid="flash-link"
                :data-interface="link.name"
              >
                <span class="font-mono text-white/60">
                  {{ link.name }} · {{ link.wireless ? 'WiFi' : 'Wired' }} ·
                  carrier {{ link.carrier ? '1' : '0' }} ·
                  {{ link.ipv4.join(', ') || 'no IPv4' }}
                  <span v-if="link.preferred" class="ml-1 text-blue-300">router link</span>
                  <span v-if="!link.physical" class="ml-1 text-white/30">virtual</span>
                </span>
                <button
                  v-if="canPrepareFlashLink(link)"
                  class="text-xs text-yellow-300 hover:text-yellow-200 underline"
                  data-testid="flash-prepare-link"
                  :data-interface="link.name"
                  @click="prepareFlashLink(link)"
                >
                  {{ link.preferred ? 'Add 192.168.1.2/24 (router link)' : 'Add 192.168.1.2/24' }}
                </button>
              </div>
              <div v-if="flashInfo.flash_ready" class="text-xs text-green-300 mt-1">
                A wired 192.168.1.x link is ready for the post-flash address.
              </div>
              <div v-else class="text-xs text-red-300 mt-1">
                <template v-if="preferredFlashLink">
                  Add <span class="font-mono">192.168.1.2/24</span> to
                  <span class="font-mono">{{ preferredFlashLink.name }}</span>
                  (the router link) to enable flashing.
                </template>
                <template v-else>
                  No physical wired interface with carrier and a 192.168.1.x
                  address. After flashing, the router is only reachable at
                  192.168.1.1 on its LAN.
                </template>
              </div>
            </div>

            <label class="block">
              <span class="block text-xs text-white/40 mb-1">
                Type the board name to confirm
              </span>
              <input
                v-model="flashConfirmation"
                type="text"
                :placeholder="flashInfo.board"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 font-mono text-sm focus:outline-none focus:border-white/40"
                data-testid="flash-confirmation"
              />
            </label>
            <label class="flex items-start gap-2 text-xs text-white/60">
              <input v-model="flashAcknowledge" type="checkbox" data-testid="flash-ack" />
              <span>
                I understand this erases all configuration and the router will only
                answer on 192.168.1.1 on its LAN.
              </span>
            </label>
            <p
              v-if="flashBlockReason"
              class="text-xs text-white/50"
              data-testid="flash-block-reason"
            >
              {{ flashBlockReason }}
            </p>
            <button
              class="glass-button glass-button-warning w-full text-sm font-medium"
              :disabled="!flashCanStart || flashRunning"
              :class="!flashCanStart || flashRunning ? 'opacity-40 cursor-not-allowed' : ''"
              data-testid="flash-start"
              @click="startFlash"
            >
              {{ flashRunning ? 'Flashing…' : `Flash ${flashVersion} vanilla OpenWrt` }}
            </button>
          </div>

          <div v-if="flashJob" class="mt-4 rounded-lg border border-white/10 p-3" data-testid="flash-progress">
            <div class="flex items-center justify-between text-xs">
              <span class="text-white/60">Status</span>
              <span class="font-mono" :class="flashJob.status === 'failed' ? 'text-red-300' : flashJob.status === 'done' ? 'text-green-300' : 'text-yellow-300'">
                {{ flashJob.status }}
              </span>
            </div>
            <div v-for="step in flashJob.steps" :key="step.name" class="mt-1 text-[11px] text-white/50">
              {{ step.name }} · {{ step.status }}<span v-if="step.detail"> · {{ step.detail }}</span>
            </div>
            <p v-if="flashJob.error" class="mt-2 text-xs text-red-400">{{ flashJob.error }}</p>
            <p v-if="flashJob.note" class="mt-2 text-xs text-yellow-300">{{ flashJob.note }}</p>
            <template v-if="flashJob.status === 'done'">
              <p class="mt-2 text-xs text-green-300">
                Router flashed. It is at
                <span class="font-mono">{{ flashJob.ip || '192.168.1.1' }}</span>; the old host key is gone.
              </p>
              <button
                class="glass-button glass-button-success text-xs px-3 py-1.5 mt-2"
                data-testid="flash-reconnect"
                @click="useFlashedRouter"
              >
                Reconnect at {{ flashJob.ip || '192.168.1.1' }}
              </button>
            </template>
            <template v-else-if="flashJob.status === 'failed'">
              <p class="mt-2 text-xs text-yellow-300">
                The job may still have flashed the router. Check
                <span class="font-mono">192.168.1.1</span> before retrying.
              </p>
              <button
                class="glass-button text-xs px-3 py-1.5 mt-2"
                :disabled="flashRecoveryLoading"
                data-testid="flash-recheck"
                @click="recheckFlashedRouter"
              >
                {{ flashRecoveryLoading ? 'Checking 192.168.1.1…' : 'Check 192.168.1.1' }}
              </button>
              <div v-if="flashRecoveryInfo" class="mt-2 rounded-lg border border-white/10 p-2">
                <template v-if="flashRecoveryInfo.ssh_fingerprint">
                  <p class="text-xs text-yellow-300">
                    The router returned with a new SSH host key.
                  </p>
                  <pre
                    class="mt-1 text-[11px] font-mono bg-black/30 rounded p-2 text-green-300 break-all whitespace-pre-wrap"
                    data-testid="flash-recovery-fingerprint"
                  >{{ flashRecoveryInfo.ssh_fingerprint }}</pre>
                  <code class="block mt-1 text-[11px] font-mono bg-black/30 rounded p-2 text-white/60 break-all whitespace-pre-wrap">dropbearkey -y -f /etc/dropbear/dropbear_ed25519_host_key</code>
                  <button
                    class="glass-button glass-button-warning text-xs px-3 py-1.5 mt-2"
                    :disabled="flashRecoveryTrusting"
                    data-testid="flash-recovery-trust"
                    @click="trustRecoveredRouter"
                  >
                    {{ flashRecoveryTrusting ? 'Trusting…' : 'Trust and reconnect' }}
                  </button>
                </template>
                <template v-else-if="flashRecoveryInfo.identified">
                  <p class="text-xs text-green-300">
                    Router is at {{ flashRecoveryInfo.ip }}; it is already on vanilla OpenWrt.
                  </p>
                  <p class="text-[11px] text-white/40 mt-1">
                    {{ flashRecoveryInfo.firmware }}<span v-if="flashRecoveryInfo.model"> · {{ flashRecoveryInfo.model }}</span>
                  </p>
                  <button
                    class="glass-button glass-button-success text-xs px-3 py-1.5 mt-2"
                    data-testid="flash-recovery-reconnect"
                    @click="useFlashedRouter"
                  >
                    Reconnect at {{ flashRecoveryInfo.ip }}
                  </button>
                </template>
              </div>
              <p v-if="flashRecoveryError" class="mt-2 text-xs text-red-400">{{ flashRecoveryError }}</p>
            </template>
            <p v-else-if="flashJob.status === 'pending'" class="mt-2 text-xs text-yellow-300">
              The router is still starting. Check
              <span class="font-mono">{{ flashJob.ip || '192.168.1.1' }}</span> before retrying.
            </p>
          </div>
          <p v-if="flashError" class="mt-2 text-xs text-red-400">{{ flashError }}</p>
        </template>
      </div>
    </div>

    <!-- Router wallet drain preview -->
    <div
      v-if="drainModal && drainPreview"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
    >
      <div
        class="glass-card max-w-lg w-full p-6 max-h-[90vh] overflow-y-auto"
        data-testid="drain-preview"
      >
        <h3 class="text-sm font-semibold text-white/90 mb-2">Drain router wallet</h3>
        <div v-if="drainPreview.error" class="text-xs text-red-400 mb-2">
          {{ drainPreview.error }}
        </div>
        <p class="text-xs text-white/60">
          Router balance:
          <span class="font-mono">{{ drainPreview.router?.total_balance ?? 0 }} sats</span>
        </p>
        <p class="text-xs text-white/60 mt-1">
          Archy wallet:
          <span class="font-mono">{{ drainPreview.node?.network }}</span>,
          <span class="font-mono">{{ drainPreview.node?.balance_sats ?? 0 }} sats</span>,
          accepts {{ drainPreview.node?.accepted_mints?.length ?? 0 }} mint(s).
        </p>

        <div v-if="(drainPreview.not_accepted?.length ?? 0) > 0" class="mt-3">
          <div class="text-xs text-yellow-300">Not accepted by the Archy wallet:</div>
          <ul class="mt-1 text-[11px] font-mono text-white/50 break-all space-y-0.5">
            <li v-for="mint in drainPreview.not_accepted" :key="mint.url">
              {{ mint.url }} · {{ mint.amount }} sats
            </li>
          </ul>
          <button
            class="glass-button glass-button-warning text-xs px-3 py-1.5 mt-2"
            :disabled="drainMintAdding"
            data-testid="drain-add-mints"
            @click="addMissingMints"
          >
            {{ drainMintAdding ? 'Adding…' : 'Add missing mints to Archy wallet' }}
          </button>
          <p v-if="drainMintError" class="mt-1 text-xs text-red-400">{{ drainMintError }}</p>
        </div>

        <div v-if="(drainPreview.wrong_network?.length ?? 0) > 0" class="mt-3 text-xs text-yellow-300">
          These mints are on the other ecash network and will be skipped:
          <span class="font-mono">
            {{ drainPreview.wrong_network?.map(mint => mint.url).join(', ') }}
          </span>
        </div>

        <div v-if="(drainPreview.receivable?.length ?? 0) > 0" class="mt-3">
          <div class="text-xs text-green-300">Receivable now:</div>
          <ul class="mt-1 text-[11px] font-mono text-white/50 break-all space-y-0.5">
            <li v-for="mint in drainPreview.receivable" :key="mint.url">
              {{ mint.url }} · {{ mint.amount }} sats
            </li>
          </ul>
        </div>

        <p class="mt-3 text-[11px] text-yellow-300/80">
          Draining removes the funds from the router and is irreversible. Any token the node
          cannot accept is still shown here and remains in the router drain journal.
        </p>
        <div class="mt-4 flex items-center gap-3">
          <button
            class="glass-button glass-button-warning text-sm font-medium"
            :disabled="drainExecuting || (drainPreview.receivable?.length ?? 0) === 0"
            :class="drainExecuting || (drainPreview.receivable?.length ?? 0) === 0 ? 'opacity-40 cursor-not-allowed' : ''"
            data-testid="drain-confirm"
            @click="confirmDrain"
          >
            {{ drainExecuting ? 'Draining…' : `Drain ${drainPreview.router?.total_balance ?? 0} sats` }}
          </button>
          <button class="text-xs text-white/40 hover:text-white" @click="drainModal = false">
            Cancel
          </button>
        </div>
      </div>
    </div>

    <!-- Router wallet drain result -->
    <div
      v-if="drainResultModal && drainResult"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
    >
      <div
        class="glass-card max-w-lg w-full p-6 max-h-[90vh] overflow-y-auto"
        data-testid="drain-result"
      >
        <h3 class="text-sm font-semibold text-white/90 mb-2">Drain result</h3>
        <p v-if="drainResult.error" class="text-xs text-red-400">{{ drainResult.error }}</p>
        <p class="text-sm text-white/70">
          Received
          <span class="font-mono text-green-300">{{ drainResult.total_received ?? 0 }} sats</span>
          into the Archy wallet.
        </p>
        <ul class="mt-2 text-xs font-mono text-white/50 break-all space-y-0.5">
          <li v-for="mint in drainResult.received" :key="mint.mint">
            {{ mint.mint }} · {{ mint.amount }} sats
          </li>
        </ul>
        <div v-if="(drainResult.failed?.length ?? 0) > 0" class="mt-3">
          <div class="text-xs text-red-300">Not imported:</div>
          <div
            v-for="failed in drainResult.failed"
            :key="failed.mint"
            class="mt-2 text-xs text-red-300"
          >
            <div>{{ failed.mint }} · {{ failed.amount }} sats — {{ failed.error }}</div>
            <textarea
              v-if="failed.token"
              readonly
              :value="failed.token"
              rows="3"
              class="mt-1 w-full px-2 py-1 bg-black/30 border border-white/10 rounded text-[10px] font-mono text-white/50"
              data-testid="drain-failed-token"
            ></textarea>
            <button
              v-if="failed.token"
              class="mt-1 text-[11px] text-white/50 hover:text-white underline"
              @click="copyText(failed.token || '')"
            >
              Copy token
            </button>
          </div>
          <p class="mt-2 text-[11px] text-yellow-300/80">
            These tokens are bearer funds. They remain in the router drain journal; save the
            token before closing.
          </p>
        </div>
        <div class="mt-4 flex items-center gap-3">
          <button class="glass-button text-sm font-medium" @click="drainResultModal = false">
            Close
          </button>
        </div>
      </div>
    </div>

    <!-- Host-key trust (the official installer refuses an unknown key). -->
    <div v-if="trustRequired" class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
      <div class="glass-card max-w-md w-full p-6">
        <h3 class="text-sm font-semibold text-white/90 mb-2">Trust this router's SSH key</h3>
        <p class="text-xs text-white/50 mb-3">
          Verify this fingerprint on the router's own console before continuing.
          The installer will not send the root password until the key is trusted.
        </p>
        <pre class="text-xs font-mono bg-black/30 rounded-lg p-3 text-green-300 break-all mb-2">{{ trustFingerprint }}</pre>
        <code class="block text-[11px] font-mono bg-black/30 rounded-lg p-2 text-white/60 break-all whitespace-pre-wrap mb-4">dropbearkey -y -f /etc/dropbear/dropbear_ed25519_host_key</code>
        <div class="flex items-center gap-3">
          <button
            class="glass-button glass-button-success text-sm font-medium"
            :disabled="trustPending"
            :class="trustPending ? 'opacity-40 cursor-not-allowed' : ''"
            @click="trustAndInstall"
          >
            {{ trustPending ? 'Trusting…' : 'Trust and install' }}
          </button>
          <button class="text-xs text-white/40 hover:text-white" @click="cancelTrust">Cancel</button>
        </div>
        <p v-if="provisionError" class="mt-3 text-xs text-red-400">{{ provisionError }}</p>
      </div>
    </div>

  </div>
</template>

<style scoped>
.router-model-card {
  margin: 24px 0 48px;
}
.router-model-card > .grid { height: 100%; min-height: 0; grid-template-rows: minmax(0, 1fr); }

@media (min-width: 1024px) {
  .router-model-card > .grid > :deep(.router-marketplace) {
    align-self: end;
    margin-top: 0;
  }
}
</style>
