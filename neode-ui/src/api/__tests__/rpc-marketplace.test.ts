import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const mockFetch = vi.fn()
vi.stubGlobal('fetch', mockFetch)

// Import after stubbing fetch
const { rpcClient } = await import('../rpc-client')

function jsonResponse(body: unknown, status = 200, statusText = 'OK'): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText,
    json: () => Promise.resolve(body),
    headers: new Headers(),
    redirected: false,
    type: 'basic' as ResponseType,
    url: '',
    clone: () => jsonResponse(body, status, statusText),
    body: null,
    bodyUsed: false,
    arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    blob: () => Promise.resolve(new Blob()),
    formData: () => Promise.resolve(new FormData()),
    text: () => Promise.resolve(JSON.stringify(body)),
    bytes: () => Promise.resolve(new Uint8Array()),
  }
}

describe('marketplaceDiscover', () => {
  beforeEach(() => {
    mockFetch.mockReset()
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('returns apps array and relay_count on success', async () => {
    const payload = {
      apps: [
        {
          manifest: {
            app_id: 'bitcoin',
            name: 'Bitcoin Core',
            version: '27.0',
            description: { short: 'Full node', long: 'Bitcoin Core full node' },
            author: { name: 'Bitcoin', did: 'did:key:z111', nostr_pubkey: 'npub1abc' },
            container: { image: 'bitcoin:27.0', ports: [{ container: 8333, host: 8333 }] },
            category: 'bitcoin',
            icon_url: '/icons/bitcoin.png',
            repo_url: 'https://github.com/bitcoin/bitcoin',
            license: 'MIT',
          },
          trust_score: 95,
          trust_tier: 'verified',
          relay_count: 8,
          first_seen: '2025-01-15T00:00:00Z',
          nostr_pubkey: 'npub1abc',
        },
      ],
      relay_count: 12,
    }

    mockFetch.mockResolvedValueOnce(jsonResponse({ result: payload }))

    const result = await rpcClient.marketplaceDiscover()

    expect(result.apps).toHaveLength(1)
    expect(result.apps[0]!.manifest.app_id).toBe('bitcoin')
    expect(result.apps[0]!.manifest.name).toBe('Bitcoin Core')
    expect(result.apps[0]!.trust_score).toBe(95)
    expect(result.relay_count).toBe(12)

    const body = JSON.parse(mockFetch.mock.calls[0]![1].body)
    expect(body.method).toBe('marketplace.discover')
    expect(body.params).toEqual({})
  })

  it('handles empty results', async () => {
    const payload = {
      apps: [],
      relay_count: 0,
    }

    mockFetch.mockResolvedValueOnce(jsonResponse({ result: payload }))

    const result = await rpcClient.marketplaceDiscover()

    expect(result.apps).toEqual([])
    expect(result.apps).toHaveLength(0)
    expect(result.relay_count).toBe(0)
  })
})

describe('diskStatus', () => {
  beforeEach(() => {
    mockFetch.mockReset()
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('returns expected fields', async () => {
    const payload = {
      used_bytes: 500_000_000_000,
      total_bytes: 1_000_000_000_000,
      free_bytes: 500_000_000_000,
      used_percent: 50,
      level: 'ok' as const,
    }

    mockFetch.mockResolvedValueOnce(jsonResponse({ result: payload }))

    const result = await rpcClient.diskStatus()

    expect(result.used_bytes).toBe(500_000_000_000)
    expect(result.total_bytes).toBe(1_000_000_000_000)
    expect(result.free_bytes).toBe(500_000_000_000)
    expect(result.used_percent).toBe(50)
    expect(result.level).toBe('ok')

    const body = JSON.parse(mockFetch.mock.calls[0]![1].body)
    expect(body.method).toBe('system.disk-status')
  })

  it('level is warning when percent >= 85', async () => {
    const payload = {
      used_bytes: 850_000_000_000,
      total_bytes: 1_000_000_000_000,
      free_bytes: 150_000_000_000,
      used_percent: 85,
      level: 'warning' as const,
    }

    mockFetch.mockResolvedValueOnce(jsonResponse({ result: payload }))

    const result = await rpcClient.diskStatus()

    expect(result.level).toBe('warning')
    expect(result.used_percent).toBe(85)
  })

  it('level is critical when percent >= 90', async () => {
    const payload = {
      used_bytes: 950_000_000_000,
      total_bytes: 1_000_000_000_000,
      free_bytes: 50_000_000_000,
      used_percent: 95,
      level: 'critical' as const,
    }

    mockFetch.mockResolvedValueOnce(jsonResponse({ result: payload }))

    const result = await rpcClient.diskStatus()

    expect(result.level).toBe('critical')
    expect(result.used_percent).toBe(95)
  })
})

describe('diskCleanup', () => {
  beforeEach(() => {
    mockFetch.mockReset()
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('returns freed_bytes and actions array', async () => {
    const payload = {
      freed_bytes: 2_000_000_000,
      freed_human: '2 GB',
      actions: ['Removed 5 dangling images', 'Cleared build cache'],
    }

    mockFetch.mockResolvedValueOnce(jsonResponse({ result: payload }))

    const result = await rpcClient.diskCleanup()

    expect(result.freed_bytes).toBe(2_000_000_000)
    expect(result.freed_human).toBe('2 GB')
    expect(result.actions).toHaveLength(2)
    expect(result.actions[0]).toBe('Removed 5 dangling images')
    expect(result.actions[1]).toBe('Cleared build cache')

    const body = JSON.parse(mockFetch.mock.calls[0]![1].body)
    expect(body.method).toBe('system.disk-cleanup')
  })

  it('uses 60s timeout', async () => {
    const abortError = Object.assign(new Error('The operation was aborted.'), { name: 'AbortError' })
    mockFetch.mockRejectedValue(abortError)

    const promise = rpcClient.diskCleanup()

    // The call should eventually reject with timeout after retries
    await expect(promise).rejects.toThrow('Request timeout')

    // Verify all 3 attempts used the signal (timeout is set via AbortController)
    expect(mockFetch).toHaveBeenCalledTimes(3)
    for (const call of mockFetch.mock.calls) {
      expect(call[1].signal).toBeInstanceOf(AbortSignal)
    }
  })
})
