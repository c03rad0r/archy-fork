import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchGitHubAppInfo, fetchMultipleAppInfo } from '../githubAppInfo'

const mockFetch = vi.fn()
vi.stubGlobal('fetch', mockFetch)

describe('fetchGitHubAppInfo', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns empty object for invalid repo URL', async () => {
    const result = await fetchGitHubAppInfo('not-a-url', 'test')
    expect(result).toEqual({})
    expect(mockFetch).not.toHaveBeenCalled()
  })

  it('returns empty object when repo API returns non-OK', async () => {
    // Start9 repo check
    mockFetch.mockResolvedValueOnce({ ok: false })
    // Original repo fetch fails
    mockFetch.mockResolvedValueOnce({ ok: false, status: 404 })

    const result = await fetchGitHubAppInfo('https://github.com/owner/repo', 'app')
    expect(result).toEqual({})
  })

  it('fetches repo info successfully', async () => {
    // Start9 wrapper check — not found
    mockFetch.mockResolvedValueOnce({ ok: false })
    // Repo API call
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({
        description: 'A Bitcoin node',
        homepage: 'https://bitcoin.org',
        html_url: 'https://github.com/owner/repo',
      }),
    })
    // README fetch
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ content: btoa('# README') }),
    })
    // Icon path checks — all fail
    for (let i = 0; i < 6; i++) {
      mockFetch.mockResolvedValueOnce({ ok: false })
    }
    // Releases check — no icon
    mockFetch.mockResolvedValueOnce({ ok: false })
    // Raw icon URL HEAD checks — all fail
    for (let i = 0; i < 6; i++) {
      mockFetch.mockResolvedValueOnce({ ok: false })
    }

    const result = await fetchGitHubAppInfo('https://github.com/owner/repo', 'app')
    expect(result.description).toBe('A Bitcoin node')
    expect(result.readme).toBe('# README')
    expect(result.homepage).toBe('https://bitcoin.org')
  })

  it('finds icon from repository contents', async () => {
    // Start9 wrapper — not found
    mockFetch.mockResolvedValueOnce({ ok: false })
    // Repo API
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ description: '', html_url: 'https://github.com/o/r' }),
    })
    // README
    mockFetch.mockResolvedValueOnce({ ok: false })
    // First icon path (icon.png) — found!
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ download_url: 'https://raw.github.com/o/r/main/icon.png' }),
    })

    const result = await fetchGitHubAppInfo('https://github.com/o/r', 'test')
    expect(result.icon).toBe('https://raw.github.com/o/r/main/icon.png')
  })

  it('tries Start9Labs wrapper repo first', async () => {
    // Start9 wrapper check — found!
    mockFetch.mockResolvedValueOnce({ ok: true })
    // Now fetches Start9Labs/app-startos repo
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({
        description: 'Start9 wrapper',
        html_url: 'https://github.com/Start9Labs/bitcoin-startos',
      }),
    })
    // README
    mockFetch.mockResolvedValueOnce({ ok: false })
    // Icon paths — all fail
    for (let i = 0; i < 6; i++) {
      mockFetch.mockResolvedValueOnce({ ok: false })
    }
    // Releases
    mockFetch.mockResolvedValueOnce({ ok: false })
    // Raw URLs
    for (let i = 0; i < 6; i++) {
      mockFetch.mockResolvedValueOnce({ ok: false })
    }

    const result = await fetchGitHubAppInfo('https://github.com/bitcoin/bitcoin', 'bitcoin')
    expect(result.description).toBe('Start9 wrapper')
  })

  it('handles fetch errors gracefully', async () => {
    mockFetch.mockRejectedValue(new Error('Network error'))
    const result = await fetchGitHubAppInfo('https://github.com/owner/repo', 'app')
    expect(result).toEqual({})
  })
})

describe('fetchMultipleAppInfo', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
  })

  it('returns empty record for empty input', async () => {
    const result = await fetchMultipleAppInfo([])
    expect(result).toEqual({})
  })

  it('fetches info for multiple apps', async () => {
    // Each app triggers multiple fetch calls, but they all return non-matching URLs
    mockFetch.mockResolvedValue({ ok: false })

    const apps = [
      { id: 'app1', 'wrapper-repo': 'not-a-github-url' },
      { id: 'app2', 'wrapper-repo': 'also-invalid' },
    ]
    const result = await fetchMultipleAppInfo(apps)
    expect(result.app1).toEqual({})
    expect(result.app2).toEqual({})
  })
})
