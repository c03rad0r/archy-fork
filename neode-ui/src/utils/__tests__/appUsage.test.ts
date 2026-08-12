import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getAppUsage, recordAppLaunch } from '../appUsage'

describe('appUsage', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.restoreAllMocks()
  })

  it('starts empty when no usage has been recorded', () => {
    expect(getAppUsage()).toEqual({})
  })

  it('records launch count and latest launch time', () => {
    recordAppLaunch('filebrowser', 1000)
    recordAppLaunch('filebrowser', 2000)

    expect(getAppUsage()).toEqual({
      filebrowser: {
        count: 2,
        lastLaunchedAt: 2000,
      },
    })
  })

  it('ignores corrupt stored usage', () => {
    localStorage.setItem('archipelago-app-usage', 'not-json')

    expect(getAppUsage()).toEqual({})
  })
})
