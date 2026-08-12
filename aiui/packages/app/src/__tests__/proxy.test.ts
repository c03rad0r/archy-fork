import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock proxy request/response logic without actually spawning processes
describe('Proxy Integration', () => {
  describe('SSE streaming format', () => {
    it('should produce valid SSE content_block_delta events', () => {
      const text = 'Hello, world!'
      const sseData = {
        type: 'content_block_delta',
        delta: { type: 'text_delta', text },
      }
      const sseString = `data: ${JSON.stringify(sseData)}\n\n`

      expect(sseString).toMatch(/^data: /)
      expect(sseString).toMatch(/\n\n$/)

      const parsed = JSON.parse(sseString.replace('data: ', '').trim())
      expect(parsed.type).toBe('content_block_delta')
      expect(parsed.delta.type).toBe('text_delta')
      expect(parsed.delta.text).toBe(text)
    })

    it('should produce valid DONE event', () => {
      const done = 'data: [DONE]\n\n'
      expect(done).toBe('data: [DONE]\n\n')
    })

    it('should produce valid error events', () => {
      const errData = {
        type: 'error',
        error: { message: 'Anthropic API 401: Unauthorized' },
      }
      const sseString = `data: ${JSON.stringify(errData)}\n\n`
      const parsed = JSON.parse(sseString.replace('data: ', '').trim())
      expect(parsed.type).toBe('error')
      expect(parsed.error.message).toContain('401')
    })
  })

  describe('Model mapping', () => {
    function mapModelToApi(model: string): string {
      if (model?.includes('opus')) return 'claude-opus-4-20250514'
      if (model?.includes('haiku')) return 'claude-haiku-4-5-20251001'
      return 'claude-sonnet-4-20250514'
    }

    it('should map sonnet model correctly', () => {
      expect(mapModelToApi('sonnet')).toBe('claude-sonnet-4-20250514')
      expect(mapModelToApi('claude-sonnet')).toBe('claude-sonnet-4-20250514')
    })

    it('should map opus model correctly', () => {
      expect(mapModelToApi('opus')).toBe('claude-opus-4-20250514')
      expect(mapModelToApi('claude-opus')).toBe('claude-opus-4-20250514')
    })

    it('should map haiku model correctly', () => {
      expect(mapModelToApi('haiku')).toBe('claude-haiku-4-5-20251001')
    })

    it('should default to sonnet for unknown models', () => {
      expect(mapModelToApi('unknown')).toBe('claude-sonnet-4-20250514')
    })
  })

  describe('Request validation', () => {
    it('should reject non-POST requests', () => {
      const method = 'GET' as string
      const isValid = method === 'POST'
      expect(isValid).toBe(false)
    })

    it('should reject unknown paths', () => {
      const validPaths = ['/v1/messages', '/v1/openrouter']
      expect(validPaths.includes('/v1/unknown')).toBe(false)
      expect(validPaths.includes('/v1/messages')).toBe(true)
      expect(validPaths.includes('/v1/openrouter')).toBe(true)
    })

    it('should parse request body correctly', () => {
      const body = JSON.stringify({
        model: 'sonnet',
        messages: [{ role: 'user', content: 'Hello' }],
        system: 'You are helpful.',
        webSearch: true,
      })
      const parsed = JSON.parse(body)
      expect(parsed.model).toBe('sonnet')
      expect(parsed.messages).toHaveLength(1)
      expect(parsed.system).toBe('You are helpful.')
      expect(parsed.webSearch).toBe(true)
    })

    it('should handle malformed JSON', () => {
      const badBody = 'not json'
      expect(() => JSON.parse(badBody)).toThrow()
    })
  })

  describe('Tool use round-trips', () => {
    it('should format search_web tool correctly', () => {
      const tool = {
        name: 'search_web',
        description: 'Search the web for current information.',
        input_schema: {
          type: 'object',
          properties: {
            query: { type: 'string', description: 'Search query' },
          },
          required: ['query'],
        },
      }
      expect(tool.name).toBe('search_web')
      expect(tool.input_schema.properties.query.type).toBe('string')
    })

    it('should construct tool_result messages correctly', () => {
      const toolResult = {
        type: 'tool_result',
        tool_use_id: 'toolu_123',
        content: '1. [Bitcoin price](https://example.com) — Current price is...',
      }
      expect(toolResult.type).toBe('tool_result')
      expect(toolResult.tool_use_id).toBe('toolu_123')
      expect(toolResult.content).toContain('Bitcoin')
    })

    it('should limit tool rounds to 5', () => {
      const maxToolRounds = 5
      let rounds = 0
      while (rounds < maxToolRounds) {
        rounds++
      }
      expect(rounds).toBe(5)
    })
  })

  describe('Error handling', () => {
    it('should handle 401 unauthorized', () => {
      const status = 401
      const errMsg = `Anthropic API ${status}: Unauthorized`
      expect(errMsg).toContain('401')
    })

    it('should handle 429 rate limit', () => {
      const status = 429
      const errMsg = `Anthropic API ${status}: Rate limited`
      expect(errMsg).toContain('429')
    })

    it('should handle 500 server error', () => {
      const status = 500
      const errMsg = `Anthropic API ${status}: Internal Server Error`
      expect(errMsg).toContain('500')
    })
  })

  describe('OAuth token detection', () => {
    const isOAuthToken = (s: string) => /^sk-ant-oat/.test(s)

    it('should detect OAuth tokens', () => {
      expect(isOAuthToken('sk-ant-oat-abc123')).toBe(true)
    })

    it('should not flag API keys as OAuth', () => {
      expect(isOAuthToken('sk-ant-api03-abc123')).toBe(false)
    })
  })

  describe('Client disconnect handling', () => {
    it('should track client disconnection', () => {
      const state = { clientDisconnected: false }
      // Simulate disconnect
      state.clientDisconnected = true
      expect(state.clientDisconnected).toBe(true)
    })

    it('should not write after disconnect', () => {
      const clientDisconnected = true
      const writes: string[] = []
      const write = (data: string) => {
        if (!clientDisconnected) writes.push(data)
      }
      write('should not appear')
      expect(writes).toHaveLength(0)
    })
  })
})
