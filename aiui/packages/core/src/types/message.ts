import type { ContentBlock } from './content'

export interface WebSearchResult {
  title: string
  url: string
  content?: string
  /** Image/thumbnail URL from search engine (e.g. SearXNG img_src) */
  imgSrc?: string
}

export interface ImageAttachment {
  /** Base64-encoded image data (no data: prefix) */
  data: string
  /** MIME type e.g. image/jpeg, image/png, image/gif, image/webp */
  mediaType: string
}

export interface Message {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  contentBlocks?: ContentBlock[]
  timestamp: number
  model?: string
  usage?: { promptTokens: number; completionTokens: number }
  replyTo?: string
  reactions?: Reaction[]
  status?: 'sending' | 'sent' | 'delivered' | 'read' | 'error'
  /** Web search results (articles, links) attached when query used web search */
  webResults?: WebSearchResult[]
  /** Timestamp of last edit (set when user edits a sent message) */
  editedAt?: number
  /** Attached images (base64, max 4) for vision-capable models */
  images?: ImageAttachment[]
  /** User feedback: 'up' = thumbs up, 'down' = thumbs down */
  feedback?: 'up' | 'down'
}

export interface Reaction {
  emoji: string
  userId: string
  timestamp: number
}

export interface Conversation {
  id: string
  title: string
  messages: Message[]
  createdAt: number
  updatedAt: number
  model?: string
  systemPrompt?: string
  /** ID of parent conversation this was branched from */
  parentConversationId?: string
  /** Message ID in parent where this branch forked */
  branchPoint?: string
  /** IDs of child branches forked from this conversation */
  childBranchIds?: string[]
  /** ID of the persona applied to this conversation */
  personaId?: string
  /** Generation params (persisted per conversation) */
  temperature?: number
  maxTokens?: number
  topP?: number
  /** Comma-separated stop sequences */
  stopSequences?: string[]
}
