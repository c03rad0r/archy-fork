// Pinia store for mesh networking state (Meshcore LoRa)
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { rpcClient } from '@/api/rpc-client'

export interface MeshStatus {
  enabled: boolean
  device_type: string
  device_path: string | null
  device_connected: boolean
  firmware_version: string | null
  self_node_id: number | null
  self_advert_name: string | null
  peer_count: number
  channel_name: string
  messages_sent: number
  messages_received: number
  detected_devices?: string[]
  /** A serial radio is physically present, whether or not a session opened it. */
  device_present?: boolean
  /** Bitcoin block-header send/receive prefs (issue #28). */
  announce_block_headers?: boolean
  receive_block_headers?: boolean
  /** Operator-configured LoRa region (e.g. "EU_868"), for the Device tab. */
  region?: string | null
  /** Persisted config: LoRa region the driver programs on fresh radios. */
  lora_region?: string | null
  /** Persisted config: firmware pin (meshcore|meshtastic|reticulum), null = auto-detect. */
  device_kind?: string | null
  /** USB identity per detected port (for the setup modal's board image). */
  detected_device_info?: Array<{
    path: string
    vid?: string | null
    pid?: string | null
    product?: string | null
    manufacturer?: string | null
    /** Epoch seconds the /dev node appeared — changes on every replug. */
    plugged_at?: number | null
  }>
  /** Hot-swap "keep as is": false = archipelago never writes config to the radio. */
  manage_radio?: boolean
  /** Persisted config: MeshCore LoRa PHY params (firmware units). */
  lora_radio_params?: { freq_khz: number; bw_hz: number; sf: number; cr: number } | null
}

/** What mesh.probe-device read off a just-plugged radio (no provisioning). */
export interface MeshDeviceProbe {
  path: string
  kind: 'reticulum' | 'meshcore' | 'meshtastic'
  firmware_version: string | null
  node_id: number | null
  advert_name: string | null
  region: string | null
  modem_preset: string | null
  primary_channel: string | null
  secondary_channel: string | null
  max_contacts: number | null
}

export type FlashFirmwareFamily = 'meshcore' | 'meshtastic' | 'reticulum'
export type FlashBoard = 'heltec-v3' | 'heltec-v4'
export type FlashStage = 'downloading' | 'preparing' | 'erasing' | 'writing' | 'autoinstalling' | 'done' | 'failed'

/** Live progress for the one flash job that can run at a time. */
export interface FlashJobStatus {
  active: boolean
  board?: FlashBoard
  family?: FlashFirmwareFamily
  path?: string
  stage?: FlashStage
  percent?: number | null
  log_tail?: string[]
  done?: boolean
  error?: string | null
}

/** Params accepted by mesh.configure (superset of the status fields). */
export interface MeshConfigureParams {
  enabled?: boolean
  device_path?: string
  channel_name?: string
  broadcast_identity?: boolean
  advert_name?: string
  announce_block_headers?: boolean
  receive_block_headers?: boolean
  lora_region?: string
  device_kind?: string
  /** MeshCore LoRa PHY params in firmware field units (freq_khz = MHz×1000,
   *  bw_hz = kHz×1000). null clears; omitted leaves unchanged. */
  lora_radio_params?: { freq_khz: number; bw_hz: number; sf: number; cr: number } | null
  /** Hot-swap "keep as is": false = use the radio exactly as flashed. */
  manage_radio?: boolean
}

export interface MeshPeer {
  contact_id: number
  advert_name: string
  did: string | null
  pubkey_hex: string | null
  /** Verified archipelago ed25519 identity key. The backend binds this onto
   *  BOTH a node's twins — the federation peer natively and the radio twin via
   *  `bind_federation_twins` — so it's the most reliable key for collapsing the
   *  cross-transport duplicate (survives the device rename, unlike the advert
   *  name). Absent on radio peers that were never matched to a federation twin. */
  arch_pubkey_hex?: string | null
  rssi: number | null
  snr: number | null
  last_heard: string
  hops: number
  last_advert?: number
  reachable?: boolean
  /** Last known position (degrees), from a Meshtastic POSITION_APP broadcast.
   *  Absent until the peer has shared one. */
  lat?: number | null
  lon?: number | null
}

export interface MeshChannel {
  index: number
  name: string
  has_secret: boolean
}

export type MeshMessageTypeLabel =
  | 'text'
  | 'alert'
  | 'invoice'
  | 'psbt_hash'
  | 'coordinate'
  | 'block_header'
  | 'tx_relay'
  | 'tx_relay_response'
  | 'tx_confirmation'
  | 'lightning_relay'
  | 'lightning_relay_response'
  | 'content_ref'
  | 'reply'
  | 'reaction'
  | 'read_receipt'
  | 'forward'
  | 'edit'
  | 'delete'
  | 'presence'
  | 'channel_invite'
  | 'contact_card'

export interface MeshMessage {
  id: number
  direction: 'sent' | 'received'
  peer_contact_id: number
  peer_name: string | null
  plaintext: string
  timestamp: string
  delivered: boolean
  encrypted: boolean
  /// How the message traveled: "meshtastic", "meshcore", "reticulum" (radio
  /// transports, one per device kind), "fips", or "tor". Drives the
  /// per-message transport pill. Absent until known.
  transport?: string | null
  message_type?: MeshMessageTypeLabel
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  typed_payload?: Record<string, any> | null
  /// Cross-transport identity for this message — (sender_pubkey, sender_seq)
  /// forms a stable MessageKey used by replies/reactions.
  sender_pubkey?: string | null
  sender_seq?: number | null
}

export interface InvoiceData {
  bolt11: string
  amount_sats: number
  memo: string | null
  payment_hash?: string
  paid?: boolean
}

export interface AlertData {
  alert_type: 'emergency' | 'status' | 'dead_man' | 'block_header'
  message: string
  coordinate?: { lat: number; lng: number; label?: string }
  signed?: boolean
}

export interface CoordinateData {
  lat: number
  lng: number
  label?: string
}

export interface SessionStatus {
  has_session: boolean
  forward_secrecy: boolean
  message_count: number
  ratchet_generation: number
  peer_did: string | null
}

export interface AlertStatus {
  dead_man_enabled: boolean
  dead_man_interval_secs: number
  triggered: boolean
  time_remaining_secs: number
  has_gps: boolean
  emergency_contacts: number
}

export interface BlockHeader {
  height: number
  hash: string
  prev_hash: string
  timestamp: number
  announced_by: string
}

export interface DeniedAsker {
  contact_id: number
  name: string
  pubkey_hex: string | null
  at: string
}

export interface AssistantStatus {
  enabled: boolean
  model: string | null
  trusted_only: boolean
  backend: string
  allowed_contacts: string[]
  default_model: string
  ollama_detected: boolean
  claude_available: boolean
  models: string[]
  denied_askers?: DeniedAsker[]
}

export interface ScheduledMessage {
  id: number
  contact_id: number | null
  channel: number | null
  body: string
  fire_at: number
  attempts: number
}

export interface NodePosition {
  lat: number
  lng: number
  label?: string
  timestamp: string
}

/// An Archipelago node (federation peer), not a raw LoRa radio peer — shown
/// on the Mesh Map with its own marker (the Archy logo) since it's a whole
/// server, not a mesh-radio contact. Only ones that opted into
/// server.set-location's `share` flag ever carry a lat/lon.
export interface FederatedNodePosition {
  did: string
  lat: number
  lng: number
  name: string | null
  onion: string
  trusted: boolean
}

export const useMeshStore = defineStore('mesh', () => {
  const status = ref<MeshStatus | null>(null)
  const peers = ref<MeshPeer[]>([])
  const messages = ref<MeshMessage[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const sending = ref(false)

  // Serialize send operations to prevent concurrent fetchMessages() races
  let sendQueue: Promise<void> = Promise.resolve()

  // Node position tracking for map view (contact_id -> position)
  const nodePositions = ref<Map<number, NodePosition>>(new Map())
  // Federated Archipelago nodes with a shared location (DID -> position) —
  // separate from nodePositions since these are whole servers, not radio
  // peers, and render with a different marker on the Mesh Map.
  const federatedPositions = ref<Map<string, FederatedNodePosition>>(new Map())

  // Track unread message counts per peer (contact_id -> count)
  const unreadCounts = ref<Record<number, number>>({})
  // Durable seen-state: highest message id the user has actually seen, per
  // contact. Without this, every page load replayed ALL historical received
  // messages into unreadCounts (the in-memory store starts empty, so every
  // old message looked "new") — badges came back on every visit. Message ids
  // are safe watermarks: the backend allocates them monotonically and
  // restores the counter as max(persisted)+1 across restarts.
  const LAST_SEEN_KEY = 'archipelago.mesh.last-seen.v1'
  let storedLastSeen = localStorage.getItem(LAST_SEEN_KEY)
  function parseLastSeen(raw: string | null): Record<number, number> {
    if (!raw) return {}
    try {
      const parsed = JSON.parse(raw)
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed as Record<number, number>
      }
    } catch {
      // Treat corrupt browser state like a first run and safely reseed it.
    }
    storedLastSeen = null
    return {}
  }
  const lastSeenId = ref<Record<number, number>>(parseLastSeen(storedLastSeen))
  // First run after this feature ships: treat existing history as seen so
  // nobody gets a wall of phantom badges for months-old messages. Complete
  // this initialization even when history is empty; otherwise the first real
  // message to arrive on a brand-new node is mistaken for old history and its
  // notification is silently swallowed.
  let seedLastSeenFromHistory = storedLastSeen === null
  function persistLastSeen() {
    localStorage.setItem(LAST_SEEN_KEY, JSON.stringify(lastSeenId.value))
  }
  function advanceLastSeen(contactId: number, msgId: number): boolean {
    if ((lastSeenId.value[contactId] ?? 0) >= msgId) return false
    lastSeenId.value[contactId] = msgId
    return true
  }
  // Contact ids of the chat currently on screen — ALL twins of the merged
  // conversation, not just the clicked row's id, since the unread badge sums
  // across every underlying contact_id (a message can land on any twin).
  const viewingChatIds = ref<number[]>([])
  // Whether the open chat is scrolled to (near) the bottom. New messages only
  // auto-clear as read while the latest messages are actually in view —
  // standard chat-app semantics; scrolled-up-in-history still counts unread.
  const viewingAtBottom = ref(true)
  // Total unread count for nav badge
  const totalUnread = computed(() =>
    Object.values(unreadCounts.value).reduce((a, b) => a + b, 0)
  )

  async function fetchStatus() {
    try {
      loading.value = true
      const res = await rpcClient.call<MeshStatus>({ method: 'mesh.status', dedup: true })
      status.value = res
      trackDetectedDevices(res)
    } catch (err: unknown) {
      // Don't clobber a user-action error (broadcast/configure/send) — this
      // runs on a 5s poll, and the old `error.value = null` on entry meant
      // any real error banner survived at most one poll tick.
      if (!error.value) {
        error.value = err instanceof Error ? err.message : 'Failed to fetch mesh status'
      }
    } finally {
      loading.value = false
    }
  }

  // ── Global device-detection (for the app-wide "device detected" modal) ──
  // Ports the user dismissed the setup modal for, persisted per browser.
  // Dismissals are cleared the moment the port disappears (unplug), so the
  // modal re-appears on EVERY plug-in — including swapping a different stick
  // into the same /dev path — per the hot-swap UX (2026-07-22).
  // v2: dismissals key on (path → plugged_at). udev recreates the /dev node
  // on every plug, so plugged_at changes on each replug — an old "Not Now"
  // can never suppress a NEWLY plugged stick, even when the swap happens
  // faster than a status poll (which absence-pruning alone missed) or when
  // the same /dev path is reused. v1 (a bare path set) is intentionally
  // abandoned: stale one-time dismissals from before 2026-07-22 shouldn't
  // suppress anything either.
  const DETECT_DISMISS_KEY = 'archipelago.mesh.detect-dismissed.v2'
  const dismissedDetected = ref<Record<string, number>>(
    JSON.parse(localStorage.getItem(DETECT_DISMISS_KEY) || '{}') as Record<string, number>
  )
  function pluggedAt(s: MeshStatus, path: string): number {
    return s.detected_device_info?.find(d => d.path === path)?.plugged_at ?? 0
  }
  // Consecutive polls each candidate port has been present-but-not-connected.
  // The modal waits for 2 sightings so it doesn't flash during the couple of
  // seconds an ordinary reconnect (same radio, transient blip) needs.
  const detectSightings = ref<Record<string, number>>({})
  /** Epoch-ms until which the device-setup modal must NOT auto-open: an
   *  operator-initiated radio restart (settings apply, Reboot Radio) takes
   *  the radio down for ~15-20s, and the modal treated that healthy,
   *  expected gap as "a new stick was plugged in" and interrupted the flow
   *  (operator, 2026-08-06). */
  const suppressDetectUntil = ref(0)
  function suppressDeviceDetect(ms = 90_000) {
    suppressDetectUntil.value = Date.now() + ms
  }
  const undismissedDetectedDevices = computed(() => {
    const s = status.value
    if (!s) return []
    if (Date.now() < suppressDetectUntil.value) return []
    return (s.detected_devices || []).filter(p =>
      dismissedDetected.value[p] !== pluggedAt(s, p) &&
      // The port the live session occupies is not a candidate…
      !(s.device_connected && s.device_path === p) &&
      // …and a port only qualifies once it has survived the blip debounce.
      (detectSightings.value[p] ?? 0) >= 2
    )
  })
  /** Called after every status fetch: advance sighting counters and drop
   *  dismissals for ports that vanished (belt-and-braces with plugged_at). */
  function trackDetectedDevices(s: MeshStatus) {
    const present = new Set(s.detected_devices || [])
    const next: Record<string, number> = {}
    for (const p of present) {
      next[p] = s.device_connected && s.device_path === p
        ? 0
        : (detectSightings.value[p] ?? 0) + 1
    }
    detectSightings.value = next
    let dirty = false
    for (const p of Object.keys(dismissedDetected.value)) {
      if (!present.has(p)) {
        delete dismissedDetected.value[p]
        dirty = true
      }
    }
    if (dirty) {
      dismissedDetected.value = { ...dismissedDetected.value }
      localStorage.setItem(DETECT_DISMISS_KEY, JSON.stringify(dismissedDetected.value))
    }
  }
  function dismissDetectedDevice(path: string) {
    const s = status.value
    dismissedDetected.value = {
      ...dismissedDetected.value,
      [path]: s ? pluggedAt(s, path) : 0,
    }
    localStorage.setItem(DETECT_DISMISS_KEY, JSON.stringify(dismissedDetected.value))
  }
  // Manual "Flash LoRa" entry (Mesh header button): forces the global setup
  // modal open at the flash step for this port — including the live session's
  // port, which detection-driven opening deliberately excludes.
  const flashFlowPath = ref<string | null>(null)
  function openFlashFlow(path: string) {
    flashFlowPath.value = path
  }
  function closeFlashFlow() {
    flashFlowPath.value = null
  }
  /** Read-only firmware/config probe of a detected port (hot-swap modal). */
  async function probeDevice(path: string): Promise<MeshDeviceProbe> {
    return rpcClient.call<MeshDeviceProbe>({
      method: 'mesh.probe-device',
      params: { path },
      timeout: 45000, // serial probes are slow (multi-firmware handshakes)
    })
  }
  /** Available firmware version(s) for a family — v1 only ever returns
   *  ["latest"], since firmware is always fetched from upstream at flash
   *  time rather than pinned/bundled. */
  async function flashListFirmware(family: FlashFirmwareFamily): Promise<string[]> {
    const res = await rpcClient.call<{ versions: string[] }>({
      method: 'mesh.flash-list-firmware',
      params: { family },
    })
    return res.versions
  }
  /** Erase + reflash a detected radio. `board` is optional — omit it to let
   *  the backend auto-resolve from the port's USB vid:pid; if that fails
   *  (e.g. Heltec V4 not yet in the vid:pid table), it errors and the UI
   *  must ask the user to pick the board explicitly. Always erases first. */
  async function flashDevice(path: string, family: FlashFirmwareFamily, board?: FlashBoard): Promise<void> {
    await rpcClient.call({
      method: 'mesh.flash-device',
      params: board ? { path, family, board } : { path, family },
    })
  }
  async function flashStatus(): Promise<FlashJobStatus> {
    return rpcClient.call<FlashJobStatus>({ method: 'mesh.flash-status' })
  }
  async function flashCancel(): Promise<void> {
    await rpcClient.call({ method: 'mesh.flash-cancel' })
  }
  let globalDetectTimer: ReturnType<typeof setInterval> | null = null
  /** App-wide light poll so the detected-device modal works on every page
   *  (the Mesh view's own 5s poll takes over while it is mounted). */
  function startGlobalDetection(intervalMs = 15000) {
    if (globalDetectTimer) return
    // Never poll unauthenticated — a 401 from a background poll on the
    // login page caused a login-refresh loop (2026-07-14, .116).
    const authed = () => {
      try { return !!localStorage.getItem('neode-auth') } catch { return false }
    }
    if (authed()) void fetchStatus()
    globalDetectTimer = setInterval(() => {
      if (document.visibilityState === 'visible' && authed()) void fetchStatus()
    }, intervalMs)
  }

  async function fetchPeers() {
    try {
      const res = await rpcClient.call<{ peers: MeshPeer[]; count: number }>({
        method: 'mesh.peers',
        dedup: true,
      })
      peers.value = res.peers
      updateNodePositionsFromPeers(res.peers)
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to fetch mesh peers'
    }
  }

  async function fetchMessages(limit?: number): Promise<MeshMessage[]> {
    try {
      const res = await rpcClient.call<{ messages: MeshMessage[]; count: number }>({
        method: 'mesh.messages',
        params: limit ? { limit } : {},
        dedup: true,
      })
      if (seedLastSeenFromHistory) {
        for (const m of res.messages) {
          if (m.direction === 'received') advanceLastSeen(m.peer_contact_id, m.id)
        }
        // Persist even an empty object as the initialization sentinel.
        persistLastSeen()
        seedLastSeenFromHistory = false
      }
      // Detect new incoming messages and increment unread counts. "New" means
      // past the durable seen watermark, not merely absent from the in-memory
      // store — otherwise a page reload re-badges the whole history.
      const newMsgs = res.messages.filter(
        m =>
          m.direction === 'received' &&
          m.id > (lastSeenId.value[m.peer_contact_id] ?? 0) &&
          !messages.value.some(existing => existing.id === m.id)
      )
      let seenDirty = false
      for (const msg of newMsgs) {
        // Don't count as unread if we're currently viewing that chat AND the
        // bottom (latest messages) is in view — i.e. the user actually sees it.
        const seenLive =
          viewingChatIds.value.includes(msg.peer_contact_id) && viewingAtBottom.value
        if (seenLive) {
          seenDirty = advanceLastSeen(msg.peer_contact_id, msg.id) || seenDirty
        } else {
          unreadCounts.value[msg.peer_contact_id] = (unreadCounts.value[msg.peer_contact_id] || 0) + 1
        }
      }
      if (seenDirty) persistLastSeen()
      messages.value = res.messages
      // Extract node positions from coordinate messages
      updateNodePositionsFromMessages(res.messages)
      // The app-wide notification poll uses this exact batch, rather than a
      // session message-count delta, so one arrival can never resurrect old
      // messages as "11 unread" after a refresh.
      return newMsgs.filter(msg => !(
        viewingChatIds.value.includes(msg.peer_contact_id) && viewingAtBottom.value
      ))
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to fetch mesh messages'
      return []
    }
  }

  async function refreshMessagesAfterSend() {
    await fetchMessages()
    // Federation sends stamp FIPS/Tor asynchronously after the RPC returns.
    // Poll briefly so the transport pill settles without a browser refresh.
    for (const delay of [350, 900, 1600]) {
      setTimeout(() => { void fetchMessages() }, delay)
    }
  }

  // Convert microdegrees (from mesh protocol) to degrees for Leaflet
  // Values > 90 for lat or > 180 for lng indicate microdegrees
  function toDegreesIfMicro(lat: number, lng: number): { lat: number; lng: number } {
    if (Math.abs(lat) > 90 || Math.abs(lng) > 180) {
      return { lat: lat / 1000000, lng: lng / 1000000 }
    }
    return { lat, lng }
  }

  function updateNodePositionsFromMessages(msgs: MeshMessage[]) {
    for (const msg of msgs) {
      if (msg.message_type === 'coordinate' && msg.typed_payload) {
        const payload = msg.typed_payload as CoordinateData
        if (typeof payload.lat === 'number' && typeof payload.lng === 'number') {
          const existing = nodePositions.value.get(msg.peer_contact_id)
          if (!existing || msg.timestamp > existing.timestamp) {
            const deg = toDegreesIfMicro(payload.lat, payload.lng)
            nodePositions.value.set(msg.peer_contact_id, {
              lat: deg.lat,
              lng: deg.lng,
              label: payload.label,
              timestamp: msg.timestamp,
            })
          }
        }
      }
      // Also extract coordinates from alert messages that include location
      if (msg.message_type === 'alert' && msg.typed_payload) {
        const payload = msg.typed_payload as AlertData
        if (payload.coordinate && typeof payload.coordinate.lat === 'number' && typeof payload.coordinate.lng === 'number') {
          const existing = nodePositions.value.get(msg.peer_contact_id)
          if (!existing || msg.timestamp > existing.timestamp) {
            const deg = toDegreesIfMicro(payload.coordinate.lat, payload.coordinate.lng)
            nodePositions.value.set(msg.peer_contact_id, {
              lat: deg.lat,
              lng: deg.lng,
              label: payload.coordinate.label,
              timestamp: msg.timestamp,
            })
          }
        }
      }
    }
  }

  /** Feed real Meshtastic POSITION_APP broadcasts into the same map the
   *  manually-shared Coordinate/Alert messages already populate — MeshMap.vue
   *  needs no changes, it already renders from `nodePositions`. Uses
   *  `last_heard` as the freshness timestamp, same comparison pattern as
   *  `updateNodePositionsFromMessages`, so a peer position never clobbers a
   *  more-recently-shared coordinate message and vice versa. */
  function updateNodePositionsFromPeers(peerList: MeshPeer[]) {
    for (const peer of peerList) {
      if (typeof peer.lat !== 'number' || typeof peer.lon !== 'number') continue
      const existing = nodePositions.value.get(peer.contact_id)
      if (!existing || peer.last_heard > existing.timestamp) {
        nodePositions.value.set(peer.contact_id, {
          lat: peer.lat,
          lng: peer.lon,
          label: peer.advert_name,
          timestamp: peer.last_heard,
        })
      }
    }
  }

  function getNodePositions(): Map<number, NodePosition> {
    return nodePositions.value
  }

  // Update self node position from deadman GPS data (contact_id = -1 for self)
  function updateSelfPosition(lat: number, lng: number, label?: string) {
    nodePositions.value.set(-1, {
      lat,
      lng,
      label: label ?? 'This Node',
      timestamp: new Date().toISOString(),
    })
  }

  // Federation nodes that opted into sharing their location (server.set-location
  // `share: true`) — fed by Mesh.vue's existing federation.list-nodes poll.
  function updateFederatedPositions(nodes: Array<{
    did: string
    name?: string | null
    onion: string
    trust_level: string
    last_state?: { lat?: number | null; lon?: number | null } | null
  }>) {
    const next = new Map<string, FederatedNodePosition>()
    for (const n of nodes) {
      const lat = n.last_state?.lat
      const lon = n.last_state?.lon
      if (typeof lat === 'number' && typeof lon === 'number') {
        next.set(n.did, {
          did: n.did,
          lat,
          lng: lon,
          name: n.name ?? null,
          onion: n.onion,
          trusted: n.trust_level === 'trusted',
        })
      }
    }
    federatedPositions.value = next
  }

  function markChatRead(contactId: number | number[]) {
    const ids = Array.isArray(contactId) ? contactId : [contactId]
    viewingChatIds.value = ids
    let seenDirty = false
    for (const id of ids) {
      delete unreadCounts.value[id]
      // Persist the watermark so the seen-state survives reloads.
      for (const m of messages.value) {
        if (m.direction === 'received' && m.peer_contact_id === id) {
          seenDirty = advanceLastSeen(id, m.id) || seenDirty
        }
      }
    }
    if (seenDirty) persistLastSeen()
  }

  function clearViewingChat() {
    viewingChatIds.value = []
    viewingAtBottom.value = true
  }

  async function sendMessage(contactId: number, message: string) {
    const doSend = async () => {
      try {
        sending.value = true
        error.value = null
        const res = await rpcClient.call<{ sent: boolean; message_id: number; encrypted: boolean }>({
          method: 'mesh.send',
          params: { contact_id: contactId, message: message.trim() },
        })
        // Refresh messages after sending
        if (res.sent) {
          await refreshMessagesAfterSend()
        }
        return res
      } catch (err: unknown) {
        error.value = err instanceof Error ? err.message : 'Failed to send mesh message'
        throw err
      } finally {
        sending.value = false
      }
    }
    // Chain onto send queue to prevent concurrent fetchMessages() calls
    const result = sendQueue.then(doSend, doSend)
    sendQueue = result.then(() => {}, () => {})
    return result
  }

  async function sendChannelMessage(channel: number, message: string) {
    const doSend = async () => {
      try {
        sending.value = true
        error.value = null
        const res = await rpcClient.call<{ sent: boolean; message_id: number; channel: number }>({
          method: 'mesh.send-channel',
          params: { channel, message: message.trim() },
        })
        if (res.sent) await refreshMessagesAfterSend()
        return res
      } catch (err: unknown) {
        error.value = err instanceof Error ? err.message : 'Failed to send channel message'
        throw err
      } finally {
        sending.value = false
      }
    }
    const result = sendQueue.then(doSend, doSend)
    sendQueue = result.then(() => {}, () => {})
    return result
  }

  async function broadcastIdentity() {
    try {
      error.value = null
      await rpcClient.call<{ broadcast: boolean }>({ method: 'mesh.broadcast' })
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to broadcast identity'
      throw err
    }
  }

  async function configure(config: MeshConfigureParams) {
    try {
      error.value = null
      await rpcClient.call<{ configured: boolean }>({
        method: 'mesh.configure',
        params: config as Record<string, unknown>,
      })
      await fetchStatus()
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to configure mesh'
      throw err
    }
  }

  async function sendInvoice(contactId: number, amountSats: number, memo?: string) {
    try {
      sending.value = true
      error.value = null
      return await rpcClient.call<{ sent: boolean; amount_sats: number; bolt11: string }>({
        method: 'mesh.send-invoice',
        params: { contact_id: contactId, amount_sats: amountSats, memo },
      })
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to send invoice'
      throw err
    } finally {
      sending.value = false
    }
  }

  async function sendCoordinate(contactId: number, lat: number, lng: number, label?: string) {
    try {
      sending.value = true
      error.value = null
      return await rpcClient.call<{ sent: boolean }>({
        method: 'mesh.send-coordinate',
        params: { contact_id: contactId, lat, lng, label },
      })
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to send coordinate'
      throw err
    } finally {
      sending.value = false
    }
  }

  async function sendAlert(message: string, alertType: string, broadcast = false, lat?: number, lng?: number) {
    try {
      error.value = null
      return await rpcClient.call<{ sent: boolean; signed: boolean }>({
        method: 'mesh.send-alert',
        params: { message, alert_type: alertType, broadcast, lat, lng },
      })
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to send alert'
      throw err
    }
  }

  async function sendContent(contactId: number, cid: string, caption?: string, peerOnion?: string) {
    try {
      sending.value = true
      error.value = null
      const res = await rpcClient.call<{ sent: boolean; message_id: number; cid: string; size: number }>({
        method: 'mesh.send-content',
        params: { contact_id: contactId, cid, caption, peer_onion: peerOnion },
      })
      if (res.sent) await refreshMessagesAfterSend()
      return res
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to send content'
      throw err
    } finally {
      sending.value = false
    }
  }

  async function transportAdvice(contactId: number, size: number) {
    return rpcClient.call<{
      tier: 'auto-mesh' | 'choose' | 'resource-mesh' | 'tor-only' | 'impossible'
      est_seconds: number
      has_tor: boolean
      has_fips: boolean
      last_transport: string | null
      reason: string
      size: number
      mesh_auto_max: number
      mesh_hard_max: number
    }>({
      method: 'mesh.transport-advice',
      params: { contact_id: contactId, size },
    })
  }

  async function sendContentInline(
    contactId: number,
    mime: string,
    bytes: Uint8Array,
    filename?: string,
    caption?: string,
  ) {
    try {
      sending.value = true
      error.value = null
      // Base64-encode bytes for JSON transport.
      let binary = ''
      for (let i = 0; i < bytes.byteLength; i++) binary += String.fromCharCode(bytes[i]!)
      const bytes_b64 = btoa(binary)
      const res = await rpcClient.call<{ sent: boolean; message_id: number; cid: string; size: number }>({
        method: 'mesh.send-content-inline',
        params: { contact_id: contactId, mime, filename, caption, bytes_b64 },
      })
      if (res.sent) await refreshMessagesAfterSend()
      return res
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to send inline content'
      throw err
    } finally {
      sending.value = false
    }
  }

  async function sendReply(contactId: number, targetPubkey: string, targetSeq: number, text: string) {
    sending.value = true
    try {
      const res = await rpcClient.call<{ sent: boolean; message_id: number; sender_seq: number }>({
        method: 'mesh.send-reply',
        params: { contact_id: contactId, target_pubkey: targetPubkey, target_seq: targetSeq, text },
      })
      if (res.sent) await refreshMessagesAfterSend()
      return res
    } finally {
      sending.value = false
    }
  }

  async function sendReaction(contactId: number, targetPubkey: string, targetSeq: number, emoji: string) {
    try {
      const res = await rpcClient.call<{ sent: boolean; message_id: number; sender_seq: number }>({
        method: 'mesh.send-reaction',
        params: { contact_id: contactId, target_pubkey: targetPubkey, target_seq: targetSeq, emoji },
      })
      if (res.sent) await refreshMessagesAfterSend()
      return res
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to send reaction'
      throw err
    }
  }

  async function rebootRadio(seconds = 2) {
    // Long timeout: Reticulum reboots restart the sidecar daemon and the
    // backend waits for the acknowledgement instead of fire-and-forgetting.
    return rpcClient.call<{ reboot: boolean; seconds: number; message?: string }>({
      method: 'mesh.reboot-radio',
      params: { seconds },
      timeout: 30000,
    })
  }

  /** Persisted RNode RF settings + live radio-confirmed state (Reticulum). */
  async function getRnodeConfig() {
    return rpcClient.call<{
      settings: Record<string, unknown>
      live: Record<string, unknown> | null
      live_error: string | null
    }>({ method: 'mesh.rnode-config', timeout: 20000 })
  }

  /** Apply RNode RF settings: persists, restarts the radio daemon, waits for
   *  the radio's own read-back confirmation (up to ~50s). */
  async function applyRnodeConfig(settings: Record<string, unknown>) {
    return rpcClient.call<{
      applied: boolean
      confirmed?: boolean
      live?: Record<string, unknown> | null
      message: string
    }>({ method: 'mesh.rnode-config-apply', params: { settings }, timeout: 70000 })
  }

  async function getOutbox() {
    try {
      return await rpcClient.call<{ count: number; messages?: unknown[] }>({ method: 'mesh.outbox' })
    } catch {
      return { count: 0 }
    }
  }

  async function sendReadReceipt(contactId: number, targetPubkey: string, targetSeq: number) {
    try {
      const res = await rpcClient.call<{ sent: boolean }>({
        method: 'mesh.send-read-receipt',
        params: { contact_id: contactId, target_pubkey: targetPubkey, target_seq: targetSeq },
      })
      return res
    } catch {
      // Read receipts are best-effort — never surface errors to the user.
      return null
    }
  }

  async function forwardMessage(contactId: number, sourceMessageId: number) {
    sending.value = true
    try {
      const res = await rpcClient.call<{ sent: boolean; message_id: number }>({
        method: 'mesh.forward-message',
        params: { contact_id: contactId, source_message_id: sourceMessageId },
      })
      if (res.sent) await refreshMessagesAfterSend()
      return res
    } finally {
      sending.value = false
    }
  }

  async function editMessage(contactId: number, targetSeq: number, newText: string) {
    try {
      const res = await rpcClient.call<{ sent: boolean; message_id: number }>({
        method: 'mesh.edit-message',
        params: { contact_id: contactId, target_seq: targetSeq, new_text: newText },
      })
      if (res.sent) await refreshMessagesAfterSend()
      return res
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to edit message'
      throw err
    }
  }

  async function deleteMessage(contactId: number, targetSeq: number) {
    try {
      const res = await rpcClient.call<{ sent: boolean; message_id: number }>({
        method: 'mesh.delete-message',
        params: { contact_id: contactId, target_seq: targetSeq },
      })
      if (res.sent) await refreshMessagesAfterSend()
      return res
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to delete message'
      throw err
    }
  }

  async function fetchContent(params: {
    cid: string
    sender_onion: string
    cap_token: string
    cap_exp: number
    mime?: string
    filename?: string
  }) {
    return rpcClient.call<{ fetched: boolean; cached: boolean; cid: string; size?: number; mime?: string }>({
      method: 'mesh.fetch-content',
      params,
    })
  }

  async function getSessionStatus(contactId: number): Promise<SessionStatus> {
    return rpcClient.call<SessionStatus>({
      method: 'mesh.session-status',
      params: { contact_id: contactId },
    })
  }

  async function rotatePrekeys() {
    return rpcClient.call<{ rotated: boolean; one_time_prekeys: number }>({
      method: 'mesh.rotate-prekeys',
    })
  }

  // ─── Phase 4: Off-Grid Bitcoin Operations ────────────────────────────

  const deadmanStatus = ref<AlertStatus | null>(null)
  const blockHeaders = ref<BlockHeader[]>([])
  const latestBlockHeight = ref(0)

  // Mesh-AI assistant (issue #50)
  const assistantStatus = ref<AssistantStatus | null>(null)

  async function fetchAssistantStatus() {
    try {
      assistantStatus.value = await rpcClient.call<AssistantStatus>({ method: 'mesh.assistant-status' })
    } catch {
      // Assistant not available (mesh service down)
    }
  }

  async function configureAssistant(config: {
    enabled?: boolean
    model?: string | null
    trusted_only?: boolean
    backend?: string
    allowed_contacts?: string[]
  }) {
    const res = await rpcClient.call<Partial<AssistantStatus>>({
      method: 'mesh.assistant-configure',
      params: config,
    })
    await fetchAssistantStatus()
    return res
  }

  // Scheduled / queued mesh messages (issue #50, phase 1.7)
  const scheduledMessages = ref<ScheduledMessage[]>([])

  async function fetchScheduledMessages() {
    try {
      const res = await rpcClient.call<{ messages: ScheduledMessage[] }>({ method: 'mesh.list-scheduled' })
      scheduledMessages.value = res.messages || []
    } catch {
      scheduledMessages.value = []
    }
  }

  async function scheduleMessage(params: {
    contact_id?: number
    channel?: number
    body: string
    fire_at: number
  }) {
    const res = await rpcClient.call<ScheduledMessage>({ method: 'mesh.schedule-message', params })
    await fetchScheduledMessages()
    return res
  }

  async function cancelScheduledMessage(id: number) {
    await rpcClient.call({ method: 'mesh.cancel-scheduled', params: { id } })
    await fetchScheduledMessages()
  }

  async function fetchDeadmanStatus() {
    try {
      deadmanStatus.value = await rpcClient.call<AlertStatus>({ method: 'mesh.deadman-status', dedup: true })
    } catch {
      // Dead man switch not available
    }
  }

  async function configureDeadman(config: {
    enabled?: boolean
    interval_secs?: number
    lat?: number
    lng?: number
    label?: string
    contacts?: string[]
    custom_message?: string
    auto_gps?: boolean
  }) {
    return rpcClient.call<AlertStatus>({
      method: 'mesh.deadman-configure',
      params: config,
    })
  }

  async function deadmanCheckin() {
    return rpcClient.call<{ checked_in: boolean; time_remaining_secs: number }>({
      method: 'mesh.deadman-checkin',
    })
  }

  async function fetchBlockHeaders(count = 10) {
    try {
      const res = await rpcClient.call<{ headers: BlockHeader[]; latest_height: number; count: number }>({
        method: 'mesh.block-headers',
        params: { count },
        dedup: true,
      })
      blockHeaders.value = res.headers
      latestBlockHeight.value = res.latest_height
    } catch {
      // Block headers not available
    }
  }

  async function relayTransaction(txHex: string, mode: 'archy' | 'broadcast' = 'archy') {
    return rpcClient.call<{ request_id: number; queued: boolean; tx_hex_len: number }>({
      method: 'mesh.relay-tx',
      params: { tx_hex: txHex, relay_mode: mode },
    })
  }

  async function relayLightning(bolt11: string, amountSats: number) {
    return rpcClient.call<{ request_id: number; queued: boolean; amount_sats: number }>({
      method: 'mesh.relay-lightning',
      params: { bolt11, amount_sats: amountSats },
    })
  }

  async function relayStatus(requestId: number) {
    return rpcClient.call<{
      status: 'pending' | 'confirmed' | 'failed' | 'unknown'
      request_id: number
      txid?: string
      error?: string
      error_code?: string
      completed_at?: string
    }>({
      method: 'mesh.relay-status',
      params: { request_id: requestId },
    })
  }

  async function refreshAll() {
    await Promise.all([fetchStatus(), fetchPeers(), fetchMessages(), fetchDeadmanStatus(), fetchBlockHeaders()])
  }

  /** Ask the backend to actively re-query the radio's contact table (and by
   *  extension re-drain daemon events for Reticulum) — the server-side half
   *  of the Refresh button; refreshAll() alone only re-reads caches. */
  async function refreshRadio(): Promise<boolean> {
    try {
      const res = await rpcClient.call<{ refreshed: boolean }>({ method: 'mesh.refresh' })
      return !!res.refreshed
    } catch {
      return false
    }
  }

  return {
    status,
    peers,
    messages,
    loading,
    error,
    sending,
    unreadCounts,
    viewingAtBottom,
    totalUnread,
    nodePositions,
    federatedPositions,
    updateFederatedPositions,
    deadmanStatus,
    blockHeaders,
    latestBlockHeight,
    fetchStatus,
    undismissedDetectedDevices,
    suppressDeviceDetect,
    dismissDetectedDevice,
    flashFlowPath,
    openFlashFlow,
    closeFlashFlow,
    probeDevice,
    flashListFirmware,
    flashDevice,
    flashStatus,
    flashCancel,
    startGlobalDetection,
    fetchPeers,
    fetchMessages,
    sendMessage,
    sendChannelMessage,
    broadcastIdentity,
    configure,
    refreshAll,
    refreshRadio,
    markChatRead,
    clearViewingChat,
    sendInvoice,
    sendCoordinate,
    sendAlert,
    sendContent,
    sendContentInline,
    transportAdvice,
    fetchContent,
    sendReply,
    sendReaction,
    rebootRadio,
    getRnodeConfig,
    applyRnodeConfig,
    getOutbox,
    sendReadReceipt,
    forwardMessage,
    editMessage,
    deleteMessage,
    getSessionStatus,
    rotatePrekeys,
    getNodePositions,
    updateSelfPosition,
    fetchDeadmanStatus,
    configureDeadman,
    deadmanCheckin,
    assistantStatus,
    fetchAssistantStatus,
    configureAssistant,
    scheduledMessages,
    fetchScheduledMessages,
    scheduleMessage,
    cancelScheduledMessage,
    fetchBlockHeaders,
    relayTransaction,
    relayLightning,
    relayStatus,
  }
})
