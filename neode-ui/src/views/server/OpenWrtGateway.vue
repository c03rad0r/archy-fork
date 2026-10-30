<script setup lang="ts">
import { ref, computed, onMounted, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import BackButton from '@/components/BackButton.vue'
import DeviceModel from '@/components/DeviceModel.vue'
import HardwareMarketplace from '@/components/HardwareMarketplace.vue'

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
}

interface WifiInterface {
  section: string
  ssid: string
  device: string
  encryption: string
  network: string
  disabled: boolean
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
  last_seen?: string
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
  persist: true, // router/network status (host/uptime/wifi/wan) — no identity/money payload
  immediate: false, // initial fetch is gated explicitly in onMounted below
})
const status = computed(() => routerResource.data.value)
// 'refreshing' deliberately excluded (D-07, keep-last-value): a background
// TTL revalidation must not hide the already-rendered status panels behind
// the full loading skeleton — only a true first-load (no data at all yet)
// blocks on it. Previously this conflated the two, so a cache hit would
// still briefly show the skeleton on every mount instead of painting
// instantly.
const loading = computed(() => {
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

// First-install pricing. The Edit form already exposes these after install;
// sending them here too means the operator does not have to install at the
// defaults and immediately edit.
const installPriceSats = ref(10)
const installStepSizeMin = ref(1)
const installMinSteps = ref(1)
const installPayoutAddress = ref('')

const installerStatus = ref<InstallerStatus | null>(null)
const discovered = ref<DiscoveredRouter[]>([])
const discovering = ref(false)
const discoveryError = ref('')
const discoveryActive = ref('')
const selectedRouterIp = ref('')
const needsPassword = ref(false)
const selectedRouter = computed(() =>
  discovered.value.find(r => r.ip === selectedRouterIp.value) ?? null,
)

const walletInfo = ref<WalletInfo | null>(null)
const walletLoading = ref(false)
const walletError = ref('')
const upstreamList = ref<UpstreamSTA[]>([])
const upstreamKnown = ref<KnownTollGate[]>([])
const upstreamNetworks = ref<UpstreamNetwork[]>([])
const upstreamLoading = ref(false)
const upstreamError = ref('')
const upstreamPassphrase = ref('')
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
const editMinSteps = ref(1)
const editMints = ref<MintConfig[]>([])
type MintTestState = { status: 'idle' | 'testing' | 'ok' | 'fail'; detail: string }
const mintTest = ref<Record<number, MintTestState>>({})
const editPayoutAddress = ref('')
const editPrivateSsid = ref('noderunner-private')
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

const mintMismatch = computed(() => {
  const t = status.value?.tollgate
  if (!t) return false
  const advertised = t.advertised_mints?.length ?? 0
  const configured = t.configured_mints ?? 0
  return configured > 0 && advertised !== configured
})

const restartRequired = computed(() => {
  const t = status.value?.tollgate
  return !!t && t.installed && (!t.daemon_running || t.daemon_version_matches === false)
})

const installerNeeded = computed(() => {
  const t = status.value?.tollgate
  return !!t && (!t.installed || t.up_to_date === false)
})

const adminReady = computed(() => {
  const t = status.value?.tollgate
  return !!t && t.installed && t.up_to_date !== false && t.daemon_running !== false
})

const showUplink = computed(
  () => !!status.value && installerNeeded.value && !status.value.wan?.internet,
)

const walletDegraded = computed(() => {
  if (walletInfo.value?.degraded) return true
  const t = status.value?.tollgate
  if (!t) return false
  return (t.configured_mints ?? 0) > 0 && (t.advertised_mints?.length ?? 0) === 0
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
async function load(params?: Record<string, string>) {
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
  installPriceSats.value = s.tollgate.price_per_step ?? 10
  installStepSizeMin.value = Math.max(
    1,
    Math.round((s.tollgate.step_size_ms ?? 60_000) / 60_000),
  )
  installMinSteps.value = s.tollgate.min_steps ?? 1
  installPayoutAddress.value = s.tollgate.payout_address ?? ''
  editPrivateSsid.value = s.tollgate.private_ssid || 'noderunner-private'
}

async function connect() {
  const ip = host.value.trim()
  if (!ip) return
  const router: DiscoveredRouter =
    selectedRouter.value && selectedRouter.value.ip === ip
      ? selectedRouter.value
      : { ip }
  await trySelectRouter(router, sshPassword.value)
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
  host.value = status.value?.host ?? host.value
  connectedParams.value = null
  error.value = ''
  detectError.value = ''
  detectedCandidates.value = []
  showConnectForm.value = true
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
  await trySelectRouter(r, '')
}

async function trySelectRouter(r: DiscoveredRouter, password: string) {
  connecting.value = true
  error.value = ''
  try {
    const result = await rpcClient.call<SelectRouterResult>({
      method: 'openwrt.select-router',
      params: {
        host: r.ip,
        ssh_user: 'root',
        ssh_password: password,
        ...(r.mac ? { mac: r.mac } : {}),
        ...(r.model ? { model: r.model } : {}),
        ...(r.name ? { name: r.name } : {}),
      },
      timeout: 30000,
    })
    if (!result.ok) {
      needsPassword.value = true
      showConnectForm.value = true
      error.value = result.bad_password
        ? 'That root password was refused. Enter the router root password.'
        : result.error || 'This router needs a root password.'
      return
    }
    needsPassword.value = false
    showConnectForm.value = false
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
    await loadAdminData()
  } catch (e) {
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
    upstreamKnown.value = known.known?.known_tollgates ?? []
  } catch (e) {
    upstreamError.value = e instanceof Error ? e.message : String(e)
  } finally {
    upstreamLoading.value = false
  }
}

async function scanUpstreams() {
  upstreamLoading.value = true
  upstreamError.value = ''
  try {
    const result = await rpcClient.call<{ networks: UpstreamNetwork[] }>({
      method: 'openwrt.upstream-scan',
      timeout: 120000,
    })
    upstreamNetworks.value = result.networks ?? []
  } catch (e) {
    upstreamError.value = e instanceof Error ? e.message : String(e)
  } finally {
    upstreamLoading.value = false
  }
}

async function connectUpstream(net: UpstreamNetwork) {
  if (
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

async function provisionTollgate() {
  provisionError.value = ''
  provisionSuccess.value = false
  if (!installPayoutAddress.value.trim()) {
    provisionError.value =
      'A Lightning payout address is required by the official installer (where TollGate sends your share).'
    return
  }
  const params: Record<string, unknown> = {
    ...authParams(),
    price_sats: installPriceSats.value,
    step_size_ms: installStepSizeMin.value * 60_000,
    min_steps: installMinSteps.value,
    payout_address: installPayoutAddress.value.trim(),
    private_ssid: editPrivateSsid.value || 'noderunner-private',
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

async function doProvision(params: Record<string, unknown>) {
  const result = await rpcClient.call<{
    ok?: boolean
    host_key_untrusted?: boolean
    ssh_fingerprint?: string
  }>({
    method: 'openwrt.provision-tollgate',
    params,
    timeout: 900000,
  })
  if (result?.host_key_untrusted && result.ssh_fingerprint) {
    trustFingerprint.value = result.ssh_fingerprint
    pendingProvision.value = params
    trustRequired.value = true
    return
  }
  provisionSuccess.value = true
  await load(connectedParams.value ?? undefined)
  await loadInstallerStatus()
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
  editPriceSats.value = tg?.price_per_step ?? 10
  editStepSizeMin.value = Math.max(1, Math.round((tg?.step_size_ms ?? 60000) / 60000))
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
  editPrivateSsid.value = tg?.private_ssid || 'noderunner-private'
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
    const params: Record<string, unknown> = {
      ...authParams(),
      price_sats: editPriceSats.value,
      step_size_ms: editStepSizeMin.value * 60_000,
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
    await rpcClient.call({ method: 'openwrt.provision-tollgate', params, timeout: 300000 })
    editingTollgate.value = false
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
  updatingTollgate.value = true
  updateTollgateError.value = ''
  try {
    const tg = status.value?.tollgate
    await rpcClient.call({
      method: 'openwrt.provision-tollgate',
      params: {
        ...authParams(),
        price_sats: tg?.price_per_step ?? 10,
        step_size_ms: tg?.step_size_ms ?? 60_000,
        min_steps: tg?.min_steps ?? 1,
        mint_url: tg?.mint_url ?? '',
        payout_address: tg?.payout_address ?? '',
        private_ssid: tg?.private_ssid || 'noderunner-private',
        enabled: tg?.enabled ?? true,
      },
      timeout: 300000,
    })
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

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000 && bytes % 1_000_000_000 === 0) return `${bytes / 1_000_000_000} GB`
  if (bytes >= 1_000_000 && bytes % 1_000_000 === 0) return `${bytes / 1_000_000} MB`
  if (bytes >= 1_000 && bytes % 1_000 === 0) return `${bytes / 1_000} KB`
  return `${bytes} bytes`
}

const openwrtVersion = computed(() => {
  if (!status.value) return ''
  const r = status.value.release
  return r.openwrt_release || r.openwrt_version || ''
})

const boardName = computed(() => {
  if (!status.value) return ''
  return status.value.release.openwrt_board_name || ''
})

// Only force-fetch when the cache is missing or past its TTL, so a repeat
// open inside the TTL paints from cache with no new RPC. On a within-TTL
// remount, still reconcile the connect-form/error UI state from the cached
// entry (mirrors what a completed load() call would have set).
onMounted(() => {
  void loadInstallerStatus()
  void discoverRouters()
  if (routerResource.data.value === null || routerResource.isStale.value) {
    void load()
    return
  }
  const err = routerResource.error.value
  if (err) {
    if (err.includes('No router configured')) showConnectForm.value = true
    else error.value = err
  } else {
    showConnectForm.value = false
  }
})
</script>

<template>
  <div class="pb-12">
    <!-- mb-0 overrides the pill's default mb-4, which pushed it above the
         title's centerline in this inline header row. -->
    <div class="flex items-center gap-3 mb-6">
      <BackButton desktop-margin="mb-0" @click="goBack" />
      <h1 class="text-lg font-semibold text-white">OpenWrt Gateway</h1>
    </div>

    <!-- Discovered gateways -->
    <div class="glass-card p-6 mb-6">
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-sm font-semibold text-white/80">Discovered gateways</h2>
        <button
          class="text-xs text-white/40 hover:text-white"
          :disabled="discovering"
          @click="discoverRouters"
        >
          {{ discovering ? 'Scanning…' : 'Rescan' }}
        </button>
      </div>
      <p v-if="discoveryError" class="text-xs text-red-400">{{ discoveryError }}</p>
      <div v-if="discovering" class="text-sm text-white/50">Scanning the node's LAN interfaces…</div>
      <div
        v-else-if="discovered.length === 0 && !discoveryError"
        class="text-sm text-white/50"
      >
        No manageable gateways found.
      </div>
      <template v-else>
        <select
          v-model="selectedRouterIp"
          class="w-full px-3 py-2.5 bg-transparent border border-white/20 rounded-lg text-white text-sm focus:outline-none focus:border-white/40"
          @change="onDiscoveryChange"
        >
          <option value="">Select a gateway…</option>
          <option v-for="r in discovered" :key="r.ip" :value="r.ip">
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
            <span class="text-white/30">Firmware:</span>
            {{ selectedRouter.firmware || selectedRouter.vendor || 'unknown' }}
            <span v-if="selectedRouter.identified"> · OpenWrt</span>
            <span v-if="!selectedRouter.ssh_open"> · no SSH</span>
          </div>
          <div v-if="selectedRouter.ssh_fingerprint" class="text-yellow-300">
            Host key not trusted yet — trust is only needed for install/upgrade.
          </div>
          <div v-if="selectedRouter.has_saved_credentials" class="text-green-300">
            Root password saved for this router.
          </div>
          <div v-else class="text-white/40">
            Root password required.
          </div>
        </div>
      </template>
    </div>

    <!-- Connect form -->
    <template v-if="showConnectForm">
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
        <button class="text-xs text-white/50 hover:text-white transition-colors underline" @click="load()">Retry</button>
        <button class="text-xs text-orange-300/80 hover:text-orange-200 transition-colors underline" @click="disconnectRouter">Reconfigure router</button>
      </div>
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
          <button class="text-xs text-white/40 hover:text-white/70 transition-colors" @click="load()">
            ↻ Refresh
          </button>
          <button class="text-xs text-white/40 hover:text-white/70 transition-colors" @click="disconnectRouter">
            Switch router →
          </button>
        </div>
      </div>

      <!-- WAN / Uplink (only when an install/upgrade needs internet) -->
      <div v-if="showUplink" class="glass-card p-6 mb-4">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-sm font-semibold text-white/80">WAN / Uplink</h2>
          <button
            v-if="wanStep === 'idle' || wanStep === 'done'"
            class="text-xs text-white/40 hover:text-white transition-colors"
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
              <div class="text-sm font-medium text-white">{{ status.wan.ssid }}</div>
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
          <p class="text-xs text-white/40 mb-3">{{ scannedNetworks.length }} network{{ scannedNetworks.length !== 1 ? 's' : '' }} found. Select one:</p>
          <div v-if="scannedNetworks.length === 0" class="text-sm text-white/50">No networks found. Try scanning again.</div>
          <div class="space-y-1.5">
            <button
              v-for="net in scannedNetworks"
              :key="net.bssid"
              class="w-full flex items-center justify-between px-3 py-2.5 rounded-lg border border-white/10 hover:border-white/30 hover:bg-white/5 transition-colors text-left"
              @click="selectNetwork(net)"
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
                <span class="text-white text-sm">{{ net.ssid || '(hidden)' }}</span>
                <span v-if="net.encryption !== 'none'" class="text-white/30 text-xs">🔒</span>
              </div>
              <span class="text-xs text-white/30 font-mono flex-shrink-0">ch{{ net.channel }}</span>
            </button>
          </div>
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
            <button class="text-xs text-white/40 hover:text-white px-3 py-2" @click="wanStep = 'list'">←</button>
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
      <div class="glass-card p-6 mb-4">
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
          <div v-if="!status.wan?.internet" class="mb-3 text-xs text-yellow-400/80 bg-yellow-400/10 rounded-lg px-3 py-2">
            Router needs internet access to install TollGate. Configure WAN above first.
          </div>
          <div class="grid grid-cols-3 gap-3 mb-3">
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">Price (sats / min)</span>
              <input v-model.number="installPriceSats" type="number" min="1"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40" />
            </label>
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">Step (minutes)</span>
              <input v-model.number="installStepSizeMin" type="number" min="1"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40" />
            </label>
            <label class="block">
              <span class="block text-xs text-white/40 mb-1">Min steps</span>
              <input v-model.number="installMinSteps" type="number" min="1"
                class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40" />
            </label>
          </div>
          <label class="block mb-3">
            <span class="block text-xs text-white/40 mb-1">Payout Lightning address (required)</span>
            <input v-model="installPayoutAddress" type="text" placeholder="you@wallet.example"
              class="w-full px-3 py-2 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40" />
            <span class="block text-[11px] text-white/30 mt-1">
              Where TollGate sends the operator share. The official installer requires it.
            </span>
          </label>
          <button
            :disabled="provisioning"
            class="glass-button glass-button-success w-full text-sm font-medium"
            :class="provisioning ? 'opacity-40 cursor-not-allowed' : ''"
            @click="provisionTollgate"
          >
            {{
              provisioning
                ? 'Working… this may take a few minutes'
                : status.tollgate.installed
                  ? 'Upgrade TollGate'
                  : 'Install TollGate'
            }}
          </button>
          <p v-if="provisionError" class="mt-3 text-xs text-red-400">{{ provisionError }}</p>
          <p v-if="provisionSuccess && !provisioning" class="mt-3 text-xs text-green-400">TollGate provisioned successfully.</p>
        </div>

        <template v-else-if="!editingTollgate">
          <div class="flex items-center justify-between mb-4">
            <div class="flex items-center gap-2">
              <span class="w-2 h-2 rounded-full inline-block"
                :class="status.tollgate.enabled ? 'bg-green-400' : 'bg-yellow-400'"></span>
              <span class="text-sm" :class="status.tollgate.enabled ? 'text-green-300' : 'text-yellow-300'">
                {{ status.tollgate.enabled ? 'Enabled' : 'Disabled' }}
              </span>
            </div>
            <div class="flex items-center gap-3">
              <button
                v-if="restartRequired"
                class="text-xs text-yellow-300 hover:text-yellow-200"
                :disabled="updatingTollgate"
                @click="restartTollgate"
              >
                Restart TollGate
              </button>
              <button class="text-xs text-white/40 hover:text-white" @click="startEditTollgate">Edit</button>
            </div>
          </div>
          <dl class="grid grid-cols-2 gap-x-6 gap-y-3 text-sm">
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
                <div>{{ mintRows.length }} configured · {{ advertisedMintCount }} advertised</div>
                <ul class="font-mono break-all mt-1 space-y-0.5">
                  <li v-for="mint in mintRows.slice(0, 3)" :key="mint.url">{{ mint.url }}</li>
                </ul>
                <div v-if="mintRows.length > 3">… +{{ mintRows.length - 3 }} more</div>
                <div v-if="mintMismatch" class="text-yellow-300 mt-1">
                  Configured and advertised counts differ — the daemon may need a restart.
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
                  Advertised mints: {{ status.tollgate.advertised_mints?.length ?? 0 }}
                  <span v-if="mintMismatch" class="text-yellow-300">
                    (config says {{ status.tollgate.configured_mints }})
                  </span>
                </div>
              </dd>
            </div>
          </dl>

          <div v-if="adminReady" class="mt-4 border-t border-white/10 pt-4 space-y-5">
            <!-- Wallet -->
            <div>
              <div class="flex items-center justify-between mb-2">
                <h3 class="text-sm font-semibold text-white/80">Wallet</h3>
                <button class="text-xs text-white/40 hover:text-white" :disabled="walletLoading" @click="loadWallet">
                  {{ walletLoading ? 'Loading…' : 'Refresh' }}
                </button>
              </div>
              <div v-if="walletError" class="text-xs text-red-400">{{ walletError }}</div>
              <div v-if="walletDegraded" class="text-xs text-yellow-300 mb-2">
                Wallet degraded: {{ walletInfo?.error || 'configured mints are not being advertised' }}
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
                No funds across configured mints.
              </div>
            </div>

            <!-- Upstream gateways -->
            <div>
              <div class="flex items-center justify-between mb-2">
                <h3 class="text-sm font-semibold text-white/80">Upstream gateways</h3>
                <button class="text-xs text-white/40 hover:text-white" :disabled="upstreamLoading" @click="scanUpstreams">
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

              <div class="text-xs text-white/40 mt-3 mb-1">Known TollGates</div>
              <div v-if="upstreamKnown.length === 0" class="text-xs text-white/40">None discovered yet</div>
              <div
                v-for="k in upstreamKnown"
                :key="k.bssid || k.ssid"
                class="flex items-center justify-between text-xs py-1"
              >
                <span class="font-mono">{{ k.ssid }}</span>
                <span class="text-white/40">
                  {{ k.price_per_step ? k.price_per_step + ' sats/' + (k.step_size || '?') + 'ms' : 'price unknown' }}
                  <span v-if="k.best_signal"> · {{ k.best_signal }} dBm</span>
                </span>
                <button
                  class="text-green-300 hover:text-green-200"
                  @click="connectUpstream({ ssid: k.ssid, signal: k.best_signal || 0, encryption: 'unknown' })"
                >
                  Connect
                </button>
              </div>

              <div v-if="upstreamNetworks.length > 0" class="mt-3">
                <div class="text-xs text-white/40 mb-1">Scanned networks</div>
                <div
                  v-for="n in upstreamNetworks"
                  :key="n.bssid || n.ssid"
                  class="flex items-center justify-between text-xs py-1"
                >
                  <span class="font-mono">{{ n.ssid }}</span>
                  <span class="text-white/40">
                    {{ n.encryption }} · {{ n.signal }} dBm
                    <span v-if="n.is_tollgate" class="text-green-300"> · TollGate</span>
                  </span>
                  <button class="text-green-300 hover:text-green-200" @click="connectUpstream(n)">
                    Connect
                  </button>
                </div>
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
            <label class="block text-xs text-white/40 mb-2">Price per minute (sats)</label>
            <input
              v-model.number="editPriceSats"
              type="number"
              min="1"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/30 focus:outline-none focus:border-white/40 transition-colors"
            />
          </div>
          <div class="mb-4">
            <label class="block text-xs text-white/40 mb-2">Step (minutes)</label>
            <input
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
              placeholder="noderunner-private"
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
      <div class="glass-card p-6">
        <h2 class="text-sm font-semibold text-white/80 mb-4">
          WiFi Interfaces
          <span class="ml-2 text-xs font-normal text-white/40">(AP mode)</span>
        </h2>

        <div v-if="status.wifi_interfaces.length === 0" class="text-sm text-white/50">
          No AP interfaces configured.
        </div>

        <div
          v-for="iface in status.wifi_interfaces"
          :key="iface.section"
          class="flex items-start justify-between py-3 border-b border-white/10 last:border-0"
        >
          <div>
            <div class="flex items-center gap-2 mb-0.5">
              <span class="w-1.5 h-1.5 rounded-full inline-block"
                :class="iface.disabled ? 'bg-white/20' : 'bg-green-400'"></span>
              <span class="text-sm font-medium text-white">{{ iface.ssid || '(hidden)' }}</span>
              <span v-if="iface.network === 'tollgate'"
                class="text-xs px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-300">TollGate</span>
            </div>
            <div class="text-xs text-white/40 ml-3.5">
              {{ iface.device }} · {{ iface.encryption === 'none' ? 'Open' : iface.encryption }}
            </div>
          </div>
          <span class="text-xs text-white/30 font-mono mt-0.5">{{ iface.section }}</span>
        </div>
      </div>

    </template>

    <!-- Host-key trust (the official installer refuses an unknown key). -->
    <div v-if="trustRequired" class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
      <div class="glass-card max-w-md w-full p-6">
        <h3 class="text-sm font-semibold text-white/90 mb-2">Trust this router's SSH key</h3>
        <p class="text-xs text-white/50 mb-3">
          Verify this fingerprint on the router's own console before continuing.
          The installer will not send the root password until the key is trusted.
        </p>
        <pre class="text-xs font-mono bg-black/30 rounded-lg p-3 text-green-300 break-all mb-4">{{ trustFingerprint }}</pre>
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
