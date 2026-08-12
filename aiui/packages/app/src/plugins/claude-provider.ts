import type {
  AIProviderAdapter,
  AIModel,
  ChatMessage,
  ChatOptions,
  ChatChunk,
  PluginContext,
} from '@aiui/core'

const BASE = import.meta.env.BASE_URL || '/'
const CLAUDE_PATH = `${BASE}api/claude/v1/messages`

async function* streamChat(
  messages: ChatMessage[],
  options: ChatOptions,
  systemPrompt?: string,
): AsyncGenerator<ChatChunk> {
  const body: Record<string, unknown> = {
    model: options.model,
    messages: messages.filter(m => m.role !== 'system').map(m => ({
      role: m.role,
      content: typeof m.content === 'string' ? m.content : m.content.map(p => p.text ?? '').join(''),
    })),
    stream: options.stream ?? true,
  }

  // Extract system message from messages or use provided systemPrompt
  const systemMsg = messages.find(m => m.role === 'system')
  if (systemPrompt) {
    body.system = systemPrompt
  } else if (systemMsg) {
    body.system = typeof systemMsg.content === 'string'
      ? systemMsg.content
      : systemMsg.content.map(p => p.text ?? '').join('')
  }

  if (options.maxTokens) body.max_tokens = options.maxTokens
  if (options.temperature !== undefined) body.temperature = options.temperature

  const res = await fetch(CLAUDE_PATH, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })

  if (!res.ok) {
    const errBody = await res.text().catch(() => 'Could not read error body')
    yield { type: 'error', error: `Claude proxy error ${res.status}: ${errBody}` }
    return
  }

  const reader = res.body?.getReader()
  if (!reader) {
    yield { type: 'error', error: 'No response body' }
    return
  }

  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() ?? ''

      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed || !trimmed.startsWith('data: ')) continue
        const payload = trimmed.slice(6)
        if (payload === '[DONE]') {
          yield { type: 'done' }
          return
        }
        try {
          const parsed = JSON.parse(payload)
          if (parsed.type === 'content_block_delta' && parsed.delta?.text) {
            yield { type: 'text', text: parsed.delta.text }
          } else if (parsed.type === 'message_stop') {
            yield { type: 'done', usage: parsed.usage ? {
              promptTokens: parsed.usage.input_tokens ?? 0,
              completionTokens: parsed.usage.output_tokens ?? 0,
            } : undefined }
          } else if (parsed.type === 'error') {
            yield { type: 'error', error: parsed.error?.message ?? 'Claude stream error' }
          }
        } catch {
          // skip malformed chunks
        }
      }
    }
  } finally {
    reader.cancel().catch(() => {})
  }

  yield { type: 'done' }
}

export const claudeProvider: AIProviderAdapter = {
  id: 'claude',
  name: 'Claude (Anthropic)',
  version: '1.0.0',
  type: 'ai-provider',
  description: 'Anthropic Claude AI via proxy server',

  supportsStreaming: true,
  supportsVision: true,
  supportsTools: true,

  async init(_context: PluginContext): Promise<void> {
    // No initialization needed — uses proxy
  },

  async destroy(): Promise<void> {
    // No cleanup needed
  },

  async isAvailable(): Promise<boolean> {
    try {
      const res = await fetch(CLAUDE_PATH, { method: 'OPTIONS' })
      return res.ok || res.status === 405 // OPTIONS may not be supported but proxy is up
    } catch {
      return false
    }
  },

  chat(messages: ChatMessage[], options: ChatOptions): AsyncIterable<ChatChunk> {
    return streamChat(messages, options)
  },

  async models(): Promise<AIModel[]> {
    return [
      {
        id: 'claude-haiku-4.5',
        name: 'Claude 4.5 Haiku',
        provider: 'claude',
        supportsVision: true,
        supportsTools: true,
        contextWindow: 200000,
      },
      {
        id: 'claude-sonnet-4',
        name: 'Claude Sonnet 4',
        provider: 'claude',
        supportsVision: true,
        supportsTools: true,
        contextWindow: 200000,
      },
      {
        id: 'claude-opus-4',
        name: 'Claude Opus 4',
        provider: 'claude',
        supportsVision: true,
        supportsTools: true,
        contextWindow: 200000,
      },
    ]
  },
}
