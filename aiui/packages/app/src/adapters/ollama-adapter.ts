import type { AIAdapter, ChatOptions } from './types'

const OLLAMA_BASE = 'http://localhost:11434'

export const ollamaAdapter: AIAdapter = {
  id: 'ollama',
  name: 'Ollama (Local)',
  supportsStreaming: true,
  supportsVision: false,
  supportsTools: false,

  models() {
    return [
      { id: 'llama3.2', name: 'Llama 3.2' },
      { id: 'mistral', name: 'Mistral' },
      { id: 'gemma2', name: 'Gemma 2' },
      { id: 'qwen2.5', name: 'Qwen 2.5' },
    ]
  },

  async chat(messages, options, onToken, onError) {
    const ollamaMessages = messages.map(m => ({
      role: m.role,
      content: m.content,
    }))

    if (options.systemPrompt) {
      ollamaMessages.unshift({ role: 'system', content: options.systemPrompt })
    }

    let res: Response
    try {
      res = await fetch(`${OLLAMA_BASE}/api/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: options.model,
          messages: ollamaMessages,
          stream: true,
        }),
        signal: options.signal,
      })
    } catch {
      onError('Cannot connect to Ollama. Is it running on localhost:11434?')
      return
    }

    if (!res.ok) {
      const body = await res.text().catch(() => 'Could not read error body')
      onError(`Ollama error ${res.status}: ${body}`)
      return
    }

    // Ollama uses newline-delimited JSON (not SSE)
    const reader = res.body?.getReader()
    if (!reader) {
      onError('No response body')
      return
    }

    const decoder = new TextDecoder()
    let buffer = ''

    try {
      while (true) {
        if (options.signal?.aborted) {
          reader.cancel()
          return
        }
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })
        const lines = buffer.split('\n')
        buffer = lines.pop() ?? ''

        for (const line of lines) {
          if (!line.trim()) continue
          try {
            const parsed = JSON.parse(line)
            if (parsed.message?.content) {
              onToken(parsed.message.content)
            }
            if (parsed.done) return
          } catch {
            // skip malformed lines
          }
        }
      }
    } finally {
      reader.cancel().catch(() => {})
    }
  },
}
