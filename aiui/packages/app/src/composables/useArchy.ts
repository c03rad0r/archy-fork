import { ref, readonly } from 'vue'
import { archyBridge } from '@/services/archyBridge'
import { useTheme } from '@/composables/useTheme'
import { useContentPanel } from '@/composables/useContentPanel'
import type { Film, Song, Podcast, ImageItem } from '@aiui/core/types/content'
import {
  mockArchyApps, mockArchySystem, mockArchyNetwork,
  mockArchyWallet, mockArchyBitcoin, mockArchyFiles,
} from '@/mocks/archy'

type AIContextCategory = 'apps' | 'system' | 'network' | 'wallet' | 'files' | 'bitcoin'

interface ArchyApp {
  id: string
  name: string
  state: string
  status: string
}

interface ArchySystemInfo {
  version?: string
  name?: string
}

interface ArchyNetworkInfo {
  connected?: boolean
}

export interface ArchyWalletInfo {
  available?: boolean
  status?: string
  alias?: string
  num_active_channels?: number
  num_peers?: number
  synced_to_chain?: boolean
  block_height?: number
  balance_sats?: number
  channel_balance_sats?: number
  pending_open_balance?: number
  message?: string
}

export interface ArchyFileEntry {
  name: string
  path: string
  size?: number
  modified?: string
  type: 'file' | 'folder'
}

export interface ArchyBitcoinInfo {
  available: boolean
  block_height?: number
  sync_progress?: number
  chain?: string
  mempool_tx_count?: number
  mempool_size?: number
}

// Singleton reactive state (shared across all components using this composable)
const isEmbedded = ref(false)
const isInitialized = ref(false)
const permissions = ref<AIContextCategory[]>([])
const accentColor = ref<string | null>(null)
const installedApps = ref<ArchyApp[]>([])
const systemInfo = ref<ArchySystemInfo>({})
const networkInfo = ref<ArchyNetworkInfo>({})
const walletInfo = ref<ArchyWalletInfo>({})
const fileList = ref<ArchyFileEntry[]>([])
const bitcoinInfo = ref<ArchyBitcoinInfo>({ available: false })
let cleanups: (() => void)[] = []

/**
 * Reactive composable wrapping archyBridge for Archy ↔ AIUI integration.
 * Call `init()` once in App.vue when `?embedded=true` is detected.
 */
export function useArchy() {
  /** Initialize the bridge and start listening for Archy messages */
  function init() {
    if (isInitialized.value) return

    const embedded = !!(window as unknown as Record<string, unknown>).__AIUI_EMBEDDED__
    isEmbedded.value = embedded

    // Dev mock mode: load realistic Archy data for standalone testing
    const useMock = import.meta.env.VITE_MOCK_ARCHY === 'true' ||
      new URLSearchParams(window.location.search).has('mockArchy')
    if (useMock && !embedded) {
      isInitialized.value = true
      isEmbedded.value = true
      permissions.value = ['apps', 'system', 'network', 'wallet', 'bitcoin', 'files']
      installedApps.value = mockArchyApps as unknown as ArchyApp[]
      systemInfo.value = mockArchySystem
      networkInfo.value = mockArchyNetwork
      walletInfo.value = mockArchyWallet
      bitcoinInfo.value = mockArchyBitcoin
      fileList.value = mockArchyFiles
      console.log('[AIUI] Mock Archy data loaded for dev testing')
      return
    }

    if (!embedded || !archyBridge.isInArchy()) return

    archyBridge.init()
    isInitialized.value = true

    // Listen for permission updates
    const unsubPerms = archyBridge.onPermissionsUpdate((cats) => {
      permissions.value = cats
      // Auto-fetch context for newly permitted categories
      fetchPermittedContext(cats)
    })
    cleanups.push(unsubPerms)

    // Listen for theme updates — Archy always reports mode:'dark' today, but
    // honor whatever it sends rather than hardcoding that assumption here;
    // App.vue's mount-time isEmbeddedFlag check already forces dark
    // immediately without waiting for this round trip, so this is a
    // corroborating update for whenever it does arrive.
    const unsubTheme = archyBridge.onThemeUpdate((theme) => {
      accentColor.value = theme.accent
      applyAccentColor(theme.accent)
      useTheme().setTheme(theme.mode)
    })
    cleanups.push(unsubTheme)

    // Request theme on init
    archyBridge.requestTheme()

    // 13-11 (GAP-FOUND 2026-08-03): fire the content/library fetch from a
    // real init-time event instead of leaving requestArchyContent /
    // requestArchyLibrary merely callable with nothing in the live UI ever
    // calling them — 13-06 built the whole content:request/content:push
    // machinery and unit-tested it end to end, but nothing in ChatPage.vue's
    // render tree (or anywhere else) ever invoked it, so real node content
    // never appeared no matter how green the tests were (13-06's own Known
    // Limitations). `init()` runs once per embedded session (guarded by
    // `isInitialized` above) and is itself triggered by App.vue mounting —
    // a real UI event, not a user-typed phrase. Fire-and-forget: both
    // functions already resolve to a silent, logged no-op when the user
    // hasn't granted Media/File access (`permitted: false`), so this never
    // throws into `init()`.
    void requestArchyAllContent()
    void requestArchyLibrary('own')
  }

  /** Fetch context for all permitted categories */
  async function fetchPermittedContext(cats: AIContextCategory[]) {
    const fetches: Promise<void>[] = []

    function fetchCategory<T>(cat: AIContextCategory, setter: (data: T) => void, validator: (data: unknown) => boolean = () => true) {
      return archyBridge.requestContext(cat).then((res) => {
        if (!res.permitted) {
          console.warn(`[AIUI Archy] ${cat}: not permitted — user should enable in Archy Settings`)
          return
        }
        if (res.data && validator(res.data)) {
          setter(res.data as T)
        }
      }).catch((err) => {
        console.warn(`[AIUI Archy] ${cat} fetch failed:`, err?.message ?? err)
      })
    }

    if (cats.includes('apps')) {
      fetches.push(fetchCategory('apps', (data) => { installedApps.value = data as ArchyApp[] }, Array.isArray))
    }

    if (cats.includes('system')) {
      fetches.push(fetchCategory('system', (data) => { systemInfo.value = data as ArchySystemInfo }))
    }

    if (cats.includes('network')) {
      fetches.push(fetchCategory('network', (data) => { networkInfo.value = data as ArchyNetworkInfo }))
    }

    if (cats.includes('wallet')) {
      fetches.push(fetchCategory('wallet', (data) => { walletInfo.value = data as ArchyWalletInfo }))
    }

    if (cats.includes('bitcoin')) {
      fetches.push(fetchCategory('bitcoin', (data) => { bitcoinInfo.value = data as ArchyBitcoinInfo }))
    }

    if (cats.includes('files')) {
      fetches.push(fetchCategory('files', (data) => { fileList.value = data as ArchyFileEntry[] }, Array.isArray))
    }

    await Promise.all(fetches)
  }

  /** Refresh context data (call when user returns to chat) */
  async function refreshContext() {
    if (!isInitialized.value) return
    await fetchPermittedContext(permissions.value)
  }

  /** Request Archy to perform an action */
  async function requestAction(action: string, params: Record<string, string> = {}) {
    if (!isInitialized.value) return { success: false, error: 'Not initialized' }
    return archyBridge.requestAction(action, params)
  }

  /** Read a file's text content via FileBrowser */
  async function readFile(path: string): Promise<{ content: string; truncated: boolean; size: number } | null> {
    const res = await requestAction('read-file', { path })
    const data = (res as unknown as Record<string, unknown>).data
    if (res.success && data) {
      return data as { content: string; truncated: boolean; size: number }
    }
    return null
  }

  /** Tail recent logs for an app */
  async function tailLogs(appId: string, lines = 50): Promise<string[] | null> {
    const res = await requestAction('tail-logs', { appId, lines: String(lines) })
    const data = (res as unknown as Record<string, unknown>).data
    if (res.success && data) {
      return (data as { lines: string[] }).lines
    }
    return null
  }

  /**
   * Request a batch of grid-ready content from Archy (D-12/D-14, AIUI-03)
   * and hand it to `useContentPanel`'s `setArchyContent`, which is the
   * source of truth for `FilmGrid`/`SongGrid` once a node has supplied it.
   * Mirrors `archyBridge.requestContext`'s permitted/not-permitted shape —
   * no new convention.
   */
  async function requestArchyContent(
    kind: 'films' | 'songs' | 'podcasts' | 'all' = 'all',
    scope: 'own' | 'peers' | 'owned' = 'own',
  ) {
    if (!isInitialized.value) return
    try {
      const res = await archyBridge.requestArchyContent(kind, scope)
      if (!res.permitted) {
        console.warn('[AIUI Archy] content: not permitted — user should enable Media/File access in Archy Settings')
        return
      }
      const panel = useContentPanel()
      panel.setArchyContent({
        films: res.films as Film[],
        songs: panel.panelSongs.value,
        podcasts: res.podcasts as Podcast[],
        images: res.images as ImageItem[],
      })
    } catch (err) {
      console.warn('[AIUI Archy] content fetch failed:', (err as Error)?.message ?? err)
    }
  }

  /**
   * Load every content scope the node can offer, merged into one grid.
   *
   * `own` alone was all init ever asked for, which meant a films search showed
   * nothing but this node's own shared files: **IndeeHub and anything else
   * purchased live in `owned` (`content.owned-list`), and other nodes' catalogs
   * in `peers` — and neither scope had a single caller anywhere in the app.**
   * They existed only as type-signature options.
   *
   * Scopes are fetched concurrently and merged once, rather than each calling
   * `setArchyContent` itself: that sink REPLACES films/podcasts, so three
   * separate pushes would leave only whichever resolved last. Deduped by id
   * because the same title can legitimately appear in more than one scope
   * (owned locally and offered by a peer).
   *
   * A failing scope must not cost the others — a dead or slow peer is normal,
   * not exceptional — so each is caught individually and contributes nothing.
   */
  async function requestArchyAllContent() {
    if (!isInitialized.value) return

    // PROGRESSIVE, not all-at-once. The first version awaited all three scopes
    // together, so the grid waited on the slowest — and `peers` browses every
    // federated node over FIPS (falling back to Tor), which routinely takes
    // tens of seconds or times out entirely when a peer is offline. On-device
    // that showed up as "content(peers) failed: Content request timed out" and
    // an AIUI that felt very slow to open, with NOTHING rendered in the
    // meantime even though local content was ready immediately.
    //
    // So: paint `own` as soon as it lands, then fold in the slower sources as
    // they arrive. A scope that times out costs only its own results.
    const films: Film[] = []
    const podcasts: Podcast[] = []
    const images: ImageItem[] = []
    const seen = new Set<string>()

    const merge = (res: { films?: unknown; podcasts?: unknown; images?: unknown } | null) => {
      if (!res) return false
      let added = false
      for (const f of (res.films ?? []) as Film[]) {
        const k = `film:${f.id}`
        if (seen.has(k)) continue
        seen.add(k); films.push(f); added = true
      }
      for (const p of (res.podcasts ?? []) as Podcast[]) {
        const k = `pod:${p.id}`
        if (seen.has(k)) continue
        seen.add(k); podcasts.push(p); added = true
      }
      for (const im of (res.images ?? []) as ImageItem[]) {
        const k = `img:${im.id}`
        if (seen.has(k)) continue
        seen.add(k); images.push(im); added = true
      }
      return added
    }

    const paint = () => {
      const panel = useContentPanel()
      panel.setArchyContent({
        films: [...films],
        songs: panel.panelSongs.value,
        podcasts: [...podcasts],
        images: [...images],
      })
    }

    const fetchScope = (scope: 'own' | 'owned' | 'peers') =>
      archyBridge
        .requestArchyContent('all', scope)
        .then((res) => {
          if (!res.permitted) {
            console.warn(`[AIUI Archy] content(${scope}): not permitted — enable Media/File access in Archy Settings`)
            return null
          }
          return res
        })
        .catch((err) => {
          // A slow or absent peer is ordinary, not exceptional.
          console.warn(`[AIUI Archy] content(${scope}) failed:`, (err as Error)?.message ?? err)
          return null
        })

    // Local content first — it is fast and is what the operator sees instantly.
    if (merge(await fetchScope('own'))) paint()

    // Then the slower sources, each painting as it lands rather than blocking
    // the others or the initial render.
    await Promise.all(
      (['owned', 'peers'] as const).map((scope) =>
        fetchScope(scope).then((res) => {
          if (merge(res)) paint()
        }),
      ),
    )
  }


  /**
   * Request the node's real music library (13-11 — the D-13 wave) and hand
   * it to the same `setArchyContent` sink `requestArchyContent` uses, so
   * `SongGrid`'s `songs` bucket fills exactly the way the films bucket
   * already does. Sibling of `requestArchyContent`, same bridge call, same
   * permitted/not-permitted shape — the `'library'` kind is what routes
   * this node-side to `music.list-tracks` (real tag-extracted metadata)
   * instead of `content.*` (see `archyBridge.ts`'s `requestArchyContent`
   * doc comment). `films`/`podcasts` are never touched by a library
   * request — only `songs` is meaningful for `kind: 'library'`, so this
   * merges into whatever films/podcasts `setArchyContent` last held rather
   * than clobbering them with empty arrays.
   */
  async function requestArchyLibrary(scope: 'own' | 'peers' | 'owned' = 'own') {
    if (!isInitialized.value) return
    try {
      const res = await archyBridge.requestArchyContent('library', scope)
      if (!res.permitted) {
        console.warn('[AIUI Archy] library: not permitted — user should enable Media/File access in Archy Settings')
        return
      }
      const panel = useContentPanel()
      panel.setArchyContent({
        films: panel.panelFilms.value,
        songs: res.songs as Song[],
        podcasts: panel.panelPodcasts.value,
      })
    } catch (err) {
      console.warn('[AIUI Archy] library fetch failed:', (err as Error)?.message ?? err)
    }
  }

  /** Apply accent color as CSS custom property */
  function applyAccentColor(color: string) {
    document.documentElement.style.setProperty('--color-accent', color)
  }

  /** Build context string for AI system prompt */
  function buildArchyContext(): string {
    if (!isInitialized.value) return ''

    const sections: string[] = []

    if (permissions.value.includes('apps') && installedApps.value.length > 0) {
      const appList = installedApps.value
        .map((a) => `- ${a.name} (${a.state}${a.status ? ', ' + a.status : ''})`)
        .join('\n')
      sections.push(`**Installed apps on this node:**\n${appList}\nYou can view recent app logs by requesting the tail-logs action with an appId.`)
    }

    if (permissions.value.includes('system') && systemInfo.value.name) {
      const sys = systemInfo.value
      sections.push(`**System:** ${sys.name}${sys.version ? ' v' + sys.version : ''}`)
    }

    if (permissions.value.includes('network')) {
      const net = networkInfo.value
      sections.push(`**Network:** ${net.connected ? 'Connected' : 'Disconnected'}`)
    }

    if (permissions.value.includes('wallet') && walletInfo.value.available) {
      const w = walletInfo.value
      const parts: string[] = []
      if (w.alias) parts.push(w.alias)
      if (w.num_active_channels !== undefined) parts.push(`${w.num_active_channels} channels`)
      if (w.num_peers !== undefined) parts.push(`${w.num_peers} peers`)
      if (w.balance_sats !== undefined) parts.push(`On-chain: ${w.balance_sats.toLocaleString()} sats`)
      if (w.channel_balance_sats !== undefined) parts.push(`In channels: ${w.channel_balance_sats.toLocaleString()} sats`)
      if (w.synced_to_chain !== undefined) parts.push(w.synced_to_chain ? 'synced' : 'syncing')
      sections.push(`**Lightning (LND):** ${parts.join(' | ')}`)
    }

    if (permissions.value.includes('bitcoin') && bitcoinInfo.value.available) {
      const btc = bitcoinInfo.value
      const syncPct = btc.sync_progress ? (btc.sync_progress * 100).toFixed(2) + '%' : 'unknown'
      const parts = [`Block ${btc.block_height?.toLocaleString() ?? '?'}`, `${syncPct} synced`]
      if (btc.chain) parts.push(btc.chain)
      if (btc.mempool_tx_count) parts.push(`mempool: ${btc.mempool_tx_count.toLocaleString()} txs`)
      sections.push(`**Bitcoin:** ${parts.join(', ')}`)
    }

    if (permissions.value.includes('files') && fileList.value.length > 0) {
      const files = fileList.value
      const folders = files.filter(f => f.type === 'folder')
      const fileItems = files.filter(f => f.type === 'file')
      const images = fileItems.filter(f => /\.(jpg|jpeg|png|gif|webp|svg|heic|heif)$/i.test(f.name))
      const videos = fileItems.filter(f => /\.(mp4|mkv|avi|mov|webm)$/i.test(f.name))
      const music = fileItems.filter(f => /\.(mp3|flac|wav|ogg|m4a|aac|opus)$/i.test(f.name))
      const docs = fileItems.filter(f => /\.(pdf|doc|docx|txt|md|ods|xlsx|csv)$/i.test(f.name))

      const parts: string[] = [`${files.length} items`]
      if (folders.length > 0) parts.push(`${folders.length} folders (${folders.map(f => f.name).join(', ')})`)
      if (images.length > 0) parts.push(`${images.length} images`)
      if (videos.length > 0) parts.push(`${videos.length} videos`)
      if (music.length > 0) parts.push(`${music.length} audio files`)
      if (docs.length > 0) parts.push(`${docs.length} documents`)

      const recent = fileItems.slice(0, 15).map(f => f.name).join(', ')
      sections.push(`**Files:** ${parts.join(' | ')}\nRecent: ${recent}\nYou can read file contents by requesting the read-file action with a file path.`)
    }

    if (sections.length === 0) return ''

    return `\n\n**Archy Node Context** (this user is running AIUI on their Archipelago node):\n${sections.join('\n')}\n\nYou can help the user manage their node, check service status, browse files, and recommend apps. Available actions: open an app (open-app), install an app (install-app), tail app logs (tail-logs), read a file (read-file), navigate in Archy (navigate). When recommending apps, use [[app_ext:...]] tags and check if they're already installed. When discussing the user's files, mention specific files you can see. If the user asks about their photos, videos, or music, reference the file counts above.`
  }

  /** Clean up on component unmount */
  function destroy() {
    for (const cleanup of cleanups) cleanup()
    cleanups = []
    archyBridge.destroy()
    isInitialized.value = false
  }

  return {
    isEmbedded: readonly(isEmbedded),
    isInitialized: readonly(isInitialized),
    permissions: readonly(permissions),
    accentColor: readonly(accentColor),
    installedApps: readonly(installedApps),
    systemInfo: readonly(systemInfo),
    networkInfo: readonly(networkInfo),
    walletInfo: readonly(walletInfo),
    fileList: readonly(fileList),
    bitcoinInfo: readonly(bitcoinInfo),
    init,
    destroy,
    refreshContext,
    requestAction,
    readFile,
    tailLogs,
    requestArchyContent,
    requestArchyAllContent,
    requestArchyLibrary,
    buildArchyContext,
  }
}
