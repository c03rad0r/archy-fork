/**
 * Unified AI adapter interface for normalizing different provider APIs.
 */

export interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
}

export interface ChatOptions {
  model: string
  systemPrompt?: string
  webSearch?: boolean
  signal?: AbortSignal
}

export interface AIAdapter {
  id: string
  name: string
  supportsStreaming: boolean
  supportsVision: boolean
  supportsTools: boolean

  /** List available models for this adapter */
  models(): { id: string; name: string }[]

  /** Stream chat completion, yielding text tokens */
  chat(
    messages: ChatMessage[],
    options: ChatOptions,
    onToken: (text: string) => void,
    onError: (err: string) => void,
  ): Promise<void>
}
