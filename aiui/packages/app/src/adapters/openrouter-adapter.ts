import type { AIAdapter, ChatOptions } from './types'
import { getApiKey } from '@/utils/key-vault'
import { apiFetch } from '@/utils/api-fetch'

const OPENROUTER_PATH = '/api/openrouter'

export const openrouterAdapter: AIAdapter = {
  id: 'openrouter',
  name: 'OpenRouter',
  supportsStreaming: true,
  supportsVision: false,
  supportsTools: false,

  models() {
    return [
      { id: 'meta-llama/llama-4-maverick', name: 'Llama 4 Maverick' },
      { id: 'qwen/qwen3-235b-a22b-thinking-2507', name: 'Qwen3 235B Thinking' },
      { id: 'mistralai/mistral-small-3.1-24b-instruct:free', name: 'Mistral Small 3.1 (free)' },
      { id: 'google/gemma-3-27b-it:free', name: 'Gemma 3 27B (free)' },
    ]
  },

  async chat(messages, options, onToken, onError) {
    const orMessages = messages.map(m => ({
      role: m.role as 'user' | 'assistant' | 'system',
      content: m.content,
    }))

    // Prepend system prompt as system message
    if (options.systemPrompt) {
      orMessages.unshift({ role: 'system', content: options.systemPrompt })
    }

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'HTTP-Referer': window.location.origin,
      'X-Title': 'AIUI',
    }

    const vaultKey = await getApiKey('openrouter')
    if (vaultKey) {
      headers['Authorization'] = `Bearer ${vaultKey}`
    }

    const res = await apiFetch(OPENROUTER_PATH, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        model: options.model,
        messages: orMessages,
        stream: true,
      }),
      signal: options.signal,
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
