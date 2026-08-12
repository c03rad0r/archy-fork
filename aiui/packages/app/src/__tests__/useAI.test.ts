import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

// Mock web search before importing useAI
vi.mock('@/composables/useWebSearch', () => ({
  searchWeb: vi.fn().mockResolvedValue([]),
}))

// Mock mock data to avoid importing large fixture files
vi.mock('@/mocks/films', () => ({
  mockFilms: [
    { id: 'f1', title: 'Test Film', year: 2020, director: 'Test Dir', genres: ['Drama'], rating: 8, sources: [{ type: 'stream' }] },
  ],
}))
vi.mock('@/mocks/songs', () => ({
  mockSongs: [
    { id: 's1', title: 'Test Song', artist: 'Test Artist', album: 'Test Album', year: 2020, genres: ['Rock'], sources: [{ type: 'jamendo' }] },
  ],
}))
vi.mock('@/mocks/podcasts', () => ({
  mockPodcasts: [
    { id: 'p1', title: 'Test Podcast', host: 'Test Host', year: 2020, genres: ['Tech'], sources: [{ type: 'rss' }] },
  ],
}))

// Mock idb-storage to avoid IndexedDB access in tests
vi.mock('@/utils/idb-storage', () => ({
  saveConversation: vi.fn().mockResolvedValue(undefined),
  loadAllConversations: vi.fn().mockResolvedValue(new Map()),
  deleteConversation: vi.fn().mockResolvedValue(undefined),
  isIDBAvailable: vi.fn().mockReturnValue(false),
}))

import { useAI } from '@/composables/useAI'
import { useChatStore } from '@/stores/chat'
import { searchWeb } from '@/composables/useWebSearch'

const originalFetch = globalThis.fetch

// Helper to create a mock SSE readable stream
function createSSEStream(events: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder()
  let index = 0
  return new ReadableStream({
    pull(controller) {
      if (index < events.length) {
        controller.enqueue(encoder.encode(events[index]))
        index++
      } else {
        controller.close()
      }
    },
  })
}

function mockClaudeResponse(events: string[]) {
  return {
    ok: true,
    body: createSSEStream(events),
    text: () => Promise.resolve(''),
  }
}

describe('useAI', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    // Reset provider state (module-level ref) back to claude
    const { setProvider } = useAI()
    setProvider('claude')
    // Re-mock searchWeb
    vi.mocked(searchWeb).mockResolvedValue([])
    // Default fetch mock (catches loadServerChats and any stray calls)
    globalThis.fetch = originalFetch
  })

  describe('provider selection', () => {
    it('defaults to claude provider', () => {
      const { activeProvider } = useAI()
      expect(activeProvider.value).toBe('claude')
    })

    it('switches provider via setProvider', () => {
      const { setProvider, activeProvider, activeModel } = useAI()
      setProvider('openrouter')
      expect(activeProvider.value).toBe('openrouter')
      expect(activeModel.value).toBe('meta-llama/llama-4-maverick')
    })

    it('switches to mock provider', () => {
      const { setProvider, activeProvider, activeModel } = useAI()
      setProvider('mock')
      expect(activeProvider.value).toBe('mock')
      expect(activeModel.value).toBe('echo')
    })

    it('lists available providers with models', () => {
      const { availableProviders } = useAI()
      expect(availableProviders.value.length).toBe(3)
      const ids = availableProviders.value.map(p => p.id)
      expect(ids).toContain('claude')
      expect(ids).toContain('openrouter')
      expect(ids).toContain('mock')
    })

    it('sets model directly via setModel', () => {
      const { setModel, activeModel } = useAI()
      setModel('claude-sonnet-4')
      expect(activeModel.value).toBe('claude-sonnet-4')
    })
  })

  describe('context injection', () => {
    it('includes film library in system prompt', async () => {
      const fetchSpy = vi.fn().mockResolvedValue(
        mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"Hi"}}\n\n'])
      )
      globalThis.fetch = fetchSpy

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage } = useAI()

      await sendMessage('hello')

      const claudeCall = fetchSpy.mock.calls.find(
        (c: unknown[]) => (c[0] as string)?.toString().includes('/claude/')
      )
      expect(claudeCall).toBeDefined()
      const body = JSON.parse(claudeCall![1].body as string)
      expect(body.system).toContain('Test Film')
      expect(body.system).toContain("user's film library")
    })

    it('includes song library in system prompt', async () => {
      const fetchSpy = vi.fn().mockResolvedValue(
        mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"Hi"}}\n\n'])
      )
      globalThis.fetch = fetchSpy

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage } = useAI()

      await sendMessage('hello')

      const claudeCall = fetchSpy.mock.calls.find(
        (c: unknown[]) => (c[0] as string)?.toString().includes('/claude/')
      )
      const body = JSON.parse(claudeCall![1].body as string)
      expect(body.system).toContain('Test Song')
      expect(body.system).toContain("user's song library")
    })

    it('includes content tag format instructions', async () => {
      const fetchSpy = vi.fn().mockResolvedValue(
        mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"Hi"}}\n\n'])
      )
      globalThis.fetch = fetchSpy

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage } = useAI()

      await sendMessage('hello')

      const claudeCall = fetchSpy.mock.calls.find(
        (c: unknown[]) => (c[0] as string)?.toString().includes('/claude/')
      )
      const body = JSON.parse(claudeCall![1].body as string)
      expect(body.system).toContain('[[film_ext:')
      expect(body.system).toContain('[[song_ext:')
      expect(body.system).toContain('[[book_ext:')
    })
  })

  describe('sendMessage', () => {
    it('adds user message to store', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(
        mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"ok"}}\n\n'])
      )

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage } = useAI()

      await sendMessage('test message')

      const userMsg = chatStore.messages.find(m => m.role === 'user')
      expect(userMsg).toBeDefined()
      expect(userMsg!.content).toBe('test message')
    })

    it('creates assistant message placeholder', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(
        mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"hi"}}\n\n'])
      )

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage } = useAI()

      await sendMessage('test')

      const assistantMsg = chatStore.messages.find(m => m.role === 'assistant')
      expect(assistantMsg).toBeDefined()
      expect(assistantMsg!.content).toContain('hi')
    })

    it('sets isStreaming to true during stream and false after', async () => {
      const streamingStates: boolean[] = []

      globalThis.fetch = vi.fn().mockImplementation(() => {
        streamingStates.push(useChatStore().isStreaming)
        return Promise.resolve(
          mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"hello"}}\n\n'])
        )
      })

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage } = useAI()

      expect(chatStore.isStreaming).toBe(false)
      await sendMessage('hi')
      expect(chatStore.isStreaming).toBe(false)
      // During the streaming fetch call, isStreaming should have been true
      // (earlier non-streaming fetches like refreshWavlakeCatalog may also be captured)
      expect(streamingStates.some(s => s === true)).toBe(true)
    })

    it('handles stream errors gracefully', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        text: () => Promise.resolve('Internal Server Error'),
      })

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage, setProvider } = useAI()
      setProvider('claude') // Ensure claude provider

      await sendMessage('test')

      const assistantMsg = chatStore.messages.find(m => m.role === 'assistant')
      expect(assistantMsg).toBeDefined()
      expect(assistantMsg!.content).toContain('⚠')
      expect(chatStore.isStreaming).toBe(false)
    })

    it('handles connection errors gracefully', async () => {
      globalThis.fetch = vi.fn().mockRejectedValue(new Error('Network failure'))

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage, setProvider } = useAI()
      setProvider('claude') // Ensure claude provider

      await sendMessage('test')

      const assistantMsg = chatStore.messages.find(m => m.role === 'assistant')
      expect(assistantMsg).toBeDefined()
      expect(assistantMsg!.content).toContain('Connection error')
      expect(chatStore.isStreaming).toBe(false)
    })

    it('uses mock provider when set to mock', async () => {
      const { sendMessage, setProvider } = useAI()
      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false

      setProvider('mock')
      await sendMessage('echo this')

      const assistantMsg = chatStore.messages.find(m => m.role === 'assistant')
      expect(assistantMsg).toBeDefined()
      expect(assistantMsg!.content).toContain('echo this')
      expect(assistantMsg!.content).toContain('echo mode')
    })
  })

  describe('stopGeneration', () => {
    it('aborts active stream and sets isStreaming to false', async () => {
      // Create a fetch that returns a stream which rejects on abort
      globalThis.fetch = vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
        const signal = init?.signal
        return Promise.resolve({
          ok: true,
          body: new ReadableStream<Uint8Array>({
            start(controller) {
              // Send first chunk so readSSE enters its loop
              const encoder = new TextEncoder()
              controller.enqueue(encoder.encode('data: {"type":"content_block_delta","delta":{"text":"h"}}\n\n'))
              // When aborted, close the stream
              if (signal) {
                signal.addEventListener('abort', () => {
                  try { controller.close() } catch { /* already closed */ }
                })
              }
            },
          }),
          text: () => Promise.resolve(''),
        })
      })

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = false
      const { sendMessage, stopGeneration, setProvider } = useAI()
      setProvider('claude')

      const sendPromise = sendMessage('test')

      // Wait for fetch to start and first chunk to process
      await new Promise(r => setTimeout(r, 50))

      expect(chatStore.isStreaming).toBe(true)

      stopGeneration()
      expect(chatStore.isStreaming).toBe(false)

      await sendPromise
    })
  })

  describe('web search integration', () => {
    it('injects web results into system prompt when enabled', async () => {
      const mockResults = [
        { title: 'Result 1', url: 'https://example.com', content: 'Some content' },
      ]
      vi.mocked(searchWeb).mockResolvedValue(mockResults)

      const fetchSpy = vi.fn().mockResolvedValue(
        mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"answer"}}\n\n'])
      )
      globalThis.fetch = fetchSpy

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = true
      const { sendMessage } = useAI()

      await sendMessage('latest bitcoin news')

      // Find the Claude API call (not any dev-chats call)
      const claudeCall = fetchSpy.mock.calls.find(
        (c: unknown[]) => (c[0] as string)?.toString().includes('/claude/')
      )
      expect(claudeCall).toBeDefined()
      const body = JSON.parse(claudeCall![1].body as string)
      expect(body.system).toContain('Web search results')
      expect(body.system).toContain('Result 1')
      // FALSE on purpose: `proxyWebSearch = webSearchEnabled && !clientSearchSucceeded`.
      // The client already searched and injected the results above, so asking
      // the proxy to search again would be a second, redundant search per turn.
      // This assertion read `true` and had been failing since that change —
      // the test was stale, the behaviour is intentional.
      expect(body.webSearch).toBe(false)
    })

    it('asks the proxy to search when the client-side search finds nothing', async () => {
      // The other half of the same contract: no client results means nothing
      // was injected, so the proxy must still do the search.
      vi.mocked(searchWeb).mockResolvedValue([])

      const fetchSpy = vi.fn().mockResolvedValue(
        mockClaudeResponse(['data: {"type":"content_block_delta","delta":{"text":"answer"}}\n\n'])
      )
      globalThis.fetch = fetchSpy

      const chatStore = useChatStore()
      chatStore.webSearchEnabled = true
      const { sendMessage } = useAI()

      await sendMessage('latest bitcoin news')

      const claudeCall = fetchSpy.mock.calls.find(
        (c: unknown[]) => (c[0] as string)?.toString().includes('/claude/')
      )
      expect(claudeCall).toBeDefined()
      const body = JSON.parse(claudeCall![1].body as string)
      expect(body.system).not.toContain('Web search results')
      expect(body.webSearch).toBe(true)
    })
  })
})
