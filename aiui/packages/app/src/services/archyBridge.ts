/**
 * Archy Bridge — postMessage client for AIUI ↔ Archipelago communication.
 *
 * This is the ONLY way AIUI should communicate with the host Archy app.
 * Never make direct HTTP requests to the host machine.
 */

type AIContextCategory = 'apps' | 'system' | 'network' | 'wallet' | 'files' | 'bitcoin'

interface ContextResponse {
  data: unknown
  permitted: boolean
}

/** `kind`/`scope` for a `content:request` — see neode-ui's
 * `types/aiui-protocol.ts` (`AIUIContentRequest`/`ArchyContentPush`). Archy
 * decides the RPC method from `scope`; this bundle carries only data, never
 * a method name (T-13-34). Films/songs/podcasts are typed `unknown[]` here
 * deliberately — this transport module has no reason to know AIUI's content
 * shapes; `useArchy.ts`/`useContentPanel.ts` cast them to `Film[]`/`Song[]`/
 * `Podcast[]` at the one call site that does. */
interface ContentResponse {
  films: unknown[]
  songs: unknown[]
  podcasts: unknown[]
  images: unknown[]
  permitted: boolean
}

export interface ActionResponse {
  success: boolean
  error?: string
}

/** One content-tool result from a chat turn, carrying the same
 * `films`/`songs`/`podcasts` buckets `content:push` delivers — see
 * neode-ui's `ArchyChatSurface`. `scope` names what was asked for
 * (`own`/`peers`/`purchased`/`films`) so the surface can title itself. */
export interface ChatSurface {
  tool: string
  scope?: string
  bundle: {
    films: unknown[]
    songs: unknown[]
    podcasts: unknown[]
    images: unknown[]
  }
}

interface ThemeInfo {
  accent: string
  mode: 'dark'
}

type PermissionsCallback = (categories: AIContextCategory[]) => void
type ThemeCallback = (theme: ThemeInfo) => void

let requestId = 0
const pendingRequests = new Map<string, {
  resolve: (value: unknown) => void
  reject: (reason: unknown) => void
}>()

const permissionsCallbacks: PermissionsCallback[] = []
const themeCallbacks: ThemeCallback[] = []
let currentPermissions: AIContextCategory[] = []
let currentTheme: ThemeInfo | null = null
let initialized = false
let allowedOrigin: string | null = null

function generateId(): string {
  return `aiui-${++requestId}-${Date.now()}`
}

function postToParent(msg: unknown) {
  if (window.parent === window) return // Not in iframe
  if (!allowedOrigin) return // Origin not configured
  window.parent.postMessage(msg, allowedOrigin)
}

function handleMessage(event: MessageEvent) {
  // Always validate origin — reject if not configured or mismatched
  if (!allowedOrigin || event.origin !== allowedOrigin) return

  const msg = event.data
  if (!msg || typeof msg.type !== 'string') return

  switch (msg.type) {
    case 'context:response': {
      const pending = pendingRequests.get(msg.id)
      if (pending) {
        pendingRequests.delete(msg.id)
        pending.resolve({ data: msg.data, permitted: msg.permitted })
      }
      break
    }

    case 'action:response': {
      const pending = pendingRequests.get(msg.id)
      if (pending) {
        pendingRequests.delete(msg.id)
        pending.resolve({ success: msg.success, error: msg.error })
      }
      break
    }

    case 'chat:response': {
      const pending = pendingRequests.get(msg.id)
      if (pending) {
        pendingRequests.delete(msg.id)
        if (msg.success) {
          pending.resolve({
            text: msg.text ?? '',
            surfaces: Array.isArray(msg.surfaces) ? msg.surfaces : [],
          })
        } else {
          pending.reject(new Error(msg.error || 'Chat request failed'))
        }
      }
      break
    }

    case 'content:push': {
      const pending = pendingRequests.get(msg.id)
      if (pending) {
        pendingRequests.delete(msg.id)
        pending.resolve({
          films: Array.isArray(msg.films) ? msg.films : [],
          songs: Array.isArray(msg.songs) ? msg.songs : [],
          podcasts: Array.isArray(msg.podcasts) ? msg.podcasts : [],
          images: Array.isArray(msg.images) ? msg.images : [],
          permitted: msg.permitted !== false,
        })
      }
      break
    }

    case 'permissions:update': {
      currentPermissions = msg.categories || []
      for (const cb of permissionsCallbacks) cb(currentPermissions)
      break
    }

    case 'theme:response': {
      const theme: ThemeInfo | undefined = msg.theme
      if (theme) {
        currentTheme = theme
        for (const cb of themeCallbacks) cb(theme)
      }
      break
    }

    // Archy's ⌘K search offers "Talk to AIUI about it"; this carries the text
    // the operator typed there into our composer.
    case 'chat:prefill': {
      const text = typeof msg.text === 'string' ? msg.text : ''
      if (!text) break
      if (prefillCallback) prefillCallback(text)
      // Buffer when nothing is listening yet: the composer may not be mounted
      // at handshake time (collapsed chat, mobile content tab), and dropping
      // the text would silently lose what the operator typed.
      else bufferedPrefill = text
      break
    }
  }
}

/**
 * The parent (Archy) page's origin — NOT this iframe's own origin. This is
 * what `postToParent` must pass as postMessage's target origin, and what
 * `handleMessage` must validate incoming messages against. Getting this
 * backwards (using `window.location.origin`, this iframe's own origin) works
 * by coincidence only when Archy serves AIUI same-origin (production's
 * `/aiui/` proxy), and silently breaks the entire bridge — including the
 * initial 'ready' message — the moment AIUI runs on a different origin than
 * its host page (e.g. a local AIUI dev server embedded via a separate Archy
 * dev server port). `document.referrer` is the standard, cross-origin-safe
 * way an iframed document learns its embedding parent's URL.
 */
let prefillCallback: ((text: string) => void) | null = null
let bufferedPrefill = ''

function deriveParentOrigin(): string {
  if (typeof document !== 'undefined' && document.referrer) {
    try {
      return new URL(document.referrer).origin
    } catch {
      /* fall through to same-origin assumption below */
    }
  }
  return window.location.origin
}

export const archyBridge = {
  /**
   * Initialize the bridge. Call once on app mount.
   * Sends 'ready' to Archy so it knows the iframe is loaded.
   */
  init(origin?: string) {
    if (initialized) return
    initialized = true
    // Use an explicit origin if the caller has one, otherwise derive the
    // parent's actual origin (not this iframe's own).
    allowedOrigin = origin ?? deriveParentOrigin()
    window.addEventListener('message', handleMessage)
    postToParent({ type: 'ready' })
  },

  /**
   * Register the composer's prefill handler. Replays a prefill that arrived
   * before the composer mounted, so a ⌘K ask is never dropped on a cold frame.
   * Returns an unsubscribe so a remounting composer does not leak the old one.
   */
  onPrefill(fn: (text: string) => void) {
    prefillCallback = fn
    if (bufferedPrefill) {
      const text = bufferedPrefill
      bufferedPrefill = ''
      fn(text)
    }
    return () => {
      if (prefillCallback === fn) prefillCallback = null
    }
  },

  /** Clean up listeners */
  destroy() {
    window.removeEventListener('message', handleMessage)
    pendingRequests.clear()
    initialized = false
  },

  /** Check if running inside Archy iframe */
  isInArchy(): boolean {
    return window.parent !== window
  },

  /**
   * Request context data from Archy.
   * Returns { data, permitted }. If permitted is false, the user hasn't enabled this category.
   */
  requestContext(category: AIContextCategory, query?: string): Promise<ContextResponse> {
    const id = generateId()
    return new Promise((resolve, reject) => {
      pendingRequests.set(id, { resolve: resolve as (v: unknown) => void, reject })
      postToParent({
        type: 'context:request',
        id,
        category,
        query,
      })

      // Timeout after 10s
      setTimeout(() => {
        if (pendingRequests.has(id)) {
          pendingRequests.delete(id)
          reject(new Error(`Context request timed out: ${category}`))
        }
      }, 10000)
    })
  },

  /**
   * Request Archy to perform an action (install app, navigate, etc.)
   */
  requestAction(action: string, params: Record<string, string> = {}): Promise<ActionResponse> {
    const id = generateId()
    return new Promise((resolve, reject) => {
      pendingRequests.set(id, { resolve: resolve as (v: unknown) => void, reject })
      postToParent({
        type: 'action:request',
        id,
        action,
        params,
      })

      setTimeout(() => {
        if (pendingRequests.has(id)) {
          pendingRequests.delete(id)
          reject(new Error(`Action request timed out: ${action}`))
        }
      }, 30000)
    })
  },

  /**
   * Send a chat turn to Archy's node-side assistant loop (D-01: the model
   * key and the tool-calling loop live on the node, never in this bundle).
   * Resolves with the turn's final text AND any content the tools it ran
   * produced, already adapted to grid records by the host broker — this
   * is what lets an answer be rendered as a surface instead of only
   * described in prose. `surfaces` is `[]` for a turn that ran no content
   * tool, so callers never branch on undefined.
   */
  sendChat(text: string): Promise<{ text: string; surfaces: ChatSurface[] }> {
    const id = generateId()
    return new Promise((resolve, reject) => {
      pendingRequests.set(id, { resolve: resolve as (v: unknown) => void, reject })
      postToParent({
        type: 'chat:request',
        id,
        text,
      })

      // Must cover the node's WHOLE turn: multiple model round trips
      // (ASSISTANT_HTTP_TIMEOUT 180s each) plus the confirm gate's
      // human-speed wait (CONFIRM_TIMEOUT 300s). Kept just ABOVE the host
      // page's own assistant.chat RPC timeout (420s) so the host's error
      // path — which also expires the confirm dialog — fires first.
      setTimeout(() => {
        if (pendingRequests.has(id)) {
          pendingRequests.delete(id)
          reject(new Error('Chat request timed out'))
        }
      }, 430000)
    })
  },

  /**
   * Request a batch of grid-ready content from Archy (D-12/D-14, AIUI-03).
   * `kind` and `scope` are enums the broker interprets — this bridge never
   * lets AIUI name an RPC method or params. Resolves with `permitted:
   * false` and empty arrays if the media/files permission categories
   * aren't granted, rather than rejecting (mirrors `requestContext`'s
   * shape so `useArchy.ts` can check `.permitted` the same way).
   *
   * `'library'` (13-11) is `useArchy.ts`'s `requestArchyLibrary`'s kind —
   * resolves node-side to `music.list-tracks` instead of `content.*`
   * (`neode-ui`'s `aiui-protocol.ts`/`contextBroker.ts`), so the returned
   * `songs` carry real tag-extracted metadata rather than filename-derived
   * guesses.
   */
  requestArchyContent(
    kind: 'films' | 'songs' | 'podcasts' | 'all' | 'library',
    scope?: 'own' | 'peers' | 'owned',
  ): Promise<ContentResponse> {
    const id = generateId()
    return new Promise((resolve, reject) => {
      pendingRequests.set(id, { resolve: resolve as (v: unknown) => void, reject })
      postToParent({
        type: 'content:request',
        id,
        kind,
        scope,
      })

      // Peer/owned scopes can fan out across multiple mesh/Tor round trips
      // node-side — give this more room than the plain context:request's 10s.
      setTimeout(() => {
        if (pendingRequests.has(id)) {
          pendingRequests.delete(id)
          reject(new Error(`Content request timed out: ${kind}`))
        }
      }, 30000)
    })
  },

  /** Request Archy's theme info */
  requestTheme() {
    postToParent({ type: 'theme:request' })
  },

  /** Register callback for permission updates */
  onPermissionsUpdate(callback: PermissionsCallback): () => void {
    permissionsCallbacks.push(callback)
    // If we already have permissions, fire immediately
    if (currentPermissions.length > 0) callback(currentPermissions)
    return () => {
      const idx = permissionsCallbacks.indexOf(callback)
      if (idx !== -1) permissionsCallbacks.splice(idx, 1)
    }
  },

  /** Register callback for theme updates */
  onThemeUpdate(callback: ThemeCallback): () => void {
    themeCallbacks.push(callback)
    if (currentTheme) callback(currentTheme)
    return () => {
      const idx = themeCallbacks.indexOf(callback)
      if (idx !== -1) themeCallbacks.splice(idx, 1)
    }
  },

  /** Get current permitted categories */
  getPermissions(): AIContextCategory[] {
    return [...currentPermissions]
  },

  /** Get current theme */
  getTheme(): ThemeInfo | null {
    return currentTheme
  },
}
