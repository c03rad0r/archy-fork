/**
 * AIUI ↔ Archy postMessage Protocol
 *
 * AIUI (iframe) communicates with Archy (host) via structured messages.
 * Archy acts as a context broker — AIUI never directly accesses node data.
 */

import type { ArchyContentBundle } from '@/composables/archyContentAdapter'

/** Data categories that AIUI can request access to */
export type AIContextCategory =
  | 'apps'
  | 'system'
  | 'network'
  | 'wallet'
  | 'files'
  | 'media'
  | 'search'
  | 'ai-local'
  | 'notes'
  | 'bitcoin'

/** Actions AIUI can request Archy to perform */
export type AIActionType = 'prepare-website' | 'install-app' | 'open-app' | 'navigate' | 'launch-app' | 'search-web' | 'read-file' | 'tail-logs'

// ─── AIUI → Archy (Requests) ───────────────────────────────────────────────

export interface AIUIContextRequest {
  type: 'context:request'
  id: string
  category: AIContextCategory
  query?: string
}

export interface AIUIActionRequest {
  type: 'action:request'
  id: string
  action: AIActionType
  params: Record<string, string>
}

export interface AIUIReadyMessage {
  type: 'ready'
}

export interface AIUIThemeRequest {
  type: 'theme:request'
}

/**
 * A chat turn from AIUI's embedded-mode client. Carries only the raw user
 * text — tool selection is node-side (D-01/D-03) and must never be
 * expressible as an AIUI-originated action, so this is deliberately NOT an
 * `AIActionType` member.
 */
export interface AIUIChatRequest {
  type: 'chat:request'
  id: string
  text: string
}

/**
 * A content-grid request from AIUI's embedded-mode client. Carries only a
 * `kind` discriminator and an optional `scope` — the iframe never names an
 * RPC method or params (T-13-34); the broker decides the call. A single
 * generic channel (not one per content type) so 13-11's music-library wave
 * can extend `kind` without touching this file again.
 *
 * `'library'` (13-11) is the one addition this wave makes to the
 * discriminator: it routes to `music.list-tracks` (13-07) instead of
 * `content.*`, since a library track carries real tag-extracted metadata
 * (artist/album/duration) that `ContentItem` has no field for at all.
 */
export interface AIUIContentRequest {
  type: 'content:request'
  id: string
  kind: 'films' | 'songs' | 'podcasts' | 'all' | 'library'
  scope?: 'own' | 'peers' | 'owned'
}

export type AIUIRequest =
  | AIUIContextRequest
  | AIUIActionRequest
  | AIUIReadyMessage
  | AIUIThemeRequest
  | AIUIChatRequest
  | AIUIContentRequest

// ─── Archy → AIUI (Responses) ──────────────────────────────────────────────

export interface ArchyContextResponse {
  type: 'context:response'
  id: string
  data: unknown
  permitted: boolean
}

export interface ArchyActionResponse {
  type: 'action:response'
  id: string
  success: boolean
  error?: string
  data?: unknown
}

export interface ArchyThemeResponse {
  type: 'theme:response'
  theme: {
    accent: string
    mode: 'dark'
  }
}

export interface ArchyPermissionsUpdate {
  type: 'permissions:update'
  categories: AIContextCategory[]
}

/** The node's answer to a `chat:request`. On RPC failure, `error` carries
 * only the error message — never the raw exception object. */
export interface ArchyChatResponse {
  type: 'chat:response'
  id: string
  success: boolean
  text?: string
  error?: string
  /** Content-producing tool results from this turn, already adapted into
   * the same grid records `content:push` delivers, so AIUI can RENDER
   * what the answer describes instead of leaving its surface empty
   * beside a correct paragraph. Absent when the turn ran no such tool. */
  surfaces?: ArchyChatSurface[]
}

/** One content tool result from a chat turn. `scope` is the tool's own
 * argument (`own` | `peers` | `purchased` | `films`) — it is what lets the
 * surface title itself with what was actually asked for rather than
 * inferring it from the payload's shape. */
export interface ArchyChatSurface {
  tool: string
  scope?: string
  bundle: ArchyContentBundle
}

/**
 * The node's answer to a `content:request` — adapted grid records
 * (`archyContentAdapter.ts`'s `adaptContentItems` output) for whichever
 * buckets the RPC scope produced. `permitted: false` means neither the
 * `media` nor the `files` category is granted (content:request is treated
 * as permitted if either is enabled — see `handleContentRequest`); the
 * three arrays are empty in that case, never omitted, so AIUI can render an
 * empty-state instead of hanging on an unresolved request.
 */
export type ArchyContentPush = {
  type: 'content:push'
  id: string
  kind: string
  permitted: boolean
} & Partial<ArchyContentBundle>

export type ArchyResponse =
  | ArchyContextResponse
  | ArchyActionResponse
  | ArchyThemeResponse
  | ArchyPermissionsUpdate
  | ArchyChatResponse
  | ArchyContentPush

// ─── All messages ───────────────────────────────────────────────────────────

export type AIUIMessage = AIUIRequest | ArchyResponse

/** Protocol version for compatibility checks */
export const AIUI_PROTOCOL_VERSION = '1.0.0'

/** Message origin prefix used for validation */
export const AIUI_MESSAGE_PREFIX = 'aiui:'
