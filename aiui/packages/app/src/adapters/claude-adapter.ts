import type { AIAdapter, ChatMessage, ChatOptions } from './types'
import { getApiKey } from '@/utils/key-vault'
import { apiFetch } from '@/utils/api-fetch'

const BASE = import.meta.env.BASE_URL || '/'
const CLAUDE_PATH = `${BASE}api/claude/v1/messages`

export const claudeAdapter: AIAdapter = {
  id: 'claude',
  name: 'Claude (Max)',
  supportsStreaming: true,
  supportsVision: true,
  supportsTools: true,

  models() {
    return [
      { id: 'claude-haiku-4.5', name: 'Claude 4.5 Haiku' },
      { id: 'claude-sonnet-4', name: 'Claude Sonnet 4' },
      { id: 'claude-opus-4', name: 'Claude Opus 4' },
    ]
  },

  async chat(messages, options, onToken, onError) {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' }

    const vaultKey = await getApiKey('claude')
    if (vaultKey) {
      headers['x-api-key'] = vaultKey
    }

    const apiMessages = messages
      .filter(m => m.role !== 'system')
      .map(m => ({ role: m.role, content: m.content }))

    const res = await apiFetch(CLAUDE_PATH, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        model: options.model,
        system: options.systemPrompt,
        messages: apiMessages,
        stream: true,
        webSearch: options.webSearch ?? false,
      }),
      signal: options.signal,
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
    }, onError, options.signal)
  },
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
