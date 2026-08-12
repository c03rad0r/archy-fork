export type PluginType =
  | 'ai-provider'
  | 'media-source'
  | 'messaging'
  | 'storage'
  | 'renderer'
  | 'file-handler'
  | 'crypto'
  | 'search'
  | 'auth'
  | 'wallet'
  | 'social-embed'
  | 'mcp'
  | 'media'

export interface PluginContext {
  settings: PluginSettingsStore
  events: PluginEventBus
  logger: PluginLogger
}

export interface PluginSettingsStore {
  get<T>(key: string): T | undefined
  set<T>(key: string, value: T): void
}

export interface PluginEventBus {
  emit(event: string, payload?: unknown): void
  on(event: string, handler: (payload?: unknown) => void): () => void
}

export interface PluginLogger {
  info(message: string, ...args: unknown[]): void
  warn(message: string, ...args: unknown[]): void
  error(message: string, ...args: unknown[]): void
}

export interface AIUIPlugin {
  id: string
  name: string
  version: string
  type: PluginType
  description?: string
  icon?: string
  init(context: PluginContext): Promise<void>
  destroy(): Promise<void>
  isAvailable(): Promise<boolean>
}

export interface AIProviderAdapter extends AIUIPlugin {
  type: 'ai-provider'
  chat(messages: ChatMessage[], options: ChatOptions): AsyncIterable<ChatChunk>
  models(): Promise<AIModel[]>
  supportsStreaming: boolean
  supportsVision: boolean
  supportsTools: boolean
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content: string | ContentPart[]
  toolCalls?: ToolCall[]
  toolCallId?: string
}

export interface ContentPart {
  type: 'text' | 'image_url'
  text?: string
  imageUrl?: string
}

export interface ChatOptions {
  model: string
  temperature?: number
  maxTokens?: number
  tools?: ToolDefinition[]
  stream?: boolean
}

export interface ChatChunk {
  type: 'text' | 'tool_call' | 'done' | 'error'
  text?: string
  toolCall?: ToolCall
  error?: string
  usage?: { promptTokens: number; completionTokens: number }
}

export interface ToolCall {
  id: string
  name: string
  arguments: Record<string, unknown>
}

export interface ToolDefinition {
  name: string
  description: string
  parameters: Record<string, unknown>
}

export interface ToolResult {
  toolCallId: string
  content: string
  isError: boolean
}

export interface AIModel {
  id: string
  name: string
  provider: string
  supportsVision: boolean
  supportsTools: boolean
  contextWindow: number
}

export interface MediaSourcePlugin extends AIUIPlugin {
  type: 'media-source'
  search(query: string): Promise<MediaItem[]>
  getLibrary(filters?: Record<string, unknown>): Promise<MediaItem[]>
  getPlayUrl(itemId: string): Promise<string>
}

export interface MediaItem {
  id: string
  title: string
  type: 'film' | 'tv' | 'music' | 'podcast' | 'audiobook'
  posterUrl?: string
  year?: number
  rating?: number
  source: string
  sourceIcon?: string
}

export interface WalletPlugin extends AIUIPlugin {
  type: 'wallet'
  supports: PaymentMethod[]
  isInstalled(): Promise<boolean>
  getPayUri(request: PaymentRequest): string
  openWallet(request: PaymentRequest): Promise<void>
}

export type PaymentMethod = 'lightning' | 'onchain' | 'cashu' | 'fedimint'

export interface PaymentRequest {
  type: PaymentMethod
  invoice?: string
  address?: string
  amount?: number
  memo?: string
  lnurl?: string
  cashuToken?: string
  mintUrl?: string
}

export interface SocialEmbedPlugin extends AIUIPlugin {
  type: 'social-embed'
  platform: 'x' | 'nostr' | 'mastodon' | 'bluesky'
  fetchPost(url: string): Promise<SocialPost>
  fetchThread(url: string): Promise<SocialPost[]>
}

export interface SocialPost {
  id: string
  author: { name: string; handle: string; avatarUrl: string }
  content: string
  media?: { type: 'image' | 'video'; url: string }[]
  metrics?: { likes: number; reposts: number; replies: number }
  timestamp: string
  url: string
}
