import { ref, computed } from 'vue'
import { useChatStore } from '@/stores/chat'
import { searchWeb } from '@/composables/useWebSearch'
import { getApiKey } from '@/utils/key-vault'
import type { ImageAttachment } from '@aiui/core/types/message'
import { usePersonaStore } from '@/stores/personas'
import { useMemoryStore } from '@/stores/memory'
import { useArchy } from '@/composables/useArchy'
import { archyBridge, type ChatSurface } from '@/services/archyBridge'
import { useContentPanel } from '@/composables/useContentPanel'
import type { Film, Song, Podcast, ImageItem } from '@aiui/core/types/content'
import { useCodeContext } from '@/composables/useCodeContext'
import { apiFetch } from '@/utils/api-fetch'
import { useSettingsStore } from '@/stores/settings'

type Provider = 'routstr' | 'claude' | 'openrouter' | 'mock' | 'openai' | 'auto' | 'local'

// API paths are relative to the base URL so they work both in dev (/) and Archy (/aiui/)
const BASE = import.meta.env.BASE_URL || '/'
const CLAUDE_PATH = `${BASE}api/claude/v1/messages`
const OPENROUTER_PATH = `${BASE}api/openrouter`
const ROUTSTR_MODELS_PATH = `${BASE}api/routstr/models`
const ROUTSTR_CHAT_PATH = `${BASE}api/routstr/chat/completions`

import { mockFilms } from '@/mocks/films'
import { mockSongs } from '@/mocks/songs'
import { mockPodcasts } from '@/mocks/podcasts'

// Demo-site content pack (operator decision 2026-08-07): mock libraries are
// presented as "the user's library" ONLY in demo/dev builds. In production
// these constant-fold to empty strings, the prompt's library block vanishes,
// and the mock modules drop out of the bundle entirely.
const filmContext = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockFilms.map((f) =>
  `- [${f.id}] "${f.title}" (${f.year}) dir. ${f.director} | ${f.genres.join(', ')} | ${f.rating}/10 | On: ${f.sources.map(s => s.type).join(', ')}`
).join('\n') : ''

const songContext = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockSongs.map((s) =>
  `- [${s.id}] "${s.title}" by ${s.artist}${s.album ? ` (${s.album})` : ''}${s.year ? ` (${s.year})` : ''} | ${(s.genres ?? []).join(', ')} | On: ${(s.sources ?? []).map(x => x.type).join(', ')}`
).join('\n') : ''

const podcastContext = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockPodcasts.map((p) =>
  `- [${p.id}] "${p.title}" by ${p.host ?? 'Unknown'}${p.year ? ` (${p.year})` : ''} | ${(p.genres ?? []).join(', ')} | On: ${p.sources.map(x => x.type).join(', ')}`
).join('\n') : ''

const librarySection = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true')
  ? `\nThe user's film library:\n${filmContext}\n\nThe user's song library:\n${songContext}\n\nThe user's podcast library:\n${podcastContext}`
  : ''

// ─── Wavlake catalog context (fetched at runtime) ────────────
interface WavlakeCatalogTrack {
  title?: string
  artist?: string
  albumTitle?: string
  duration?: number
}

const wavlakeCatalog = ref<WavlakeCatalogTrack[]>([])
let wavlakeFetchedAt = 0
const WAVLAKE_REFRESH_INTERVAL = 30 * 60 * 1000 // 30 minutes

async function refreshWavlakeCatalog() {
  if (Date.now() - wavlakeFetchedAt < WAVLAKE_REFRESH_INTERVAL && wavlakeCatalog.value.length > 0) return
  try {
    const BASE = import.meta.env.BASE_URL || '/'
    const res = await apiFetch(`${BASE}api/music/rankings?days=30&limit=40`)
    if (!res.ok) return
    const data = await res.json()
    if (Array.isArray(data)) {
      wavlakeCatalog.value = data.map((t: Record<string, unknown>) => ({
        title: t.title as string,
        artist: t.artist as string,
        albumTitle: t.albumTitle as string | undefined,
        duration: t.duration as number | undefined,
      }))
      wavlakeFetchedAt = Date.now()
    }
  } catch {
    // Silently fail — catalog is optional context
  }
}

function buildWavlakeContext(): string {
  if (wavlakeCatalog.value.length === 0) return ''
  const lines = wavlakeCatalog.value.map((t) =>
    `- "${t.title}" by ${t.artist}${t.albumTitle ? ` (${t.albumTitle})` : ''}`
  )
  return `\n\n**Wavlake trending tracks** (these are confirmed playable — prefer recommending from this list when relevant):\n${lines.join('\n')}`
}

const SYSTEM_PROMPT = `You are AIUI, a helpful AI assistant with access to the user's media library (films, songs, and podcasts).

**News/Factual queries:** When the user asks for "news", "latest", "recent", or current information, lead with a direct answer summarizing the news/facts. You MAY add "For deeper coverage:" with [[podcast_ext:...]] tags only. Do NOT use [[song_ext:...]] or [[film_ext:...]] for news queries—podcasts are the appropriate follow-up. Never substitute an answer with only recommendations.

**Films:** When recommending or discussing films from the user's library, use [[film:ID]] where ID is the film's id. For films NOT in the library, use [[film_ext:Title|Year|Director]], e.g. [[film_ext:Brokeback Mountain|2005|Ang Lee]]. Write a brief reason why the film is worth watching on the same line as the tag.

**Songs:** When recommending or discussing songs, ALWAYS use tags for every song you mention:
- Library songs: [[song:ID]] where ID is the song's id (e.g. [[song:s1]]).
- Other songs: [[song_ext:Title|Artist|Year]] (year optional), e.g. [[song_ext:Never Meant|American Football|1999]].
Never list songs in plain text only—each recommendation must have a tag so the UI can show playable cards.

**Podcasts:** When recommending or discussing podcasts, use tags:
- Library podcasts: [[podcast:ID]] where ID is the podcast's id (e.g. [[podcast:p1]]).
- Other podcasts: [[podcast_ext:Title|Host|Year]] (year optional), e.g. [[podcast_ext:What Bitcoin Did|Peter McCormack|2018]].
Prioritize Podcasting 2.0–friendly platforms: Fountain.fm, Podcast Index, Castopod, Odysee, Rumble, YouTube, Podverse.

**Books:** When recommending or discussing books, use [[book_ext:Title|Author|Year]], e.g. [[book_ext:Neuromancer|William Gibson|1984]]. Write a brief reason why the book is worth reading on the same line.

**TV Series:** When recommending or discussing TV series/shows, use [[tv_ext:Title|Year|Creator]], e.g. [[tv_ext:Breaking Bad|2008|Vince Gilligan]]. Do NOT use [[film_ext:...]] for TV series — use [[tv_ext:...]] instead. Write a brief reason why the show is worth watching on the same line.

**Places/Restaurants:** When recommending restaurants, cafes, bars, or other places to visit, use [[place_ext:Name|Cuisine|City|Rating|PriceLevel|Address]], e.g. [[place_ext:Sushi Nakazawa|Japanese|New York|4.7|3|23 Commerce St]]. Rating is out of 5, PriceLevel is 1-4 ($ to $$$$). Omit fields you don't know. Write a brief description on the same line.

**Apps/Tools:** When recommending apps, clients, wallets, or tools, use [[app_ext:Name|Category|Platforms|URL]], e.g. [[app_ext:Damus|nostr-client|iOS|https://damus.io]]. Categories: nostr-client, lightning-wallet, bitcoin-wallet, privacy, node, dev-tool. Platforms: comma-separated list of ios,android,web,desktop,cli. Write a brief description on the same line.

**Images:** When sharing or describing images, use standard markdown image syntax: ![Description](https://image-url). Include a brief caption.

**Websites / "Best places to check":** When listing resources, places to check online, or websites for the user to visit, use markdown links: [Name](https://full-url). For simple domains use **Name** (domain.com), e.g. **Bitcoin Mailing List** (gnusha.org).

**Music discovery:** All music plays from **Wavlake** — a Lightning-powered, Nostr-native music platform. When recommending songs, prefer tracks from the Wavlake trending list (provided below) since those are confirmed playable. For genre requests, use [[song_ext:Title|Artist]] tags — the UI will search Wavlake automatically. Songs not on Wavlake won't play, so stick to Wavlake artists when you can. The user can zap (tip) artists with Lightning directly through the platform.

Always include these tags so the UI can render rich cards. Write a brief reason why each is worth checking out.
${librarySection}`

const activeProvider = ref<Provider>(archyBridge.isInArchy() ? 'auto' : 'claude')

const activeModel = ref('claude-haiku-4.5')

// Credentials require setup; network failures and provider outages require retry.
const needsApiKey = ref(false)
const needsFunding = ref(false)
function looksLikeMissingApiKey(err: string): boolean {
  return /\b(401|403)\b|api[ _-]?key|unauthorized|authentication_error|credential/i.test(err)
}

// ─── Routstr model catalog (fetched from the node's session-gated proxy) ───
// The node forwards the live Routstr aggregator's /v1/models; entries carry
// sats_pricing so completions are Cashu-paid against the operator's budget.
const routstrModels = ref<{ id: string; name: string }[]>([])
let routstrModelsFetched = false

async function refreshRoutstrModels() {
  if (routstrModelsFetched) return
  routstrModelsFetched = true
  try {
    const res = await apiFetch(ROUTSTR_MODELS_PATH)
    if (!res.ok) { routstrModelsFetched = false; return }
    const data = await res.json()
    if (Array.isArray(data?.data)) {
      routstrModels.value = data.data
        .filter((m: Record<string, unknown>) => typeof m.id === 'string')
        .map((m: Record<string, unknown>) => ({
          id: m.id as string,
          name: (m.name as string) || (m.id as string),
        }))
      if (activeProvider.value === 'routstr' && activeModel.value === 'routstr-unavailable' && routstrModels.value[0]) activeModel.value = routstrModels.value[0].id
    } else { routstrModelsFetched = false }
  } catch {
    routstrModelsFetched = false // allow a retry on the next send/open
  }
}

const availableProviders = computed(() => {
  if (archyBridge.isInArchy()) return [
    { id: 'local' as Provider, name: 'Local AI', models: [{ id: 'node', name: 'Node configuration' }] },
    { id: 'auto' as Provider, name: 'Node AI', models: [{ id: 'node', name: 'Node configuration' }] },
    { id: 'claude' as Provider, name: 'Claude API', models: [{ id: 'node', name: 'Node configuration' }] },
    { id: 'openai' as Provider, name: 'OpenAI API', models: [{ id: activeProvider.value === 'openai' ? activeModel.value : 'node', name: activeProvider.value === 'openai' ? activeModel.value : 'Configure model' }] },
    { id: 'routstr' as Provider, name: 'Routstr (sats)', models: routstrModels.value.length ? routstrModels.value : [{ id: 'routstr-unavailable', name: 'Models unavailable — retry' }] },
  ]
  const providers: { id: Provider; name: string; models: { id: string; name: string }[] }[] = [
    {
      id: 'routstr',
      name: 'Routstr (sats)',
      models: routstrModels.value.length > 0
        ? routstrModels.value
        : [{ id: 'routstr-unavailable', name: 'No models — node offline?' }],
    },
    {
      id: 'claude',
      name: 'Claude API',
      models: [
        { id: 'claude-haiku-4.5', name: 'Claude 4.5 Haiku' },
        { id: 'claude-sonnet-4', name: 'Claude Sonnet 4' },
        { id: 'claude-opus-4', name: 'Claude Opus 4' },
      ],
    },
  ]
  providers.push({
    id: 'openrouter',
    name: 'OpenRouter',
    models: [
      { id: 'meta-llama/llama-4-maverick', name: 'Llama 4 Maverick' },
      { id: 'qwen/qwen3-235b-a22b-thinking-2507', name: 'Qwen3 235B Thinking' },
      { id: 'mistralai/mistral-small-3.1-24b-instruct:free', name: 'Mistral Small 3.1 (free)' },
      { id: 'google/gemma-3-27b-it:free', name: 'Gemma 3 27B (free)' },
    ],
  })
  providers.push({
    id: 'mock',
    name: 'Demo echo',
    models: [{ id: 'echo', name: 'Echo (mirror input)' }],
  })
  return providers
})

function setProvider(provider: Provider) {
  if (archyBridge.isInArchy()) {
    if (provider === 'routstr' && activeProvider.value === 'routstr') return true
    archyBridge.requestAISetup(); return false
  }
  activeProvider.value = provider
  const p = availableProviders.value.find((pp) => pp.id === provider)
  if (p && p.models.length > 0) {
    activeModel.value = p.models[0].id
  }
  return true
}

function setModel(model: string) {
  if (archyBridge.isInArchy() && activeProvider.value !== 'routstr') { archyBridge.requestAISetup(); return }
  activeModel.value = model
}

archyBridge.onProviderConfigured(({ provider, model }) => {
  activeProvider.value = provider
  activeModel.value = model || (provider === 'routstr' ? routstrModels.value[0]?.id || 'routstr-unavailable' : 'node')
  if (provider === 'routstr') void refreshRoutstrModels()
})

interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  images?: ImageAttachment[]
}

/** Build Claude API content array for a message (multimodal when images present) */
function buildClaudeContent(msg: ChatMessage): string | Array<Record<string, unknown>> {
  const text = msg.content || '...'
  if (!msg.images || msg.images.length === 0) return text
  const blocks: Array<Record<string, unknown>> = []
  for (const img of msg.images) {
    blocks.push({
      type: 'image',
      source: { type: 'base64', media_type: img.mediaType, data: img.data },
    })
  }
  blocks.push({ type: 'text', text })
  return blocks
}

/**
 * Sanitize message history for the Claude API:
 * - Ensure every message has non-empty content
 * - Merge consecutive same-role messages to enforce strict alternation
 * - Guarantees the resulting array is valid for Claude's messages API
 */
function sanitizeHistory(messages: ChatMessage[]): ChatMessage[] {
  const result: ChatMessage[] = []
  for (const msg of messages) {
    const content = msg.content && msg.content.trim().length > 0 ? msg.content : '...'
    const sanitized: ChatMessage = { role: msg.role, content, images: msg.images }

    if (result.length > 0 && result[result.length - 1].role === sanitized.role) {
      // Merge into previous message of same role to maintain alternation
      const prev = result[result.length - 1]
      prev.content = prev.content + '\n' + sanitized.content
      if (sanitized.images && sanitized.images.length > 0) {
        prev.images = [...(prev.images ?? []), ...sanitized.images]
      }
    } else {
      result.push(sanitized)
    }
  }
  return result
}

async function streamMock(
  messages: ChatMessage[],
  onToken: (text: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  const lastUser = messages.filter((m) => m.role === 'user').pop()
  const text = lastUser
    ? `You said: "${lastUser.content}"\n\nThis is AIUI in echo mode. Select Claude or OpenRouter from the model picker.`
    : 'Hello! I am AIUI running in mock mode.'

  for (const char of text) {
    if (signal?.aborted) return
    onToken(char)
    await new Promise((r) => setTimeout(r, 12))
  }
}

interface GenerationParams {
  temperature?: number
  maxTokens?: number
  topP?: number
  stopSequences?: string[]
}

async function streamClaude(
  messages: ChatMessage[],
  onToken: (text: string) => void,
  onError: (err: string) => void,
  systemPrompt: string,
  webSearch: boolean,
  signal?: AbortSignal,
  params?: GenerationParams,
): Promise<void> {
  const headers: Record<string, string> = { 'Content-Type': 'application/json' }

  // Use the user's own key when enabled: memory-only store ref first, then
  // the encrypted vault. The key is never read from localStorage (S2).
  const settingsStore = useSettingsStore()
  if (settingsStore.settings.useOwnApiKey) {
    const ownKey = settingsStore.claudeApiKey || await getApiKey('claude')
    if (ownKey) {
      headers['x-api-key'] = ownKey
    }
  } else {
    const vaultKey = await getApiKey('claude')
    if (vaultKey) {
      headers['x-api-key'] = vaultKey
    }
  }

  // Build API messages — sanitize to ensure valid alternation and non-empty content
  const apiMessages = messages.map(m => ({
    role: m.role,
    content: buildClaudeContent(m),
  }))

  const body: Record<string, unknown> = {
    model: activeModel.value,
    system: systemPrompt,
    messages: apiMessages,
    stream: true,
    webSearch,
  }
  if (params?.temperature !== undefined) body.temperature = params.temperature
  if (params?.maxTokens !== undefined) body.max_tokens = params.maxTokens
  if (params?.topP !== undefined) body.top_p = params.topP
  if (params?.stopSequences && params.stopSequences.length > 0) body.stop_sequences = params.stopSequences

  const res = await apiFetch(CLAUDE_PATH, {
    method: 'POST',
    headers,
    body: JSON.stringify(body),
    signal,
  })

  if (!res.ok) {
    const body = await res.text().catch(() => 'Could not read error body')
    onError(`Claude proxy error ${res.status}: ${body}`)
    return
  }

  await readSSE(res, (data) => {
    try {
      const parsed = JSON.parse(data)
      if (parsed.type === 'content_block_delta' && parsed.delta?.text) {
        onToken(parsed.delta.text)
      } else if (parsed.type === 'error') {
        onError(parsed.error?.message ?? 'Claude stream error')
      }
    } catch { /* malformed SSE chunk */ }
  }, onError, signal)
}

async function streamOpenRouter(
  messages: ChatMessage[],
  onToken: (text: string) => void,
  onError: (err: string) => void,
  systemPrompt: string,
  signal?: AbortSignal,
): Promise<void> {
  const orMessages = [
    { role: 'system' as const, content: systemPrompt },
    ...messages.map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content })),
  ]

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'HTTP-Referer': window.location.origin,
    'X-Title': 'AIUI',
  }

  // Use vault key if available, otherwise proxy handles auth
  const vaultKey = await getApiKey('openrouter')
  if (vaultKey) {
    headers['Authorization'] = `Bearer ${vaultKey}`
  }

  const res = await apiFetch(OPENROUTER_PATH, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      model: activeModel.value,
      messages: orMessages,
      stream: true,
    }),
    signal,
  })

  if (!res.ok) {
    const body = await res.text().catch(() => 'Could not read error body')
    onError(`OpenRouter error ${res.status}: ${body}`)
    return
  }

  await readSSE(res, (data) => {
    if (data === '[DONE]') return
    try {
      const parsed = JSON.parse(data)
      const delta = parsed.choices?.[0]?.delta?.content
      if (delta) onToken(delta)
    } catch { /* malformed SSE chunk */ }
  }, onError, signal)
}

/**
 * Routstr: one paid, NON-streaming, OpenAI-shaped completion through the
 * node's session-gated `/aiui/api/routstr/` forwarder. The node quotes a
 * price from the live catalog, pays with a Cashu token against the
 * operator's budget (Settings → System → Routstr AI budget), redeems the
 * change, and passes the provider's JSON back. The full answer is emitted
 * as a single token — streaming across a paid hop is the planned follow-up.
 */
async function streamRoutstr(
  messages: ChatMessage[],
  onToken: (text: string) => void,
  onError: (err: string) => void,
  systemPrompt: string,
  signal?: AbortSignal,
): Promise<void> {
  const wireMessages = [
    { role: 'system' as const, content: systemPrompt },
    ...messages.map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content })),
  ]

  const res = await apiFetch(ROUTSTR_CHAT_PATH, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: activeModel.value,
      messages: wireMessages,
      stream: false,
    }),
    signal,
  })

  const bodyText = await res.text().catch(() => '')
  if (res.status === 402) needsFunding.value = true
  if (!res.ok) {
    // The node's refusals carry a plain-language error.message (budget not
    // set, budget spent, wallet can't fund) — surface it verbatim.
    let msg = `Routstr error ${res.status}`
    try {
      const parsed = JSON.parse(bodyText)
      // Node refusals use {error:{message}}; the upstream provider nests
      // its own as {detail:{error:{message}}} or a plain {detail:"..."}.
      const detail = parsed?.detail
      msg =
        parsed?.error?.message ??
        detail?.error?.message ??
        (typeof detail === 'string' ? detail : undefined) ??
        msg
    } catch { /* keep the status-only message */ }
    onError(msg)
    return
  }

  if (signal?.aborted) return
  try {
    const parsed = JSON.parse(bodyText)
    const text = parsed?.choices?.[0]?.message?.content
    if (typeof text === 'string' && text.length > 0) {
      onToken(text)
    } else {
      onError('Routstr returned an empty response')
    }
  } catch {
    onError('Routstr returned a malformed response')
  }
}

/**
 * Embedded-mode chat delegation (D-01/D-17): when AIUI is running inside
 * Archy, the model call, the tool-calling loop, and the model key all live
 * node-side. This sends only the latest user turn over the existing
 * origin-checked postMessage bridge (`archyBridge.sendChat`) and emits the
 * node's final answer as a single token — there is no streaming across the
 * bridge in this tracer, only a one-shot `chat:request`/`chat:response`
 * round trip.
 */
async function streamViaArchy(
  messages: ChatMessage[],
  onToken: (text: string) => void,
  onError: (err: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  const lastUser = [...messages].reverse().find((m) => m.role === 'user')
  const text = lastUser?.content ?? ''
  const { beginArchyContentLoad, setArchyContent } = useContentPanel()
  beginArchyContentLoad()
  try {
    const result = await archyBridge.sendChat(text)
    if (signal?.aborted) return
    // The turn's tool results, rendered — not just described. A node that
    // listed twelve shared files used to produce a correct paragraph next
    // to an empty grid, because this was the point the structured results
    // were dropped.
    setArchyContent(mergeChatSurfaces(result.surfaces))
    onToken(result.text)
  } catch (err) {
    if (signal?.aborted) return
    // Clear the 'Loading…' heading — an errored turn must not leave the
    // panel claiming it is still working.
    setArchyContent({})
    onError(err instanceof Error ? err.message : 'Archy chat request failed')
  }
}

/**
 * Flatten a turn's surfaces into the one bundle the panel renders. A
 * single turn can legitimately run `content_list` more than once (own
 * files AND peer films, say); concatenating rather than letting the last
 * call win is what keeps both visible.
 */
function mergeChatSurfaces(surfaces: ChatSurface[] = []) {
  return {
    films: surfaces.flatMap((s) => (s.bundle?.films ?? []) as Film[]),
    songs: surfaces.flatMap((s) => (s.bundle?.songs ?? []) as Song[]),
    podcasts: surfaces.flatMap((s) => (s.bundle?.podcasts ?? []) as Podcast[]),
    images: surfaces.flatMap((s) => (s.bundle?.images ?? []) as ImageItem[]),
  }
}

async function readSSE(
  res: Response,
  onData: (data: string) => void,
  onError: (err: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  const reader = res.body?.getReader()
  if (!reader) {
    onError('No response body')
    return
  }

  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      if (signal?.aborted) {
        reader.cancel()
        return
      }
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() ?? ''

      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed || !trimmed.startsWith('data: ')) continue
        const payload = trimmed.slice(6)
        if (payload === '[DONE]') return
        try {
          onData(payload)
        } catch {
          // skip malformed chunks
        }
      }
    }
  } catch (err) {
    if (signal?.aborted) return
    onError(err instanceof Error ? err.message : 'Stream read error')
  } finally {
    reader.cancel().catch(() => {})
  }
}

function formatWebSearchContext(results: { title: string; url: string; content?: string }[]): string {
  if (results.length === 0) return ''
  const lines = results.map((r, i) => {
    const snippet = r.content ? ` — ${r.content.slice(0, 200)}${r.content.length > 200 ? '…' : ''}` : ''
    return `${i + 1}. [${r.title}](${r.url})${snippet}`
  })
  return `\n\n**Web search results (PRIORITIZE these):**
- Answer the user's question using these results. Cite sources.
- You MAY add [[podcast_ext:...]] or [[film_ext:...]] tags for "to learn more" recommendations after your answer.\n\n${lines.join('\n')}`
}

/** Build the system prompt, incorporating persona and memory */
function buildSystemPrompt(chatStore: ReturnType<typeof useChatStore>): string {
  let prompt = SYSTEM_PROMPT

  // Prepend persona system prompt if active
  const personaId = chatStore.activeConversation?.personaId
  if (personaId) {
    const personaStore = usePersonaStore()
    const persona = personaStore.getPersona(personaId)
    if (persona?.systemPrompt) {
      prompt = persona.systemPrompt + '\n\n' + prompt
    }
  }

  // Append Wavlake catalog context
  prompt += buildWavlakeContext()

  // Append memory facts
  const memoryStore = useMemoryStore()
  prompt += memoryStore.buildMemoryContext()

  if (chatStore.webSearchEnabled) {
    prompt += `
**Web search:** You have access to WebSearch and WebFetch tools. Use them to look up current information, news, and facts when the user asks. You can search the web and fetch page content. Web search is enabled for this session—do not tell the user it is unavailable.`
  }

  // Append Archy node context when running embedded in Archipelago
  const archy = useArchy()
  prompt += archy.buildArchyContext()

  // Append code context when in code mode
  const code = useCodeContext()
  if (code.isCodeMode.value) {
    const sections: string[] = []
    if (code.activeProject.value) {
      sections.push(`**Active project:** ${code.activeProject.value.name} (${code.activeProject.value.language ?? 'Unknown'})`)
    }
    if (code.selectedDesignTokens.value.length > 0) {
      sections.push(`**Selected design tokens:** ${code.selectedDesignTokens.value.join(', ')}`)
    }
    if (code.selectedFiles.value.length > 0) {
      sections.push(`**Selected files:** ${code.selectedFiles.value.join(', ')}`)
    }
    if (code.activeFileContent.value && code.activeFile.value) {
      const content = code.activeFileContent.value.slice(0, 2000)
      sections.push(`**Open file (${code.activeFile.value}):**\n\`\`\`${code.activeFileLanguage.value}\n${content}\n\`\`\``)
    }
    if (sections.length > 0) {
      prompt += `\n\n**Code Context:**\n${sections.join('\n')}`
    }
  }

  return prompt
}

function getConversationParams(chatStore: ReturnType<typeof useChatStore>): GenerationParams {
  const conv = chatStore.activeConversation
  if (!conv) return {}
  return {
    temperature: conv.temperature,
    maxTokens: conv.maxTokens,
    topP: conv.topP,
    stopSequences: conv.stopSequences,
  }
}

let currentAbort: AbortController | null = null

/** Background title generation after first exchange */
async function generateAutoTitle(conversationId: string) {
  const chatStore = useChatStore()
  const conv = chatStore.conversations.get(conversationId)
  if (!conv) return

  // Only auto-title after first exchange (1 user + 1 assistant message)
  if (conv.messages.length !== 2) return
  const userMsg = conv.messages[0]
  if (userMsg.role !== 'user') return

  // Skip if title was manually set (not auto-generated from first message)
  const autoTitle = userMsg.content.slice(0, 60) + (userMsg.content.length > 60 ? '...' : '')
  if (conv.title !== autoTitle) return

  try {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' }
    const vaultKey = await getApiKey('claude')
    if (vaultKey) headers['x-api-key'] = vaultKey

    const res = await apiFetch(CLAUDE_PATH, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        model: 'claude-haiku-4.5',
        system: 'You generate very short conversation titles. Respond with ONLY a 3-5 word title, no quotes, no punctuation at the end.',
        messages: [{ role: 'user', content: `Title this conversation: "${userMsg.content.slice(0, 200)}"` }],
        max_tokens: 20,
        stream: false,
      }),
    })

    if (!res.ok) return

    const data = await res.json()
    const title = data?.content?.[0]?.text?.trim()
    if (title && title.length > 0 && title.length < 60) {
      conv.title = title
      conv.updatedAt = Date.now()
    }
  } catch {
    // Silent fail — title stays as default
  }
}

/** Stream a specific provider/model — used by comparison mode */
export async function streamWithModel(
  provider: string,
  model: string,
  messages: { role: string; content: string }[],
  onToken: (text: string) => void,
  onError: (err: string) => void,
  signal: AbortSignal,
): Promise<void> {
  const history = messages.map(m => ({ role: m.role as 'user' | 'assistant', content: m.content }))
  const savedModel = activeModel.value
  activeModel.value = model

  try {
    if (provider === 'routstr') {
      // Explicitly chosen Routstr wins even embedded in Archy — the whole
      // point of the picker entry is that it is a selection, not a fallback.
      await streamRoutstr(history, onToken, onError, 'You are a helpful assistant.', signal)
    } else if (useArchy().isEmbedded.value) {
      // D-17: embedded mode delegates the loop, the tools and the key to
      // Archy — provider/model selection here doesn't apply node-side.
      await streamViaArchy(history, onToken, onError, signal)
    } else if (provider === 'claude') {
      await streamClaude(history, onToken, onError, 'You are a helpful assistant.', false, signal)
    } else if (provider === 'openrouter') {
      await streamOpenRouter(history, onToken, onError, 'You are a helpful assistant.', signal)
    } else {
      await streamMock(history, onToken, signal)
    }
  } finally {
    activeModel.value = savedModel
  }
}

export function useAI() {
  const chatStore = useChatStore()

  // Fetch Wavlake + Routstr catalogs on first use (non-blocking)
  refreshWavlakeCatalog()
  refreshRoutstrModels()

  function stopGeneration() {
    if (currentAbort) {
      currentAbort.abort()
      currentAbort = null
    }
    chatStore.isStreaming = false
  }

  async function sendMessage(userText: string, images?: ImageAttachment[]) {
    // Refresh Wavlake catalog if stale (non-blocking, fire-and-forget)
    refreshWavlakeCatalog()

    const provider = activeProvider.value
    currentAbort = new AbortController()
    const signal = currentAbort.signal

    let convId = chatStore.activeConversationId
    if (!convId) {
      convId = chatStore.createConversation()
    }
    const cid = convId

    chatStore.addMessage(cid, {
      role: 'user',
      content: userText,
      images: images && images.length > 0 ? images : undefined,
    })
    const assistantMsg = chatStore.addMessage(cid, { role: 'assistant', content: '' })
    if (!assistantMsg) return

    chatStore.isStreaming = true

    let systemPrompt = buildSystemPrompt(chatStore)
    let clientSearchSucceeded = false
    // Embedded in Archy, the NODE owns the tool loop and `streamViaArchy`
    // sends only the user's text — this system prompt, and anything folded
    // into it, is never transmitted. So a client-side search here cost a
    // round trip and a CSP console error on every single turn while its
    // results provably reached no model. Web search for the embedded path
    // belongs node-side, next to the other tools.
    if (chatStore.webSearchEnabled && userText.trim() && !archyBridge.isInArchy()) {
      const results = await searchWeb(userText)
      if (results.length > 0) {
        systemPrompt += formatWebSearchContext(results)
        chatStore.setMessageWebResults(cid, assistantMsg.id, results)
        clientSearchSucceeded = true
        console.log('[AIUI] Injected', results.length, 'web search results into context')
      } else {
        console.warn('[AIUI] Web search enabled but 0 results — proxy will handle search')
      }
    }

    // If client-side search succeeded, don't ask the proxy to search again
    const proxyWebSearch = chatStore.webSearchEnabled && !clientSearchSucceeded

    const history: ChatMessage[] = sanitizeHistory(
      chatStore.messages
        .filter((m) => m.id !== assistantMsg.id)
        .map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content, images: m.images }))
    )

    const onToken = (text: string) => chatStore.appendToLastMessage(cid, text)
    const onError = (err: string) => {
      console.error(`[AIUI ${provider}]`, err)
      chatStore.appendToLastMessage(cid, `⚠ ${err}`)
      if (provider !== 'mock' && looksLikeMissingApiKey(err)) needsApiKey.value = true
    }

    const genParams = getConversationParams(chatStore)

    try {
      if (provider === 'routstr') {
        // Explicitly chosen Routstr wins even embedded in Archy — a
        // selection, not a fallback.
        await streamRoutstr(history, onToken, onError, systemPrompt, signal)
      } else if (useArchy().isEmbedded.value) {
        // D-17: embedded mode delegates the loop, the tools and the key to
        // Archy — provider/model selection here doesn't apply node-side.
        await streamViaArchy(history, onToken, onError, signal)
      } else if (provider === 'claude') {
        await streamClaude(history, onToken, onError, systemPrompt, proxyWebSearch, signal, genParams)
      } else if (provider === 'openrouter') {
        await streamOpenRouter(history, onToken, onError, systemPrompt, signal)
      } else {
        await streamMock(history, onToken, signal)
      }
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return
      const msg = err instanceof Error ? err.message : String(err)
      console.error(`[AIUI] Connection error:`, err)
      chatStore.appendToLastMessage(cid, `\n\n⚠ Connection error: ${msg}`)
      if (provider !== 'mock' && looksLikeMissingApiKey(msg)) needsApiKey.value = true
    } finally {
      currentAbort = null
      chatStore.isStreaming = false
    }

    // Auto-title: generate a short title after first exchange
    generateAutoTitle(cid)
  }

  /**
   * Edit a user message and regenerate the AI response.
   * Clears all messages after the edited message, then re-sends.
   */
  async function editAndResend(messageId: string, newContent: string) {
    const convId = chatStore.activeConversationId
    if (!convId) return

    const conv = chatStore.activeConversation
    if (!conv) return

    const msgIndex = conv.messages.findIndex((m) => m.id === messageId)
    if (msgIndex === -1) return

    // Update the message content
    chatStore.updateMessageContent(convId, messageId, newContent)

    // Delete all messages after this one
    chatStore.deleteMessagesAfter(convId, msgIndex + 1)

    // Re-send (creates new assistant message and streams)
    await resendLastUserMessage()
  }

  /**
   * Regenerate the last assistant response.
   * Deletes the last assistant message and re-sends with the same user message.
   */
  async function regenerateLastResponse() {
    const convId = chatStore.activeConversationId
    if (!convId) return

    const conv = chatStore.activeConversation
    if (!conv || conv.messages.length === 0) return

    // Find the last assistant message and remove it
    const lastIndex = conv.messages.length - 1
    if (conv.messages[lastIndex].role === 'assistant') {
      chatStore.deleteMessagesAfter(convId, lastIndex)
    }

    await resendLastUserMessage()
  }

  /** Internal: re-send from the current last user message */
  async function resendLastUserMessage() {
    const convId = chatStore.activeConversationId
    if (!convId) return

    const conv = chatStore.activeConversation
    if (!conv || conv.messages.length === 0) return

    const lastUserMsg = [...conv.messages].reverse().find((m) => m.role === 'user')
    if (!lastUserMsg) return

    const provider = activeProvider.value
    currentAbort = new AbortController()
    const signal = currentAbort.signal
    const cid = convId

    const assistantMsg = chatStore.addMessage(cid, { role: 'assistant', content: '' })
    if (!assistantMsg) return

    chatStore.isStreaming = true

    let systemPrompt = buildSystemPrompt(chatStore)
    if (chatStore.webSearchEnabled && lastUserMsg.content.trim() && !archyBridge.isInArchy()) {
      const results = await searchWeb(lastUserMsg.content)
      if (results.length > 0) {
        systemPrompt += formatWebSearchContext(results)
        chatStore.setMessageWebResults(cid, assistantMsg.id, results)
      }
    }

    const history: ChatMessage[] = sanitizeHistory(
      chatStore.messages
        .filter((m) => m.id !== assistantMsg.id)
        .map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content, images: m.images }))
    )

    const onToken = (text: string) => chatStore.appendToLastMessage(cid, text)
    const onError = (err: string) => {
      console.error(`[AIUI ${provider}]`, err)
      chatStore.appendToLastMessage(cid, `⚠ ${err}`)
      if (provider !== 'mock' && looksLikeMissingApiKey(err)) needsApiKey.value = true
    }

    const genParams = getConversationParams(chatStore)

    try {
      if (provider === 'routstr') {
        // Explicitly chosen Routstr wins even embedded in Archy — a
        // selection, not a fallback.
        await streamRoutstr(history, onToken, onError, systemPrompt, signal)
      } else if (useArchy().isEmbedded.value) {
        // D-17: embedded mode delegates the loop, the tools and the key to
        // Archy — provider/model selection here doesn't apply node-side.
        await streamViaArchy(history, onToken, onError, signal)
      } else if (provider === 'claude') {
        await streamClaude(history, onToken, onError, systemPrompt, chatStore.webSearchEnabled, signal, genParams)
      } else if (provider === 'openrouter') {
        await streamOpenRouter(history, onToken, onError, systemPrompt, signal)
      } else {
        await streamMock(history, onToken, signal)
      }
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return
      const msg = err instanceof Error ? err.message : String(err)
      chatStore.appendToLastMessage(cid, `\n\n⚠ Connection error: ${msg}`)
      if (provider !== 'mock' && looksLikeMissingApiKey(msg)) needsApiKey.value = true
    } finally {
      currentAbort = null
      chatStore.isStreaming = false
    }
  }

  return {
    sendMessage,
    stopGeneration,
    editAndResend,
    regenerateLastResponse,
    activeProvider,
    activeModel,
    availableProviders,
    setProvider,
    setModel,
    needsApiKey,
    needsFunding,
  }
}
