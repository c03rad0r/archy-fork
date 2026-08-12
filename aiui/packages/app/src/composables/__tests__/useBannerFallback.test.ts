// Regression pin for the stale-banner bug (operator-reported 2026-08-04):
// detail views are REUSED, not remounted, when a different item is selected in
// the content window. The composable's stage/apiUrl/primaryIndex used to
// survive that change, so the context-surface banner kept showing the previous
// item's artwork forever. These tests pin the identity-keyed reset and the
// stale-fetch generation guard.
import { describe, it, expect, vi } from 'vitest'
import { ref, nextTick } from 'vue'
import { useBannerFallback } from '../useBannerFallback'

function deferred<T>() {
  let resolve!: (v: T) => void
  const promise = new Promise<T>((r) => { resolve = r })
  return { promise, resolve }
}

describe('useBannerFallback', () => {
  it('resets to the new item\'s primary URL after the previous item resolved via API', async () => {
    const title = ref('Item A')
    const primaries = ref<(string | null)[]>([])
    const apiFetch = vi.fn().mockResolvedValue({ posterUrl: 'https://api/a.jpg', backdropUrl: null })

    const { bannerSrc } = useBannerFallback({
      primaryUrls: () => primaries.value,
      apiFetch,
      title: () => title.value,
    })

    // Item A has no primaries — resolves through the API path.
    await nextTick()
    await vi.waitFor(() => expect(bannerSrc.value).toBe('https://api/a.jpg'))

    // Select item B, which has its own primary URL. Before the fix, stage was
    // stuck at 'api' and bannerSrc kept returning A's artwork.
    title.value = 'Item B'
    primaries.value = ['https://cdn/b-poster.jpg']
    await nextTick()
    await nextTick()

    expect(bannerSrc.value).toBe('https://cdn/b-poster.jpg')
  })

  it('discards a stale API resolution that lands after the item changed', async () => {
    const title = ref('Item A')
    const primaries = ref<(string | null)[]>([])
    const slow = deferred<{ posterUrl: string | null; backdropUrl: string | null }>()
    const apiFetch = vi.fn().mockReturnValue(slow.promise)

    const { bannerSrc } = useBannerFallback({
      primaryUrls: () => primaries.value,
      apiFetch,
      title: () => title.value,
    })
    await nextTick()

    // Item changes while A's fetch is still in flight.
    title.value = 'Item B'
    primaries.value = ['https://cdn/b-poster.jpg']
    await nextTick()
    await nextTick()
    expect(bannerSrc.value).toBe('https://cdn/b-poster.jpg')

    // A's fetch finally resolves — it must NOT stamp A's artwork over B.
    slow.resolve({ posterUrl: 'https://api/a-late.jpg', backdropUrl: null })
    await slow.promise
    await nextTick()

    expect(bannerSrc.value).toBe('https://cdn/b-poster.jpg')
  })

  it('re-runs the no-primary API kickoff for the new item after a reset', async () => {
    const title = ref('Item A')
    const primaries = ref<(string | null)[]>(['https://cdn/a.jpg'])
    const apiFetch = vi.fn().mockResolvedValue({ posterUrl: 'https://api/b.jpg', backdropUrl: null })

    const { bannerSrc } = useBannerFallback({
      primaryUrls: () => primaries.value,
      apiFetch,
      title: () => title.value,
    })
    expect(bannerSrc.value).toBe('https://cdn/a.jpg')
    expect(apiFetch).not.toHaveBeenCalled()

    // New item with NO primaries — the kickoff must fire again post-reset.
    title.value = 'Item B'
    primaries.value = []
    await nextTick()
    await nextTick()
    await vi.waitFor(() => expect(bannerSrc.value).toBe('https://api/b.jpg'))
    expect(apiFetch).toHaveBeenCalledTimes(1)
  })
})
