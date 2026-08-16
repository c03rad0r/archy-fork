// RPC Client for connecting to Archipelago backend

export interface RPCOptions {
  method: string
  params?: Record<string, unknown>
  timeout?: number
  /** Abort the call (and any pending retries) from the outside — pass a
   * component-scoped controller's signal so fan-outs stop on unmount. */
  signal?: AbortSignal
  /** Per-call retry budget (default 3). Use 1 for calls whose caller has its
   * own timeout/fallback UX — retry×3 on a slow peer is how one unreachable
   * node turns a 30s timeout into a 90s spinner. */
  maxRetries?: number
  /** Collapse concurrent identical calls (same method + params) into one
   * request. Opt-in: only safe for reads. */
  dedup?: boolean
}

export interface RPCResponse<T> {
  result?: T
  error?: {
    code: number
    message: string
    data?: unknown
  }
}

// Multi-version support (docs/bitcoin-multi-version-design.md). Mirrors the
// `package.versions` / `package.set-config` RPC payloads.
export interface CatalogVersionInfo {
  version: string
  default: boolean
  deprecated: boolean
  eol: string | null
}

export interface PackageVersionsResponse {
  id: string
  supportsVersions: boolean
  default: string | null
  installedVersion: string | null
  pinnedVersion: string | null
  autoUpdate: boolean
  versions: CatalogVersionInfo[]
}

export interface AppGatePortStatus {
  port: number
  app_id: string
  app_name: string
  /** Login challenge active right now (manifest default + operator override, resolved). */
  gate_enabled: boolean
  /** Operator override on record; null/undefined = manifest default applies. */
  override?: boolean | null
}

export interface AppGateStatusResponse {
  fully_enforced: boolean
  claimed: [number, string][]
  unprotected: { port: number; app_id: string; app_name: string; reason: string }[]
  gated: AppGatePortStatus[]
  exempt: { port: number; app_id: string; protocol: string; rationale: string }[]
}

export interface SetPackageConfigResponse {
  status: 'ok' | 'confirm_required'
  id: string
  // present on status === 'ok'
  pinnedVersion?: string | null
  autoUpdate?: boolean
  versionChanged?: boolean
  recreating?: boolean
  // present on status === 'confirm_required'
  kind?: string
  currentVersion?: string
  targetVersion?: string
  warning?: string
}

/// Mirrors `crate::federation::pending::PendingPeerRequest` on the backend.
export type PendingState = 'pending' | 'sent' | 'approved' | 'rejected' | 'expired'

export interface PendingPeerRequest {
  id: string
  from_nostr_pubkey: string
  from_nostr_npub: string
  from_did: string
  from_name: string | null
  message: string | null
  received_at: string
  state: PendingState
  outbound: boolean
}

function getCsrfToken(): string | null {
  const match = document.cookie.match(/(?:^|;\s*)csrf_token=([^;]+)/)
  if (match) return match[1]!
  // Fallback: check for a meta tag (useful when cookies are blocked or not yet set)
  return document.querySelector('meta[name="csrf-token"]')?.getAttribute('content') ?? null
}

class RPCClient {
  private static _sessionExpiredRedirecting = false
  private baseUrl: string
  /** In-flight dedup map for `dedup: true` calls, keyed method+params. */
  private inflight = new Map<string, Promise<unknown>>()

  constructor(baseUrl: string = '/rpc/v1') {
    this.baseUrl = baseUrl
  }

  async call<T>(options: RPCOptions): Promise<T> {
    if (options.dedup) {
      const key = `${options.method}:${JSON.stringify(options.params ?? {})}`
      const existing = this.inflight.get(key)
      if (existing) return existing as Promise<T>
      const p = this.callInner<T>(options).finally(() => this.inflight.delete(key))
      this.inflight.set(key, p)
      return p
    }
    return this.callInner<T>(options)
  }

  private async callInner<T>(options: RPCOptions): Promise<T> {
    const { method, params = {}, timeout = 15000, signal: external } = options
    const maxRetries = Math.max(1, options.maxRetries ?? 3)

    for (let attempt = 0; attempt < maxRetries; attempt++) {
      if (external?.aborted) throw new Error('Aborted')
      const controller = new AbortController()
      const timeoutId = setTimeout(() => controller.abort(), timeout)
      const onExternalAbort = () => controller.abort()
      external?.addEventListener('abort', onExternalAbort, { once: true })

      try {
        const headers: Record<string, string> = {
          'Content-Type': 'application/json',
        }
        const csrfToken = getCsrfToken()
        if (csrfToken) {
          headers['X-CSRF-Token'] = csrfToken
        }

        const response = await fetch(this.baseUrl, {
          method: 'POST',
          credentials: 'include', // Important for session cookies
          headers,
          body: JSON.stringify({ method, params }),
          signal: controller.signal,
        })

        clearTimeout(timeoutId)
        external?.removeEventListener('abort', onExternalAbort)

        if (!response.ok) {
          // Session expired — debounced redirect to login
          // Use a single shared timeout to prevent redirect storms when
          // multiple parallel requests all get 401 at once
          if (response.status === 401 && method !== 'auth.login') {
            // Clear stale auth immediately — stops App.vue watcher from
            // firing more requests and prevents the router from
            // optimistically navigating to /dashboard
            try { localStorage.removeItem('neode-auth') } catch { /* noop */ }

            const isOnboarding = window.location.pathname.startsWith('/onboarding')
            // Already on the login page: redirecting = a full reload of the
            // page we're on. Any background poll that 401s would refresh
            // /login in a loop forever (seen with the global mesh poll).
            const isLogin = window.location.pathname.startsWith('/login')
            console.warn(`[RPC] 401 on ${method} | path=${window.location.pathname} | onboarding=${isOnboarding} | redirecting=${RPCClient._sessionExpiredRedirecting}`)
            if (!isOnboarding && !isLogin && !RPCClient._sessionExpiredRedirecting) {
              RPCClient._sessionExpiredRedirecting = true
              console.warn(`[RPC] Session expired — redirecting to /login in 300ms`)
              setTimeout(() => {
                window.location.href = '/login'
              }, 300)
            }
            throw new Error('Session expired')
          }
          // 403: read body to distinguish CSRF (retryable) from RBAC (permanent)
          if (response.status === 403) {
            let reason = ''
            try {
              const body: RPCResponse<unknown> = await response.json()
              reason = body.error?.message || ''
            } catch { /* body parse failed */ }

            const isCsrf = !reason || reason.toLowerCase().includes('csrf')
            if (isCsrf && attempt < maxRetries - 1) {
              // CSRF mismatch — cookie may have been updated by a concurrent
              // Set-Cookie response not yet visible to JS. Retry after delay.
              await new Promise((r) => setTimeout(r, 500))
              continue
            }
            throw new Error(reason || `HTTP 403: Forbidden`)
          }
          const err = new Error(`HTTP ${response.status}: ${response.statusText}`)
          const isRetryable = response.status === 502 || response.status === 503
          if (isRetryable && attempt < maxRetries - 1) {
            const delay = 600 * (attempt + 1)
            await new Promise((r) => setTimeout(r, Math.floor(delay * (0.5 + Math.random() * 0.5))))
            continue
          }
          throw err
        }

        const data: RPCResponse<T> = await response.json()

        if (data.error) {
          throw new Error(data.error.message || 'RPC Error')
        }

        return data.result as T
      } catch (error) {
        clearTimeout(timeoutId)
        external?.removeEventListener('abort', onExternalAbort)
        if (error instanceof Error) {
          if (error.name === 'AbortError') {
            // Caller-initiated abort is final — never retried.
            if (external?.aborted) throw new Error('Aborted')
            const timeoutErr = new Error('Request timeout')
            if (attempt < maxRetries - 1) {
              const delay = 600 * (attempt + 1)
              await new Promise((r) => setTimeout(r, Math.floor(delay * (0.5 + Math.random() * 0.5))))
              continue
            }
            throw timeoutErr
          }
          const msg = error.message
          const isRetryable = /502|503|Bad Gateway|fetch|network/i.test(msg)
          if (isRetryable && attempt < maxRetries - 1) {
            const delay = 600 * (attempt + 1)
            await new Promise((r) => setTimeout(r, Math.floor(delay * (0.5 + Math.random() * 0.5))))
            continue
          }
          throw error
        }
        throw new Error('Unknown error occurred')
      }
    }

    throw new Error('Request failed after retries')
  }

  // Convenience methods
  async login(password: string): Promise<{ requires_totp?: boolean } | null> {
    return this.call({
      method: 'auth.login',
      params: {
        password,
      },
    })
  }

  async loginTotp(code: string): Promise<{ success: boolean }> {
    return this.call({ method: 'auth.login.totp', params: { code } })
  }

  async loginBackup(code: string): Promise<{ success: boolean }> {
    return this.call({ method: 'auth.login.backup', params: { code } })
  }

  async totpSetupBegin(password: string): Promise<{
    qr_svg: string
    secret_base32: string
    pending_token: string
  }> {
    return this.call({ method: 'auth.totp.setup.begin', params: { password } })
  }

  async totpSetupConfirm(params: {
    code: string
    password: string
    pendingToken: string
  }): Promise<{ enabled: boolean; backup_codes: string[] }> {
    return this.call({ method: 'auth.totp.setup.confirm', params })
  }

  async totpDisable(password: string, code: string): Promise<{ disabled: boolean }> {
    return this.call({ method: 'auth.totp.disable', params: { password, code } })
  }

  async totpStatus(): Promise<{ enabled: boolean }> {
    return this.call({ method: 'auth.totp.status', params: {} })
  }

  async changePassword(params: {
    currentPassword: string
    newPassword: string
    alsoChangeSsh?: boolean
  }): Promise<{ success: boolean; ssh_updated?: boolean; ssh_error?: string | null }> {
    return this.call({
      method: 'auth.changePassword',
      params: {
        currentPassword: params.currentPassword,
        newPassword: params.newPassword,
        alsoChangeSsh: params.alsoChangeSsh ?? true,
      },
    })
  }

  async logout(): Promise<void> {
    return this.call({
      method: 'auth.logout',
      params: {},
    })
  }

  async completeOnboarding(): Promise<boolean> {
    return this.call({
      method: 'auth.onboardingComplete',
      params: {},
    })
  }

  async isOnboardingComplete(): Promise<boolean> {
    return this.call({
      method: 'auth.isOnboardingComplete',
      params: {},
    })
  }

  async resetOnboarding(): Promise<boolean> {
    return this.call({
      method: 'auth.resetOnboarding',
      params: {},
    })
  }

  // ─── Seed Management ───────────────────────────────────────────────

  async generateSeed(): Promise<{ words: string[] }> {
    return this.call({ method: 'seed.generate' })
  }

  async verifySeed(words: string[], indices: number[]): Promise<{
    verified: boolean
    did: string
    nostr_npub: string
  }> {
    return this.call({ method: 'seed.verify', params: { words, indices } })
  }

  async restoreSeed(words: string[]): Promise<{
    did: string
    nostr_npub: string
    restored: boolean
  }> {
    return this.call({ method: 'seed.restore', params: { words } })
  }

  async saveSeedEncrypted(passphrase: string): Promise<{ saved: boolean }> {
    return this.call({ method: 'seed.save-encrypted', params: { passphrase } })
  }

  async seedStatus(): Promise<{
    has_seed: boolean
    is_legacy: boolean
    identity_count: number
    next_index: number
  }> {
    return this.call({ method: 'seed.status' })
  }

  // ─── Node Identity ───────────────────────────────────────────────

  async getNodeDid(): Promise<{ did: string; pubkey: string; nostr_pubkey?: string; nostr_npub?: string }> {
    return this.call({
      method: 'node.did',
      params: {},
      dedup: true, // read-only identity lookup; safe to collapse concurrent callers (PERF-02)
    })
  }

  async signChallenge(challenge: string): Promise<{ signature: string }> {
    return this.call({
      method: 'node.signChallenge',
      params: { challenge },
    })
  }

  async createBackup(passphrase: string): Promise<{
    version: number
    did: string
    pubkey: string
    kid: string
    encrypted: boolean
    blob: string
    timestamp: string
  }> {
    return this.call({
      method: 'node.createBackup',
      params: { passphrase },
    })
  }

  async resolveDid(did?: string): Promise<Record<string, unknown>> {
    return this.call({
      method: 'identity.resolve-did',
      params: did ? { did } : {},
    })
  }

  async createPresentation(params: {
    holderId: string
    credentialIds: string[]
  }): Promise<Record<string, unknown>> {
    return this.call({
      method: 'identity.create-presentation',
      params: { holder_id: params.holderId, credential_ids: params.credentialIds },
    })
  }

  async verifyPresentation(presentation: Record<string, unknown>): Promise<{
    valid: boolean
    holder_valid: boolean
    credentials: Array<{ id: string; valid: boolean; revoked: boolean }>
  }> {
    return this.call({
      method: 'identity.verify-presentation',
      params: { presentation },
    })
  }

  async createPsbt(params: {
    outputs: Array<{ address: string; amount_sats: number }>
    feeRateSatPerVbyte?: number
  }): Promise<{
    psbt_base64: string
    change_output_index: number
    total_amount_sats: number
    fee_rate_sat_per_vbyte: number
  }> {
    return this.call({
      method: 'lnd.create-psbt',
      params: {
        outputs: params.outputs,
        fee_rate_sat_per_vbyte: params.feeRateSatPerVbyte ?? 10,
      },
    })
  }

  async finalizePsbt(signedPsbtBase64: string): Promise<{
    raw_final_tx: string
    broadcast: boolean
  }> {
    return this.call({
      method: 'lnd.finalize-psbt',
      params: { signed_psbt_base64: signedPsbtBase64 },
    })
  }

  /** Pay a Lightning invoice and resolve it to a REAL terminal state.
   *
   * The backend waits up to 120s on LND's synchronous pay endpoint; if the
   * payment is still routing after that it answers `status: "pending"` with
   * the payment hash instead of an error. This helper then polls
   * lnd.paymentstatus until LND itself reports succeeded/failed, so callers
   * never show "failed" for a payment that is merely slow — the bug where a
   * settling payment was declared failed and then appeared in history a
   * minute later. Returns `pending` only if the payment is STILL in flight
   * after the polling window (rare; caller should say "still settling",
   * not "failed"). */
  async payLightningInvoice(params: {
    payment_request: string
    amount_sats?: number
  }, onPending?: (payment_hash: string) => void): Promise<{
    status: 'succeeded' | 'failed' | 'pending'
    payment_hash: string
    amount_sats: number
    failure_reason?: string
  }> {
    const res = await this.call<{
      status?: string
      payment_hash?: string
      amount_sats?: number
    }>({
      method: 'lnd.payinvoice',
      params,
      // Above the backend's 120s wait so the backend always answers first.
      timeout: 130000,
    })

    const hash = res.payment_hash || ''
    const amount = res.amount_sats || 0
    // Older backends have no status field — a plain response was a success.
    if (res.status !== 'pending') {
      return { status: 'succeeded', payment_hash: hash, amount_sats: amount }
    }
    if (!hash) return { status: 'pending', payment_hash: '', amount_sats: amount }

    // Let the caller unblock its UI right now ("settling…") — the backend
    // answers pending after ~8s, and freezing a modal through the poll loop
    // below was the "paid but the interface stalled" report.
    try { onPending?.(hash) } catch { /* caller UI must not break polling */ }

    // Poll to a terminal state: every 3s for up to 2 minutes.
    for (let i = 0; i < 40; i++) {
      await new Promise((r) => setTimeout(r, 3000))
      try {
        const st = await this.call<{
          status: string
          failure_reason?: string
          amount_sats?: number
        }>({
          method: 'lnd.paymentstatus',
          params: { payment_hash: hash },
          timeout: 15000,
        })
        if (st.status === 'succeeded') {
          return { status: 'succeeded', payment_hash: hash, amount_sats: st.amount_sats || amount }
        }
        if (st.status === 'failed') {
          return {
            status: 'failed',
            payment_hash: hash,
            amount_sats: amount,
            failure_reason: st.failure_reason || 'Payment failed',
          }
        }
      } catch {
        // Transient poll error — keep trying; only LND decides failure.
      }
    }
    return { status: 'pending', payment_hash: hash, amount_sats: amount }
  }

  async publishNostrIdentity(): Promise<{ event_id: string; success: number; failed: number }> {
    return this.call({
      method: 'node.nostr-publish',
      params: {},
    })
  }

  async getNostrPubkey(): Promise<{ nostr_pubkey: string; nostr_npub?: string }> {
    return this.call({
      method: 'node.nostr-pubkey',
      params: {},
    })
  }

  async listPeers(): Promise<{ peers: Array<{ onion: string; pubkey: string; name?: string }> }> {
    return this.call({
      method: 'node-list-peers',
      params: {},
    })
  }

  async addPeer(params: { onion: string; pubkey: string; name?: string }): Promise<{ peers: unknown[] }> {
    return this.call({
      method: 'node-add-peer',
      params,
    })
  }

  async removePeer(pubkey: string): Promise<{ peers: unknown[] }> {
    return this.call({
      method: 'node-remove-peer',
      params: { pubkey },
    })
  }

  async sendMessageToPeer(onion: string, message: string): Promise<{ ok: boolean; sent_to: string }> {
    return this.call({
      method: 'node-send-message',
      params: { onion, message },
      timeout: 90000,
    })
  }

  async checkPeerReachable(onion: string): Promise<{ onion: string; reachable: boolean }> {
    return this.call({
      method: 'node-check-peer',
      params: { onion },
      // Backend health-dial timeout is 12s — 15s here covers RPC overhead.
      timeout: 15000,
    })
  }

  async getReceivedMessages(): Promise<{ messages: Array<{ from_pubkey: string; from_name?: string; message: string; timestamp: string; direction?: string }> }> {
    return this.call({
      method: 'node-messages-received',
      params: {},
    })
  }

  async discoverNodes(): Promise<{ nodes: Array<{ did: string; onion: string; pubkey: string; node_address: string }> }> {
    return this.call({
      method: 'node-nostr-discover',
      params: {},
      timeout: 20000,
    })
  }

  async getTorAddress(): Promise<{ tor_address: string | null }> {
    return this.call({
      method: 'node.tor-address',
      params: {},
      dedup: true, // read-only; near-static value, safe to collapse concurrent callers (PERF-02)
    })
  }

  async torListServices(): Promise<{ services: Array<{ name: string; local_port: number; onion_address: string | null; enabled: boolean }> }> {
    return this.call({ method: 'tor.list-services' })
  }

  async torRotateService(name: string): Promise<{ rotated: boolean; name: string; old_onion: string | null; new_onion: string | null; transition_hours: number }> {
    return this.call({ method: 'tor.rotate-service', params: { name } })
  }

  async torToggleApp(appId: string, enabled: boolean): Promise<{ app_id: string; enabled: boolean; changed: boolean; onion_address: string | null }> {
    return this.call({ method: 'tor.toggle-app', params: { app_id: appId, enabled } })
  }

  async torCleanupRotated(): Promise<{ cleaned: string[]; count: number }> {
    return this.call({ method: 'tor.cleanup-rotated' })
  }

  async verifyNostrRevoked(): Promise<{
    revoked: boolean
    nostr_pubkey: string
    latest_content?: string
    error?: string
  }> {
    return this.call({
      method: 'node-nostr-verify-revoked',
      params: {},
      timeout: 25000,
    })
  }

  async echo(message: string): Promise<string> {
    return this.call({
      method: 'server.echo',
      params: { message },
    })
  }

  async getSystemTime(): Promise<{ now: string; uptime: number }> {
    return this.call({
      method: 'server.time',
      params: {},
    })
  }

  async getMetrics(): Promise<Record<string, unknown>> {
    return this.call({
      method: 'server.metrics',
      params: {},
    })
  }

  async updateServer(marketplaceUrl: string): Promise<'updating' | 'no-updates'> {
    return this.call({
      method: 'server.update',
      params: { 'marketplace-url': marketplaceUrl },
    })
  }

  async detectUsbDevices(): Promise<{
    devices: Array<{
      type: string
      vendor_id: string
      product_id: string
      manufacturer: string
      product: string
    }>
  }> {
    return this.call({
      method: 'system.detect-usb-devices',
      params: {},
    })
  }

  async restartServer(): Promise<void> {
    return this.call({
      method: 'server.restart',
      params: {},
    })
  }

  async shutdownServer(): Promise<void> {
    return this.call({
      method: 'server.shutdown',
      params: {},
    })
  }

  async installPackage(
    id: string,
    marketplaceUrl: string,
    version: string,
  ): Promise<{ status: string; package_id: string }> {
    // Backend is async — returns { status: 'installing' } in <1s after
    // flipping state and spawning the pull/install pipeline. Progress is
    // streamed via WebSocket (install_progress field on the package entry).
    return this.call({
      method: 'package.install',
      params: { id, 'marketplace-url': marketplaceUrl, version },
      timeout: 600000,
    })
  }

  async uninstallPackage(
    id: string,
    options: { preserveData?: boolean } = {},
  ): Promise<{ status: string; package_id: string }> {
    // Backend is async — returns { status: 'removing' } immediately after
    // flipping state. Graceful stop (up to 600s for bitcoin) and data wipe
    // (up to minutes for large chainstate) run in a background task.
    // Progress shown via uninstall_stage field on the package entry.
    return this.call({
      method: 'package.uninstall',
      params: { id, ...(options.preserveData !== undefined ? { preserve_data: options.preserveData } : {}) },
      timeout: 15000,
    })
  }

  async startPackage(id: string): Promise<void> {
    return this.call({
      method: 'package.start',
      params: { id },
      timeout: 15000,
    })
  }

  async stopPackage(id: string): Promise<void> {
    return this.call({
      method: 'package.stop',
      params: { id },
      timeout: 15000,
    })
  }

  async restartPackage(id: string): Promise<void> {
    return this.call({
      method: 'package.restart',
      params: { id },
      timeout: 15000,
    })
  }

  async updatePackage(id: string): Promise<{ status: string; package_id: string }> {
    // Backend is async — returns { status: 'updating' } immediately after
    // flipping state. Pull / stop / recreate / verify runs in background,
    // with rollback-on-failure.
    return this.call({
      method: 'package.update',
      params: { id },
      timeout: 15000,
    })
  }

  // Multi-version support (docs/bitcoin-multi-version-design.md): list the
  // versions a runner may install / switch to for an app, plus the current pin
  // / auto-update preference and the version actually running.
  async getPackageVersions(id: string): Promise<PackageVersionsResponse> {
    return this.call({
      method: 'package.versions',
      params: { id },
      timeout: 15000,
    })
  }

  // Persist a version pin (or un-pin to track latest) and/or the auto-update
  // toggle. A DOWNGRADE returns { status: 'confirm_required', warning } unless
  // confirm:true is passed — the UI warns first, then re-calls with confirm.
  async setPackageConfig(
    id: string,
    opts: { version?: string; autoUpdate?: boolean; confirm?: boolean },
  ): Promise<SetPackageConfigResponse> {
    return this.call({
      method: 'package.set-config',
      params: { id, ...opts },
      timeout: 600000,
    })
  }

  // What the app gate enforces: which app ports are fronted, whether each
  // one's login challenge is active, exemptions with their rationale.
  async getAppGateStatus(): Promise<AppGateStatusResponse> {
    return this.call({
      method: 'security.app-gate-status',
      timeout: 15000,
    })
  }

  // Per-app gate toggle. enabled=true forces the login challenge, false
  // serves the app on its own authentication, null clears the override so
  // the manifest default applies. Live on the next request — no restart.
  async setAppGate(
    id: string,
    enabled: boolean | null,
  ): Promise<{ id: string; override: boolean | null; ports: { port: number; gate_enabled: boolean }[] }> {
    return this.call({
      method: 'security.set-app-gate',
      params: { id, enabled },
      timeout: 15000,
    })
  }

  async checkPackageUpdates(): Promise<{
    status: string
    refreshed: boolean
    catalog_apps?: number
    error?: string
  }> {
    // Refreshes the remote app catalog now (decoupled from the binary OTA).
    // Per-app `available-update` badges repopulate on the next package scan
    // and arrive via the usual WebSocket push.
    return this.call({
      method: 'package.check-updates',
      timeout: 25000,
    })
  }

  async getMarketplace(url: string): Promise<Record<string, unknown>> {
    return this.call({
      method: 'marketplace.get',
      params: { url },
    })
  }

  // Federation
  /** Minting a `trusted` invite requires the node password — the backend
   *  rejects it with a `PASSWORD_REQUIRED` error until one is supplied.
   *  Observer invites never need one. */
  async federationInvite(
    trustLevel: 'trusted' | 'observer' = 'trusted',
    password?: string,
  ): Promise<{ code: string; did: string; onion: string; trust_level: string }> {
    const params: Record<string, unknown> = { trust_level: trustLevel }
    if (password) params.password = password
    return this.call({
      method: 'federation.invite',
      params,
    })
  }

  async federationJoin(code: string): Promise<{
    joined: boolean
    node: { did: string; onion: string; pubkey: string; trust_level: string }
  }> {
    return this.call({
      method: 'federation.join',
      params: { code },
    })
  }

  async meshContactsList(): Promise<{
    contacts: Array<{ pubkey: string; alias?: string | null; notes?: string | null; pinned?: boolean; blocked?: boolean }>
  }> {
    return this.call({ method: 'mesh.contacts-list', params: {}, dedup: true }) // read-only (PERF-02)
  }

  async meshContactsSave(
    pubkey: string,
    alias?: string | null,
  ): Promise<{ saved: boolean; pubkey: string; alias: string | null }> {
    const params: Record<string, unknown> = { pubkey }
    if (alias !== undefined) params.alias = alias
    return this.call({ method: 'mesh.contacts-save', params })
  }

  async federationListNodes(): Promise<{
    nodes: Array<{
      did: string
      pubkey: string
      onion: string
      trust_level: string
      added_at: string
      name?: string
      last_seen?: string
      last_state?: {
        timestamp: string
        node_name?: string
        apps: Array<{ id: string; status: string; version?: string }>
        cpu_usage_percent?: number
        mem_used_bytes?: number
        mem_total_bytes?: number
        disk_used_bytes?: number
        disk_total_bytes?: number
        uptime_secs?: number
        tor_active?: boolean
        nostr_npub?: string
        lat?: number | null
        lon?: number | null
      }
    }>
  }> {
    return this.call({
      method: 'federation.list-nodes',
      params: {},
      dedup: true, // read-only; safe to collapse concurrent callers (PERF-02)
    })
  }

  async federationRemoveNode(did: string): Promise<{ removed: boolean; nodes_remaining: number }> {
    return this.call({
      method: 'federation.remove-node',
      params: { did },
    })
  }

  /** Promotion TO `trusted` requires the node password — the backend rejects
   *  it with a `PASSWORD_REQUIRED` error until one is supplied. Demotion is
   *  never gated: making a peer less privileged must stay easy. */
  async federationSetTrust(
    did: string,
    trustLevel: 'trusted' | 'observer' | 'untrusted',
    password?: string,
  ): Promise<{ updated: boolean; did: string; trust_level: string }> {
    const params: Record<string, unknown> = { did, trust_level: trustLevel }
    if (password) params.password = password
    return this.call({
      method: 'federation.set-trust',
      params,
    })
  }

  // Nostr peer-discovery — see `core/archipelago/src/nostr_handshake.rs`.
  // None of these methods ever exchange the local onion address on a public
  // relay. `handshake.discover` returns presence-only events (DID + npub);
  // `handshake.connect` ships a NIP-44-encrypted PeerRequest with no onion;
  // `handshake.poll` queues inbound requests into the federation pending
  // inbox for manual approval (it does NOT auto-accept).

  async nostrDiscoveryStatus(): Promise<{ enabled: boolean; npub?: string | null; name?: string | null }> {
    return this.call({ method: 'nostr.discovery-status', params: {} })
  }

  async nostrSetDiscovery(enabled: boolean, name?: string): Promise<{ enabled: boolean }> {
    // `name` omitted = backend keeps the stored display name; empty string
    // clears it. Only sent when the caller explicitly provides it.
    return this.call({
      method: 'nostr.set-discovery',
      params: name === undefined ? { enabled } : { enabled, name },
      timeout: 30000,
    })
  }

  async handshakeDiscover(): Promise<{
    nodes: Array<{
      nostr_pubkey: string
      nostr_npub: string
      did: string
      version: string
      name?: string | null
    }>
  }> {
    return this.call({ method: 'handshake.discover', params: {}, timeout: 30000 })
  }

  async handshakeConnect(
    recipient: string,
    message?: string,
    name?: string,
  ): Promise<{ ok: boolean; sent_to: string; id: string }> {
    return this.call({
      method: 'handshake.connect',
      params: {
        recipient_nostr_pubkey: recipient,
        ...(message ? { message } : {}),
        ...(name ? { name } : {}),
      },
      timeout: 30000,
    })
  }

  async handshakePoll(): Promise<{
    polled: number
    new_requests: PendingPeerRequest[]
    applied_invites: string[]
    rejected_outbound: string[]
    skipped: string[]
  }> {
    return this.call({ method: 'handshake.poll', params: {}, timeout: 30000 })
  }

  async federationListPendingRequests(): Promise<{
    requests: PendingPeerRequest[]
  }> {
    return this.call({ method: 'federation.list-pending-requests', params: {} })
  }

  async federationApproveRequest(id: string): Promise<{ approved: boolean; id: string }> {
    return this.call({
      method: 'federation.approve-request',
      params: { id },
      timeout: 30000,
    })
  }

  async federationRejectRequest(
    id: string,
    reason?: string,
    notify = false,
  ): Promise<{ rejected: boolean; id: string }> {
    return this.call({
      method: 'federation.reject-request',
      params: {
        id,
        ...(reason ? { reason } : {}),
        notify,
      },
      timeout: 30000,
    })
  }

  /// Withdraw an outbound peer request we sent. Default notify=true so
  /// the recipient's inbound row disappears from their UI (the main
  /// UX reason to cancel — avoid leaving a stale handshake dangling).
  async federationCancelRequest(
    id: string,
    reason?: string,
    notify = true,
  ): Promise<{ cancelled: boolean; id: string; notified: boolean }> {
    return this.call({
      method: 'federation.cancel-request',
      params: {
        id,
        ...(reason ? { reason } : {}),
        notify,
      },
      timeout: 30000,
    })
  }

  async federationSyncState(): Promise<{
    synced: number
    failed: number
    results: Array<{ did: string; status: string; apps?: number; error?: string }>
  }> {
    return this.call({
      method: 'federation.sync-state',
      params: {},
      timeout: 120000,
    })
  }

  async federationDeployApp(params: {
    did: string
    appId: string
    version?: string
    marketplaceUrl?: string
  }): Promise<{ deployed: boolean; app_id: string; peer_did: string; peer_onion: string }> {
    return this.call({
      method: 'federation.deploy-app',
      params: {
        did: params.did,
        app_id: params.appId,
        version: params.version ?? 'latest',
        marketplace_url: params.marketplaceUrl ?? '',
      },
      timeout: 180000,
    })
  }

  // VPN
  async vpnStatus(signal?: AbortSignal): Promise<{
    connected: boolean
    provider?: string
    interface?: string
    ip_address?: string
    hostname?: string
    peers_connected: number
    bytes_in: number
    bytes_out: number
    configured: boolean
    configured_provider: string
    wg_ip?: string | null
    node_npub?: string | null
    relay_url?: string | null
    relay_onion?: string | null
    relay_direct?: string | null
  }> {
    return this.call({
      method: 'vpn.status',
      params: {},
      signal,
      dedup: true, // read-only status; safe to collapse concurrent callers (PERF-02)
    })
  }

  async vpnConfigure(params: {
    provider: 'tailscale' | 'wireguard'
    auth_key?: string
    address?: string
    dns?: string
    peer?: {
      public_key: string
      endpoint: string
      allowed_ips?: string
      persistent_keepalive?: number
    }
  }): Promise<{ configured: boolean; provider: string; public_key?: string; address?: string }> {
    return this.call({
      method: 'vpn.configure',
      params,
      timeout: 60000,
    })
  }

  async vpnDisconnect(): Promise<{ disconnected: boolean }> {
    return this.call({
      method: 'vpn.disconnect',
      params: {},
    })
  }

  // Marketplace
  async marketplaceDiscover(): Promise<{
    apps: Array<{
      manifest: {
        app_id: string
        name: string
        version: string
        description: { short: string; long: string } | string
        author: { name: string; did: string; nostr_pubkey: string }
        container: { image: string; ports: Array<{ container: number; host: number }>; }
        category: string
        icon_url: string
        repo_url: string
        license: string
      }
      trust_score: number
      trust_tier: string
      relay_count: number
      first_seen: string
      nostr_pubkey: string
      /**
       * Whether the author proved control of the key their `author.did` names.
       * `invalid` manifests are dropped during discovery and should never
       * appear here; typed anyway so the UI fails safe rather than falling
       * through to "signed" if that ever changes.
       */
      signature?:
        | { status: 'valid' }
        | { status: 'missing' }
        | { status: 'invalid'; reason: string }
    }>
    relay_count: number
  }> {
    return this.call({
      method: 'marketplace.discover',
      params: {},
      timeout: 30000,
    })
  }

  // DNS
  async dnsStatus(signal?: AbortSignal): Promise<{
    provider: string
    servers: string[]
    doh_enabled: boolean
    doh_url: string | null
    resolv_conf_servers: string[]
  }> {
    return this.call({
      method: 'network.dns-status',
      params: {},
      signal,
      dedup: true, // read-only status; safe to collapse concurrent callers (PERF-02)
    })
  }

  async configureDns(params: {
    provider: 'system' | 'cloudflare' | 'google' | 'quad9' | 'mullvad' | 'custom'
    servers?: string[]
  }): Promise<{
    ok: boolean
    provider: string
    servers: string[]
    doh_enabled: boolean
    doh_url: string | null
  }> {
    return this.call({
      method: 'network.configure-dns',
      params,
    })
  }

  // Disk management
  async diskStatus(): Promise<{
    used_bytes: number
    total_bytes: number
    free_bytes: number
    used_percent: number
    level: 'ok' | 'warning' | 'critical'
  }> {
    return this.call({ method: 'system.disk-status', dedup: true }) // read-only; safe to collapse concurrent callers (PERF-02)
  }

  async diskCleanup(): Promise<{
    freed_bytes: number
    freed_human: string
    actions: string[]
  }> {
    return this.call({
      method: 'system.disk-cleanup',
      timeout: 60000,
    })
  }

  /** This node's Lightning credential state. Digests and counts only — the
   * backend never returns macaroon content, so nothing here is sensitive. */
  async lndMacaroonStatus(): Promise<LndMacaroonStatus> {
    return this.call({ method: 'lnd.macaroon-status', timeout: 30000 })
  }

  /** Begin a macaroon rotation. Returns as soon as the job is accepted; the
   * work takes minutes (LND has to close and reopen its databases), so poll
   * `lndMacaroonRotationProgress` for the outcome. */
  async lndRotateMacaroons(password: string): Promise<{ status: string }> {
    return this.call({
      method: 'lnd.rotate-macaroons',
      params: { password },
      timeout: 30000,
    })
  }

  async lndMacaroonRotationProgress(): Promise<LndRotationProgress> {
    return this.call({ method: 'lnd.macaroon-rotation-progress' })
  }
}

export type RotationStepState = 'pending' | 'running' | 'done' | 'failed' | 'skipped'

export interface LndRotationStep {
  key: string
  label: string
  state: RotationStepState
  detail: string | null
}

export interface LndRotationProgress {
  running: boolean
  /** null while running, then the verdict. Lets the UI tell "in progress"
   * apart from "finished and failed". */
  ok: boolean | null
  started_at: string | null
  finished_at: string | null
  error: string | null
  steps: LndRotationStep[]
  /** Holds the OLD root key, so it is still secret. The UI tells the operator
   * to delete it once every wallet app has been re-paired. */
  backup_path: string | null
  identity_pubkey: string | null
  channels_before: number | null
  channels_after: number | null
  new_admin_macaroon_sha256: string | null
}

export interface LndMacaroonStatus {
  installed: boolean
  admin_macaroon_sha256: string | null
  /** When LND last minted these credentials, host local time. */
  issued_at: string | null
  identity_pubkey: string | null
  channels_open: number | null
  channels_pending: number | null
  /** Why LND could not be asked, when it could not. */
  lnd_error: string | null
  btcpay_uses_internal_lnd: boolean
  /** null when BTCPay has no internal Lightning node — an absence, not a fault. */
  btcpay_credential_current: boolean | null
  rotation: LndRotationProgress
}

export const rpcClient = new RPCClient()
