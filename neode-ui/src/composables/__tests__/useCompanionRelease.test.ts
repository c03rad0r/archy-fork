import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  companionRelease,
  companionReleaseMarker,
  loadCompanionRelease,
} from '../useCompanionRelease'

beforeEach(() => {
  companionRelease.value = null
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('companion release metadata', () => {
  it('uses the Android build number as the durable prompt marker', () => {
    expect(companionReleaseMarker({ versionName: '0.5.32', versionCode: 52 })).toBe('build:52')
    expect(companionReleaseMarker({ versionName: '0.5.32', versionCode: 0 })).toBe(
      'version:0.5.32',
    )
    expect(companionReleaseMarker(null)).toBe('1')
  })

  it('loads the same no-cache metadata used by the download prompt', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ versionName: '0.5.32', versionCode: 52 }),
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(loadCompanionRelease()).resolves.toEqual({
      versionName: '0.5.32',
      versionCode: 52,
    })
    expect(companionRelease.value).toEqual({ versionName: '0.5.32', versionCode: 52 })
    expect(fetchMock).toHaveBeenCalledWith('/packages/archipelago-companion.json', {
      cache: 'no-store',
    })
  })
})
