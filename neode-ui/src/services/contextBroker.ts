import type { Ref } from 'vue'
import type {
  AIUIRequest,
  ArchyResponse,
  AIContextCategory,
  ArchyContextResponse,
  ArchyActionResponse,
  ArchyChatResponse,
  ArchyChatSurface,
  ArchyContentPush,
} from '@/types/aiui-protocol'
import { useAIPermissionsStore } from '@/stores/aiPermissions'
import { useAppStore } from '@/stores/app'
import { useContainerStore, BUNDLED_APPS } from '@/stores/container'
import { rpcClient } from '@/api/rpc-client'
import { fileBrowserClient } from '@/api/filebrowser-client'
import {
  adaptContentItems,
  adaptLibraryTracks,
  type ArchyContentBundle,
  type ArchyContentItem,
  type ArchyLibraryTrack,
} from '@/composables/archyContentAdapter'

/** Wire shape of one entry in `assistant.chat`'s `surfaces` — the raw
 * result of a content-producing tool the turn ran, exactly as its RPC
 * handler returned it (`crate::assistant::Surface`). Every such handler
 * answers `{ items: [...] }`, which is what `adaptChatSurfaces` reads. */
interface NodeChatSurface {
  tool: string
  scope?: string
  data?: { items?: ArchyContentItem[] }
}

/** Wire shape of `content_owned::OwnedItem` (already-purchased peer
 * content, cached locally). Its fields don't match `ArchyContentItem` —
 * this is intentional; `normalizeOwnedItem` below bridges the gap rather
 * than widening the adapter's input type for one RPC's shape. */
interface OwnedRpcItem {
  onion: string
  content_id: string
  filename: string
  mime_type: string
  size_bytes: number
  paid_sats: number
  purchased_at: string
}

/** An already-purchased item is, by definition, unlocked for this node —
 * map it to `access: 'free'` regardless of what the seller's catalog
 * still shows other buyers, so the adapter doesn't lock a card the user
 * already paid for. */
function normalizeOwnedItem(owned: OwnedRpcItem): ArchyContentItem {
  return {
    id: owned.content_id,
    filename: owned.filename,
    mime_type: owned.mime_type,
    size_bytes: owned.size_bytes,
    description: '',
    access: 'free',
    added_at: owned.purchased_at,
  }
}

/** Wire shape of `assistant.pending`'s response payload: the node-authored
 * description and the node-minted nonce for the one pending destructive
 * action (13-08, D-07/D-11). The description is drawn by the HOST chrome
 * (ToolConfirmModal.vue), never by AIUI — and it reaches the page over the
 * authenticated RPC session only, never over the iframe's postMessage
 * channel, so the frame cannot forge or restyle it. */
interface PendingToolConfirm {
  req_id: string
  nonce: string
  description: string
  tool_name?: string
}

function emptyBundle(): ArchyContentBundle {
  return { films: [], songs: [], podcasts: [], images: [] }
}

function mergeBundles(bundles: ArchyContentBundle[]): ArchyContentBundle {
  return bundles.reduce<ArchyContentBundle>(
    (acc, cur) => ({
      films: [...acc.films, ...cur.films],
      songs: [...acc.songs, ...cur.songs],
      podcasts: [...acc.podcasts, ...cur.podcasts],
      images: [...acc.images, ...cur.images],
    }),
    emptyBundle(),
  )
}

/**
 * Ceiling on one `context:request` gather. Comfortably above a healthy
 * File Browser round trip (login + usage + list) and far below AIUI's own
 * bridge timeout, so a stall surfaces here as an empty category rather than
 * there as a dead request.
 */
const CONTEXT_FETCH_TIMEOUT_MS = 10_000

/**
 * Resolve to `null` if `p` has not settled within `ms`.
 *
 * Deliberately resolves rather than rejects: the caller's job is to always
 * post a `context:response`, and a rejection would just move the problem to
 * a catch block. The pending promise is left to finish on its own — nothing
 * downstream reads it once we have answered.
 */
function withTimeout<T>(p: Promise<T>, ms: number): Promise<T | null> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), ms)
    p.then(
      (v) => {
        clearTimeout(timer)
        resolve(v)
      },
      () => {
        clearTimeout(timer)
        resolve(null)
      },
    )
  })
}

/**
 * Context Broker — mediates all communication between AIUI (iframe) and Archy.
 *
 * AIUI sends context/action requests via postMessage.
 * The broker checks permissions, fetches data from Pinia stores,
 * sanitizes it (strips sensitive fields), and responds.
 */
export class ContextBroker {
  private iframe: Ref<HTMLIFrameElement | null>
  private allowedOrigin: string
  private listener: ((e: MessageEvent) => void) | null = null
  /** Per-(kind, scope) content-request sequence — the AIUI-03 concurrency
   * guard. Each `handleContentRequest` call captures its own sequence number
   * before awaiting the RPC(s); if a newer request FOR THE SAME kind+scope
   * has started by the time it resolves, its result is discarded, so a slow
   * response can never overwrite fresher grid data.
   *
   * Keyed rather than global. A single shared counter made different kinds
   * cancel each other: `useArchy.ts` init fires `content('all','own')` and
   * `library('own')` back to back, both sequence numbers are assigned
   * synchronously before either awaits, so the first request ALWAYS resolved
   * with a stale number and was dropped — films, podcasts and own files never
   * reached the grid and only music ever did. Different kinds populate
   * different grids and cannot stale each other by definition; only a newer
   * request for the same grid can. */
  private contentRequestSeq = new Map<string, number>()

  /** 13-08: how often the broker asks the node for a pending destructive-
   * tool confirmation while a chat turn is in flight. The node's loop is
   * suspended on its confirm gate during that window, so this poll is what
   * turns "the node is waiting on a human" into a visible dialog. */
  private static readonly CONFIRM_POLL_MS = 1200

  /** How long the one-shot aiui:tool-confirm-response listener stays armed
   * before being cleaned up — slightly beyond the node's own CONFIRM_TIMEOUT
   * (300s since 13-08's UAT bump; was 120s), after which the node has
   * already declined the action itself. Must stay ABOVE the node timeout:
   * a TTL below it disarms the listener while the dialog is still
   * legitimately open, and an approve clicked in that window goes nowhere
   * until the next poll re-announces. */
  private static readonly CONFIRM_LISTENER_TTL_MS = 310_000

  private confirmPollTimer: ReturnType<typeof setInterval> | null = null
  /** Chat turns currently in flight — polling runs while > 0. */
  private activeChatTurns = 0
  /** req_ids already announced to the host chrome, so one pending action is
   * dialogued exactly once no matter how many polls observe it. */
  private announcedConfirmReqIds = new Set<string>()

  constructor(iframe: Ref<HTMLIFrameElement | null>, aiuiUrl: string) {
    this.iframe = iframe
    try {
      const url = new URL(aiuiUrl, window.location.origin)
      this.allowedOrigin = url.origin
    } catch {
      this.allowedOrigin = window.location.origin
    }
  }

  start() {
    // Grants live on the NODE, not in this browser. localStorage is per-origin
    // and a node answers on several addresses (LAN, Tailscale, <host>.local),
    // so a perfectly granted permission can read as denied at a second origin.
    // Reconcile ONCE here rather than inside each permission gate: the gates
    // are on the hot path, and an await there would add an RPC to every
    // content and context request.
    void useAIPermissionsStore().hydrate()

    this.listener = (e: MessageEvent) => this.handleMessage(e)
    window.addEventListener('message', this.listener)
  }

  stop() {
    if (this.listener) {
      window.removeEventListener('message', this.listener)
      this.listener = null
    }
    if (this.confirmPollTimer) {
      clearInterval(this.confirmPollTimer)
      this.confirmPollTimer = null
    }
    this.activeChatTurns = 0
  }

  sendPermissionsUpdate() {
    const perms = useAIPermissionsStore()
    this.postToIframe({
      type: 'permissions:update',
      categories: perms.enabledCategories,
    })
  }

  sendTheme() {
    this.postToIframe({
      type: 'theme:response',
      theme: { accent: '#fb923c', mode: 'dark' },
    })
  }

  private handleMessage(event: MessageEvent) {
    if (event.origin !== this.allowedOrigin) return

    const msg = event.data as AIUIRequest
    if (!msg || typeof msg.type !== 'string') return

    switch (msg.type) {
      case 'ready':
        this.sendPermissionsUpdate()
        this.sendTheme()
        break
      case 'context:request':
        this.handleContextRequest(msg.id, msg.category, msg.query)
        break
      case 'action:request':
        this.handleActionRequest(msg.id, msg.action, msg.params)
        break
      case 'theme:request':
        this.sendTheme()
        break
      case 'chat:request':
        this.handleChatRequest(msg.id, msg.text)
        break
      case 'content:request':
        this.handleContentRequest(msg.id, msg.kind, msg.scope)
        break
      // Deliberately NO arm for anything confirmation-shaped (13-08,
      // D-11): a frame message whose `type` resembles a tool confirmation
      // falls through here and is ignored. The confirmation dialog is
      // opened only from assistant.pending's RPC response and resolved
      // only via the host's own aiui:tool-confirm-response CustomEvent —
      // asserted by iframe_message_cannot_open_or_resolve_confirmation.
    }
  }

  // Note: no permission category is threaded through here on purpose.
  // Authority for a chat turn is resolved node-side from the RPC session's
  // CallerScope (assistant.chat, core/archipelago/src/assistant/mod.rs) —
  // duplicating a browser-side gate here would recreate the second,
  // divergent security model D-02 exists to prevent. Do not "helpfully"
  // add a permission check back into this handler.
  private async handleChatRequest(id: string, text: string) {
    // 13-08: while this turn is in flight the node may suspend on its
    // confirm gate waiting for a human — poll assistant.pending so the
    // trusted chrome can draw the dialog (see handleToolConfirmRequest).
    this.beginConfirmPolling()
    try {
      // 13-08 UAT: a chat turn legitimately spans multiple model round
      // trips (ASSISTANT_HTTP_TIMEOUT 180s each) plus a human-speed
      // confirm-gate wait (CONFIRM_TIMEOUT 300s). The rpcClient default
      // (15s) aborted every confirmable turn client-side while the node
      // kept the pending confirmation alive — the modal then re-announced
      // on the next turn, over and over. 420s must stay below AIUI's
      // bridge timeout (430s) so this error, not the bridge's, is the one
      // the user sees.
      const result = await rpcClient.call<{
        text: string
        surfaces?: NodeChatSurface[]
        refused_categories?: string[]
      }>({
        method: 'assistant.chat',
        params: { text },
        timeout: 420_000,
      })
      // A tool was blocked by an ungranted category. Offer the toggle in
      // the TRUSTED chrome — the iframe must not be able to draw anything
      // that looks like a permission prompt (same reasoning as the confirm
      // dialog), and the model's own "I don't have a tool for that" gives
      // the operator no idea the capability is one switch away.
      if (result.refused_categories?.length) {
        window.dispatchEvent(
          new CustomEvent('aiui:permission-needed', {
            detail: { categories: result.refused_categories },
          }),
        )
      }
      this.postToIframe({
        type: 'chat:response',
        id,
        success: true,
        text: result.text,
        surfaces: this.adaptChatSurfaces(result.surfaces),
      } satisfies ArchyChatResponse)
    } catch (err) {
      this.postToIframe({
        type: 'chat:response',
        id,
        success: false,
        error: err instanceof Error ? err.message : 'Chat request failed',
      } satisfies ArchyChatResponse)
    } finally {
      this.endConfirmPolling()
    }
  }

  /**
   * Turn the node's raw content-tool results into the SAME adapted grid
   * records `content:push` already delivers, so AIUI renders them with
   * the film/song/image components it has rather than needing a second,
   * chat-only shape.
   *
   * Gated on media/files exactly like `handleContentRequest`: this
   * carries node data (own files, peer catalogues, purchases) into the
   * iframe, so it is a consent surface and is checked HERE rather than
   * trusting the node's own grant check to be the only one. Dropping the
   * surfaces never drops the answer — the prose still goes through.
   */
  private adaptChatSurfaces(surfaces?: NodeChatSurface[]): ArchyChatSurface[] | undefined {
    if (!surfaces?.length) return undefined
    const perms = useAIPermissionsStore()
    if (!perms.isEnabled('media') && !perms.isEnabled('files')) return undefined

    const adapted = surfaces.flatMap((s) => {
      const items = Array.isArray(s.data?.items) ? s.data.items : []
      if (!items.length) return []
      // The adapter uses `source` to decide the badge and how a playable
      // URL is built, so a wrong value renders a peer's paid item as
      // freely local — map each scope to what it actually is. Two scopes
      // need more than a label:
      //
      // `purchased` arrives as OwnedRpcItem (content_id/onion/paid_sats),
      // not ArchyContentItem — normalize per item AND carry the seller's
      // onion, or the card adapts with id=undefined and url='' and renders
      // as a permanent placeholder (the live "shows a placeholder" bug).
      if (s.scope === 'purchased') {
        const bundles = (items as unknown as OwnedRpcItem[]).map((owned) =>
          adaptContentItems([normalizeOwnedItem(owned)], { source: 'peer', peerOnion: owned.onion }),
        )
        return [{ tool: s.tool, scope: s.scope, bundle: mergeBundles(bundles) }]
      }
      // `peers` items each carry their seller's onion (the node stamps
      // `peer` per item in the fan-out) — adapt per-peer or every URL
      // comes out '' (`buildMediaUrl` refuses peer URLs without one).
      if (s.scope === 'peers') {
        const byOnion = new Map<string, ArchyContentItem[]>()
        for (const it of items) {
          const peer: unknown = (it as { peer?: unknown }).peer
          const onion = typeof peer === 'string' ? peer : ''
          if (!onion) continue
          byOnion.set(onion, [...(byOnion.get(onion) ?? []), it])
        }
        const bundles = [...byOnion.entries()].map(([onion, its]) =>
          adaptContentItems(its, { source: 'peer', peerOnion: onion }),
        )
        return [{ tool: s.tool, scope: s.scope, bundle: mergeBundles(bundles) }]
      }
      const source = s.scope === 'films' ? 'indeehub' : 'own'
      return [{ tool: s.tool, scope: s.scope, bundle: adaptContentItems(items, { source }) }]
    })
    return adapted.length ? adapted : undefined
  }

  private beginConfirmPolling() {
    this.activeChatTurns += 1
    if (this.confirmPollTimer) return
    this.confirmPollTimer = setInterval(() => {
      void this.checkPendingConfirmation()
    }, ContextBroker.CONFIRM_POLL_MS)
  }

  private endConfirmPolling() {
    this.activeChatTurns = Math.max(0, this.activeChatTurns - 1)
    if (this.activeChatTurns === 0 && this.confirmPollTimer) {
      clearInterval(this.confirmPollTimer)
      this.confirmPollTimer = null
      // The turn is over, so any confirmation still on screen is dead —
      // either it was resolved (dialog already closed) or the node's
      // timeout declined it while the human was still reading.
      this.expireStaleConfirms()
    }
  }

  private async checkPendingConfirmation() {
    try {
      const res = await rpcClient.call<{ pending: PendingToolConfirm | null }>({
        method: 'assistant.pending',
      })
      this.expireStaleConfirms(res?.pending?.req_id)
      if (res?.pending) this.handleToolConfirmRequest(res.pending)
    } catch {
      // Transient RPC failure — the next poll retries; the node's own
      // timeout is the backstop, and it declines rather than approves.
    }
  }

  /** 13-08 on-device UAT: when a pending confirmation vanishes node-side
   * (timed out, or resolved from another session) while the trusted chrome
   * still shows its dialog, tell the chrome to close it. An approval
   * clicked after expiry can only be refused by the node — leaving the
   * dialog up invites exactly that dead click. Same host-only CustomEvent
   * discipline as the request/response pair: the iframe has no path to
   * dispatch or observe this. */
  private expireStaleConfirms(currentReqId?: string) {
    for (const id of [...this.announcedConfirmReqIds]) {
      if (id === currentReqId) continue
      this.announcedConfirmReqIds.delete(id)
      window.dispatchEvent(new CustomEvent('aiui:tool-confirm-expired', { detail: { reqId: id } }))
    }
  }

  /**
   * 13-08 (D-07/D-11): announce one node-reported pending confirmation to
   * the trusted chrome, and arm a one-shot listener for the host's answer.
   *
   * Anti-spoofing invariants, all load-bearing:
   * - The description and nonce arrive here ONLY from assistant.pending's
   *   RPC response — never from the iframe (there is no handleMessage arm
   *   for anything confirmation-shaped, deliberately).
   * - The CustomEvent pair (`aiui:tool-confirm-request` /
   *   `aiui:tool-confirm-response`) is NEW and distinct from the
   *   install-app pair — an install confirmation and a tool confirmation
   *   must never be interchangeable.
   * - The response listener listens for the host page's own CustomEvent on
   *   window. An iframe cannot dispatch that (its postMessage arrives as a
   *   MessageEvent, which this method never reads), so the decision path
   *   is host-only.
   * - The user's decision travels to the node over the authenticated RPC
   *   session (assistant.confirm-tool) carrying the node-minted nonce —
   *   never back through the frame.
   * - No response is ever synthesized: if the host closes the dialog
   *   without deciding, nothing is sent, and the node's own timeout
   *   declines the action.
   */
  handleToolConfirmRequest(pending: PendingToolConfirm) {
    if (!pending?.req_id || !pending.nonce || typeof pending.description !== 'string') return
    if (this.announcedConfirmReqIds.has(pending.req_id)) return
    this.announcedConfirmReqIds.add(pending.req_id)

    const reqId = pending.req_id
    const nonce = pending.nonce

    const responseHandler = (e: Event) => {
      const detail = (e as CustomEvent).detail as { reqId?: string; approved?: boolean }
      if (detail?.reqId !== reqId) return
      window.removeEventListener('aiui:tool-confirm-response', responseHandler)
      void rpcClient
        .call({
          method: 'assistant.confirm-tool',
          params: { req_id: reqId, nonce, approved: detail.approved === true },
        })
        .catch(() => {
          // A refused resolution (stale nonce, already timed out) is the
          // node protecting itself — nothing to retry from here.
        })
    }
    window.addEventListener('aiui:tool-confirm-response', responseHandler)
    setTimeout(() => {
      window.removeEventListener('aiui:tool-confirm-response', responseHandler)
      this.announcedConfirmReqIds.delete(reqId)
    }, ContextBroker.CONFIRM_LISTENER_TTL_MS)

    // Only node-fetched values travel to the chrome — and not the nonce:
    // it stays in this closure and reappears only on the RPC call above.
    window.dispatchEvent(
      new CustomEvent('aiui:tool-confirm-request', {
        detail: { reqId, description: pending.description, toolName: pending.tool_name },
      }),
    )
  }

  // Content surfaces (D-12/D-14, AIUI-03) — a single generic channel with a
  // `kind` discriminator rather than one channel per content type, so
  // 13-11's music-library wave can extend `kind` without touching this
  // file again. Unlike `chat:request`, this channel carries node data
  // (peer files, this node's own shared files, IndeeHub, owned/paid
  // content) INTO the iframe, so it is a consent surface: gated on the
  // media/files permission categories, checked here rather than trusting
  // AIUI to have asked honestly (T-13-33).
  private async handleContentRequest(id: string, kind: string, scope?: string) {
    const perms = useAIPermissionsStore()
    if (!perms.isEnabled('media') && !perms.isEnabled('files')) {
      this.postToIframe({
        type: 'content:push',
        id,
        kind,
        permitted: false,
        ...emptyBundle(),
      } satisfies ArchyContentPush)
      return
    }

    const requestedScope: 'own' | 'peers' | 'owned' =
      scope === 'peers' || scope === 'owned' ? scope : 'own'
    const seqKey = `${kind}:${requestedScope}`
    const seq = (this.contentRequestSeq.get(seqKey) ?? 0) + 1
    this.contentRequestSeq.set(seqKey, seq)

    // 13-11: 'library' is the one kind value this wave adds — it resolves
    // to music.list-tracks (real tag-extracted metadata) instead of
    // content.* (see aiui-protocol.ts's AIUIContentRequest doc comment).
    const bundle =
      kind === 'library' ? await this.fetchLibraryContent() : await this.fetchAdaptedContent(requestedScope)

    // Stale-response guard: a newer content:request for this same kind+scope
    // has since started — discard this result instead of flipping that grid
    // back to older data (AIUI-03 concurrency edge).
    if (seq !== this.contentRequestSeq.get(seqKey)) return

    this.postToIframe({
      type: 'content:push',
      id,
      kind,
      permitted: true,
      ...bundle,
    } satisfies ArchyContentPush)
  }

  /**
   * Resolve one RPC call (or, for `peers`, a fan-out over every known
   * federation peer) into an adapted content bundle. The iframe never
   * chooses the RPC method or its params — only `scope` (an enum) reaches
   * here, and this function is the only place that turns it into a
   * `content.*` method name (T-13-34).
   */
  private async fetchAdaptedContent(scope: 'own' | 'peers' | 'owned'): Promise<ArchyContentBundle> {
    try {
      if (scope === 'own') {
        const res = await rpcClient.call<{ items: ArchyContentItem[] }>({ method: 'content.list-mine' })
        return adaptContentItems(res.items ?? [], { source: 'own' })
      }

      if (scope === 'owned') {
        const res = await rpcClient.call<{ items: OwnedRpcItem[] }>({ method: 'content.owned-list' })
        return mergeBundles(
          (res.items ?? []).map((owned) =>
            adaptContentItems([normalizeOwnedItem(owned)], { source: 'peer', peerOnion: owned.onion }),
          ),
        )
      }

      // scope === 'peers' — aggregate every known federation peer's catalog.
      // Any single peer's browse failing (offline, Tor timeout) does not
      // fail the whole request; it just contributes an empty bundle.
      const { nodes } = await rpcClient.federationListNodes()
      const onions = (nodes ?? []).map((n) => n.onion).filter((onion): onion is string => !!onion)
      const perPeer = await Promise.all(
        onions.map((onion) =>
          rpcClient
            .call<{ items: ArchyContentItem[] }>({ method: 'content.browse-peer', params: { onion } })
            .then((res) => adaptContentItems(res.items ?? [], { source: 'peer', peerOnion: onion }))
            .catch(() => emptyBundle()),
        ),
      )
      return mergeBundles(perPeer)
    } catch {
      return emptyBundle()
    }
  }

  /**
   * 13-11: resolve a `content:request` whose `kind` is `'library'`. Calls
   * `music.list-tracks` (13-07) directly, not `content.*` — a library
   * track carries real tag-extracted metadata (artist/album/duration)
   * `ContentItem` has no field for at all, so `adaptContentItems`'s
   * generic mapping cannot produce it (`adaptToSong` always sets
   * `artist: ''`). `music.*` rides the same authenticated session as every
   * other RPC this broker calls; no separate permission check is added
   * here beyond `handleContentRequest`'s existing media/files gate, which
   * already ran before this is reached. A page cap of 500 matches
   * `music.list-tracks`'s own `MAX_TRACK_LIMIT` (T-13-41/T-13-73) — this
   * is the one-page-at-a-time truth the RPC itself enforces, not a second
   * cap invented here. Any failure (no index yet, RPC error) degrades to
   * an empty songs bucket rather than failing the whole request, matching
   * `fetchAdaptedContent`'s own error handling below.
   */
  private async fetchLibraryContent(): Promise<ArchyContentBundle> {
    try {
      const res = await rpcClient.call<{ tracks: ArchyLibraryTrack[] }>({
        method: 'music.list-tracks',
        params: { limit: 500 },
      })
      return { ...emptyBundle(), songs: adaptLibraryTracks(res.tracks ?? []) }
    } catch {
      return emptyBundle()
    }
  }

  private async handleContextRequest(id: string, category: AIContextCategory, query?: string) {
    const perms = useAIPermissionsStore()

    if (!perms.isEnabled(category)) {
      this.postToIframe({
        type: 'context:response',
        id,
        data: null,
        permitted: false,
      } satisfies ArchyContextResponse)
      return
    }

    // Always answer, even if the gatherer never settles. `sanitizeFiles`
    // makes three sequential calls into the File Browser app (login, usage,
    // list) and `sanitizeSystem` awaits the app store; a connection that
    // HANGS rather than erroring leaves their promise pending forever, so no
    // `context:response` is ever posted and the AIUI side waits out its own
    // bridge timeout instead. That is the reported "`files` context request
    // times out" — the try/catch in sanitizeFiles only covers rejection, and
    // a stalled socket does not reject.
    //
    // A late `null` is safe by the protocol's own shape: the AIUI reader
    // treats a response with no usable `data` as "nothing to show" and logs
    // nothing, exactly as it already does for an empty category.
    const data = await withTimeout(this.fetchAndSanitize(category, query), CONTEXT_FETCH_TIMEOUT_MS)
    this.postToIframe({
      type: 'context:response',
      id,
      data,
      permitted: true,
    } satisfies ArchyContextResponse)
  }

  private handleActionRequest(id: string, action: string, params: Record<string, string>) {
    const appStore = useAppStore()
    let success = false
    let error: string | undefined

    try {
      switch (action) {
        case 'navigate':
          if (params.path) {
            window.dispatchEvent(new CustomEvent('aiui:navigate', { detail: params.path }))
            success = true
          } else {
            error = 'Missing path parameter'
          }
          break

        case 'open-app':
        case 'launch-app':
          if (params.appId) {
            const url = this.getAppUrl(params.appId)
            if (url) {
              window.dispatchEvent(new CustomEvent('aiui:open-app', { detail: params.appId }))
              success = true
            } else {
              error = `App "${params.appId}" not found or not running`
            }
          } else {
            error = 'Missing appId parameter'
          }
          break

        case 'install-app':
          if (params.appId && params.marketplaceUrl && params.version) {
            const packages = appStore.packages || {}
            const existing = packages[params.appId]
            if (existing && existing.state === 'installed') {
              this.postToIframe({
                type: 'action:response',
                id,
                success: false,
                error: `${params.appId} is already installed`,
              } satisfies ArchyActionResponse)
              return
            }
            // Capture values for use in closure
            const appId = params.appId
            const marketplaceUrl = params.marketplaceUrl
            const version = params.version
            // Emit event for UI confirmation instead of installing directly
            window.dispatchEvent(new CustomEvent('aiui:install-request', {
              detail: { requestId: id, appId, marketplaceUrl, version },
            }))
            {
              const broker = this
              const responseHandler = (e: Event) => {
                const detail = (e as CustomEvent).detail as { requestId: string; confirmed: boolean }
                if (detail.requestId !== id) return
                window.removeEventListener('aiui:install-response', responseHandler)
                if (detail.confirmed) {
                  appStore.installPackage(appId, marketplaceUrl, version).then(() => {
                    broker.postToIframe({
                      type: 'action:response',
                      id,
                      success: true,
                    } satisfies ArchyActionResponse)
                  }).catch((err: Error) => {
                    broker.postToIframe({
                      type: 'action:response',
                      id,
                      success: false,
                      error: err.message,
                    } satisfies ArchyActionResponse)
                  })
                } else {
                  broker.postToIframe({
                    type: 'action:response',
                    id,
                    success: false,
                    error: 'User declined the installation',
                  } satisfies ArchyActionResponse)
                }
              }
              window.addEventListener('aiui:install-response', responseHandler)
              setTimeout(() => {
                window.removeEventListener('aiui:install-response', responseHandler)
              }, 60000)
            }
            return
          }
          error = 'Missing required parameters (appId, marketplaceUrl, version)'
          break

        case 'search-web':
          if (params.query) {
            this.handleSearchAction(id, params.query)
            return
          }
          error = 'Missing query parameter'
          break

        case 'read-file':
          this.handleReadFileAction(id, params.path)
          return

        case 'tail-logs':
          this.handleTailLogsAction(id, params.appId, params.lines)
          return

        default:
          error = `Unknown action: ${action}`
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Unknown error'
    }

    this.postToIframe({
      type: 'action:response',
      id,
      success,
      error,
    } satisfies ArchyActionResponse)
  }

  private async handleSearchAction(id: string, query: string) {
    const appStore = useAppStore()
    const packages = appStore.packages || {}
    const searxng = packages['searxng']

    if (!searxng || searxng.state !== 'installed' || searxng.installed?.status !== 'running') {
      this.postToIframe({
        type: 'action:response',
        id,
        success: false,
        error: 'SearXNG is not installed or not running',
      } satisfies ArchyActionResponse)
      return
    }

    try {
      const response = await fetch(`/apps/searxng/search?q=${encodeURIComponent(query)}&format=json`)
      const results: unknown = await response.json()
      this.postToIframe({
        type: 'action:response',
        id,
        success: true,
        data: results,
      } as ArchyActionResponse & { data: unknown })
    } catch (err) {
      this.postToIframe({
        type: 'action:response',
        id,
        success: false,
        error: err instanceof Error ? err.message : 'Search failed',
      } satisfies ArchyActionResponse)
    }
  }

  private getAppUrl(appId: string): string | null {
    const appStore = useAppStore()
    const packages = appStore.packages || {}
    const pkg = packages[appId]
    if (pkg?.installed?.status === 'running') {
      const ifaces = pkg.installed['interface-addresses']
      if (ifaces) {
        const main = ifaces['main'] || Object.values(ifaces)[0]
        if (main?.['lan-address']) return main['lan-address']
      }
    }
    const containerStore = useContainerStore()
    const containers = containerStore.containers
    const container = containers.find(c => c.name === appId || c.name === `archy-${appId}`)
    if (container?.lan_address) return container.lan_address
    const bundled = BUNDLED_APPS.find(a => a.id === appId)
    if (bundled?.ports?.[0]) return `/apps/${appId}/`
    return null
  }

  private async fetchAndSanitize(category: AIContextCategory, _query?: string): Promise<unknown> {
    const appStore = useAppStore()

    switch (category) {
      case 'apps': return this.sanitizeApps(appStore)
      case 'system': return await this.sanitizeSystem(appStore)
      case 'network': return this.sanitizeNetwork(appStore)
      case 'wallet': return this.sanitizeWallet(appStore)
      case 'files': return this.sanitizeFiles()
      case 'bitcoin': return this.sanitizeBitcoin(appStore)
      case 'media': return this.sanitizeMedia(appStore)
      case 'search': return this.sanitizeSearch(appStore)
      case 'ai-local': return this.sanitizeAILocal(appStore)
      case 'notes': return this.sanitizeNotes()
      default: return null
    }
  }

  // T4: Enhanced apps with version, health, URL, web UI info
  private sanitizeApps(store: ReturnType<typeof useAppStore>): unknown {
    const packages = store.packages || {}
    const containerStore = useContainerStore()

    const apps = Object.entries(packages).map(([id, pkg]) => {
      const hasWebUI = !!pkg.manifest?.interfaces?.main?.ui
      const url = hasWebUI ? `/apps/${id}/` : null
      return {
        id,
        name: pkg.manifest?.title || id,
        version: pkg.manifest?.version || 'unknown',
        state: pkg.state || 'unknown',
        status: pkg.installed?.status || 'unknown',
        hasWebUI,
        url,
      }
    })

    const bundledApps = containerStore.containers.map(c => ({
      id: c.name,
      name: BUNDLED_APPS.find(b => b.id === c.name)?.name || c.name,
      state: c.state === 'running' ? 'installed' : 'stopped',
      status: c.state,
      hasWebUI: !!(BUNDLED_APPS.find(b => b.id === c.name)?.ports?.length),
      url: c.lan_address || null,
    }))

    return [...apps, ...bundledApps]
  }

  // T5: Real system metrics from RPC
  private async sanitizeSystem(store: ReturnType<typeof useAppStore>): Promise<unknown> {
    const info = store.serverInfo
    const base = {
      version: info?.version || 'unknown',
      name: info?.name || 'Archipelago',
    }

    try {
      const [metrics, time] = await Promise.all([
        rpcClient.call<{ cpu: number; disk: { used: number; total: number }; memory: { used: number; total: number } }>({ method: 'server.metrics' }),
        rpcClient.call<{ now: string; uptime: number }>({ method: 'server.time' }),
      ])
      return {
        ...base,
        cpu: metrics.cpu,
        memory: { used: metrics.memory.used, total: metrics.memory.total },
        disk: { used: metrics.disk.used, total: metrics.disk.total },
        uptime: time.uptime,
      }
    } catch {
      return { ...base, status: 'metrics unavailable' }
    }
  }

  // T6: Network with peer count and Tor/Tailscale status
  private sanitizeNetwork(store: ReturnType<typeof useAppStore>): unknown {
    const info = store.serverInfo
    const containerStore = useContainerStore()
    const tailscale = containerStore.containers.find(c => c.name === 'tailscale')
    const hasTor = !!info?.['tor-address']

    return {
      connected: store.isConnected,
      torConnected: hasTor,
      tailscaleActive: tailscale?.state === 'running',
    }
  }

  // T7: Bitcoin status + deep data from backend RPC
  private async sanitizeBitcoin(store: ReturnType<typeof useAppStore>): Promise<unknown> {
    const packages = store.packages || {}
    const containerStore = useContainerStore()

    const btcPkg = packages['bitcoind'] || packages['bitcoin-core'] || packages['bitcoin']
    const btcContainer = containerStore.containers.find(c =>
      c.name === 'bitcoin-knots' || c.name === 'archy-bitcoin-knots'
    )

    const isRunning = (btcPkg?.installed?.status === 'running') ||
      (btcContainer?.state === 'running')

    if (!isRunning) {
      return { available: false, message: 'Bitcoin Core not running' }
    }

    try {
      const info = await rpcClient.call<{
        block_height: number
        sync_progress: number
        chain: string
        difficulty: number
        mempool_size: number
        mempool_tx_count: number
        verification_progress: number
      }>({ method: 'bitcoin.getinfo' })
      return { available: true, status: 'running', ...info }
    } catch {
      return { available: true, status: 'running', network: 'mainnet' }
    }
  }

  // T8: Media libraries from installed media apps
  private sanitizeMedia(store: ReturnType<typeof useAppStore>): unknown {
    const packages = store.packages || {}
    const mediaAppIds = ['plex', 'jellyfin', 'navidrome', 'nextcloud']
    const libraries: { source: string; name: string; status: string }[] = []

    for (const id of mediaAppIds) {
      const pkg = packages[id]
      if (pkg && (pkg.state === 'installed' || pkg.state === 'running' || pkg.state === 'stopped')) {
        libraries.push({
          source: id,
          name: pkg.manifest?.title || id,
          status: pkg.state,
        })
      }
    }

    if (libraries.length === 0) {
      return {
        available: false,
        libraries: [],
        message: 'No media apps installed. Install Plex or Jellyfin from the App Store.',
      }
    }
    return { available: true, libraries }
  }

  // T9: Files from FileBrowser
  private async sanitizeFiles(): Promise<unknown> {
    try {
      if (!fileBrowserClient.isAuthenticated) {
        const ok = await fileBrowserClient.login()
        if (!ok) return { available: false, message: 'File browser authentication failed' }
      }
      const usage = await fileBrowserClient.getUsage()
      const items = await fileBrowserClient.listDirectory('/')
      const folders = items.filter(i => i.isDir).map(i => ({ name: i.name, path: i.path }))
      const recentFiles = items
        .filter(i => !i.isDir)
        .sort((a, b) => new Date(b.modified).getTime() - new Date(a.modified).getTime())
        .slice(0, 10)
        .map(i => ({ name: i.name, path: i.path, size: i.size, modified: i.modified }))
      return {
        available: true,
        totalSize: usage.totalSize,
        folderCount: usage.folderCount,
        fileCount: usage.fileCount,
        folders,
        recentFiles,
      }
    } catch {
      return { available: false, message: 'File browser not reachable' }
    }
  }

  // T10: SearXNG search engine availability
  private sanitizeSearch(store: ReturnType<typeof useAppStore>): unknown {
    const packages = store.packages || {}
    const searxng = packages['searxng']
    if (!searxng || searxng.state !== 'installed' || searxng.installed?.status !== 'running') {
      return { available: false }
    }
    return { available: true, engine: 'searxng', endpoint: '/apps/searxng/' }
  }

  // T11: Ollama local AI models
  private sanitizeAILocal(store: ReturnType<typeof useAppStore>): unknown {
    const packages = store.packages || {}
    const ollama = packages['ollama']
    if (!ollama || ollama.state !== 'installed' || ollama.installed?.status !== 'running') {
      return { available: false }
    }
    return {
      available: true,
      models: [],
      message: 'Ollama is running. Query /api/tags for model list.',
    }
  }

  // T12: Wallet — LND deep data from backend RPC
  private async sanitizeWallet(store: ReturnType<typeof useAppStore>): Promise<unknown> {
    const packages = store.packages || {}
    const containerStore = useContainerStore()

    const lndPkg = packages['lnd']
    const lndContainer = containerStore.containers.find(c =>
      c.name === 'lnd' || c.name === 'archy-lnd'
    )

    const isRunning = (lndPkg?.installed?.status === 'running') ||
      (lndContainer?.state === 'running')

    if (!isRunning) {
      return { available: false, message: 'Lightning (LND) not running' }
    }

    try {
      const info = await rpcClient.call<{
        alias: string
        num_active_channels: number
        num_peers: number
        synced_to_chain: boolean
        block_height: number
        balance_sats: number
        channel_balance_sats: number
        pending_open_balance: number
      }>({ method: 'lnd.getinfo' })
      return { available: true, status: 'running', ...info }
    } catch {
      return { available: true, status: 'running', message: 'LND is running but detailed info unavailable' }
    }
  }

  // T13: Notes/documents
  private sanitizeNotes(): unknown {
    return {
      available: false,
      documents: [],
      message: 'No note-taking apps installed',
    }
  }

  private static readonly ALLOWED_FILE_DIRS = [
    '/var/lib/archipelago/',
    '/var/log/',
    '/opt/archipelago/',
    '/home/archipelago/',
  ]

  private static readonly SENSITIVE_PATH_PATTERNS = [
    'id_rsa', 'id_ed25519', 'private', 'secret', 'password',
    'seed', '.env', 'wallet', 'macaroon', 'tls.key', 'tls.cert',
    'credentials', 'keystore', 'mnemonic',
  ]

  private isPathAllowed(path: string): boolean {
    const normalized = path.replace(/\/+/g, '/').replace(/\.\.\//g, '')
    const inAllowedDir = ContextBroker.ALLOWED_FILE_DIRS.some(dir => normalized.startsWith(dir))
    if (!inAllowedDir) return false
    const lower = normalized.toLowerCase()
    return !ContextBroker.SENSITIVE_PATH_PATTERNS.some(pattern => lower.includes(pattern))
  }

  private async handleReadFileAction(id: string, path?: string) {
    const perms = useAIPermissionsStore()
    if (!perms.isEnabled('files')) {
      this.postToIframe({ type: 'action:response', id, success: false, error: 'File access not permitted' } satisfies ArchyActionResponse)
      return
    }
    if (!path) {
      this.postToIframe({ type: 'action:response', id, success: false, error: 'Missing path parameter' } satisfies ArchyActionResponse)
      return
    }
    if (!this.isPathAllowed(path)) {
      this.postToIframe({ type: 'action:response', id, success: false, error: 'Access denied: path is outside allowed directories or contains sensitive patterns' } satisfies ArchyActionResponse)
      return
    }
    try {
      if (!fileBrowserClient.isAuthenticated) {
        const ok = await fileBrowserClient.login()
        if (!ok) throw new Error('FileBrowser authentication failed')
      }
      const result = await fileBrowserClient.readFileAsText(path)
      this.postToIframe({
        type: 'action:response', id, success: true,
        data: { content: result.content, truncated: result.truncated, size: result.size, path },
      } as ArchyActionResponse)
    } catch (err) {
      this.postToIframe({
        type: 'action:response', id, success: false,
        error: err instanceof Error ? err.message : 'Failed to read file',
      } satisfies ArchyActionResponse)
    }
  }

  private async handleTailLogsAction(id: string, appId?: string, linesStr?: string) {
    const perms = useAIPermissionsStore()
    if (!perms.isEnabled('apps')) {
      this.postToIframe({ type: 'action:response', id, success: false, error: 'App access not permitted' } satisfies ArchyActionResponse)
      return
    }
    if (!appId) {
      this.postToIframe({ type: 'action:response', id, success: false, error: 'Missing appId parameter' } satisfies ArchyActionResponse)
      return
    }
    const lines = Math.min(parseInt(linesStr || '50', 10) || 50, 200)
    try {
      const logs = await rpcClient.call<string[]>({ method: 'container-logs', params: { app_id: appId, lines } })
      const redactedLogs = logs.map(line => ContextBroker.redactLogLine(line))
      this.postToIframe({
        type: 'action:response', id, success: true,
        data: { appId, lines: redactedLogs, count: redactedLogs.length },
      } as ArchyActionResponse)
    } catch (err) {
      this.postToIframe({
        type: 'action:response', id, success: false,
        error: err instanceof Error ? err.message : 'Failed to fetch logs',
      } satisfies ArchyActionResponse)
    }
  }

  private static redactLogLine(line: string): string {
    // Redact RPC passwords (e.g., rpcpassword=xxx)
    let redacted = line.replace(/(?:rpcpassword|rpcauth|password|passwd|secret|token|apikey|api_key|macaroon)[\s]*[=:]\s*\S+/gi, '$&'.replace(/[=:]\s*\S+/, '=[REDACTED]'))
    // More targeted: key=value patterns
    redacted = redacted.replace(/((?:password|secret|token|apikey|api_key|macaroon|rpcpassword|rpcauth)\s*[=:]\s*)\S+/gi, '$1[REDACTED]')
    // Redact long hex strings (>32 chars, likely private keys)
    redacted = redacted.replace(/\b[0-9a-fA-F]{64,}\b/g, '[REDACTED_KEY]')
    // Redact base64 macaroon values (long base64 strings)
    redacted = redacted.replace(/\b[A-Za-z0-9+/]{64,}={0,2}\b/g, '[REDACTED_TOKEN]')
    return redacted
  }

  private postToIframe(msg: ArchyResponse) {
    if (!this.iframe.value?.contentWindow) return
    this.iframe.value.contentWindow.postMessage(msg, this.allowedOrigin)
  }
}
