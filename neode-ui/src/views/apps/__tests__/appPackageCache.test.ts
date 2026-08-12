import { describe, expect, it } from 'vitest'
import { ref, nextTick } from 'vue'
import { PackageState, type PackageDataEntry } from '@/types/api'
import { useLastKnownPackages, type PackageMap } from '../appPackageCache'

function makePkg(id: string): PackageDataEntry {
  return {
    state: PackageState.Running,
    manifest: {
      id,
      title: id,
      version: '1.0.0',
      description: { short: '', long: '' },
      'release-notes': '',
      license: '',
      'wrapper-repo': '',
      'upstream-repo': '',
      'support-site': '',
      'marketing-site': '',
      'donation-url': null,
    },
    'static-files': { license: '', instructions: '', icon: '' },
  }
}

describe('useLastKnownPackages', () => {
  it('keeps the last package list visible while the scanner reports not ready', async () => {
    const livePackages = ref<PackageMap>({ filebrowser: makePkg('filebrowser') })
    const containersScanned = ref(true)
    const cache = useLastKnownPackages(livePackages, containersScanned)

    expect(Object.keys(cache.packages.value)).toEqual(['filebrowser'])
    expect(cache.isUsingLastKnownPackages.value).toBe(false)

    containersScanned.value = false
    livePackages.value = {}
    await nextTick()

    expect(Object.keys(cache.packages.value)).toEqual(['filebrowser'])
    expect(cache.isUsingLastKnownPackages.value).toBe(true)
  })

  it('accepts an empty list once the scanner has completed', async () => {
    const livePackages = ref<PackageMap>({ filebrowser: makePkg('filebrowser') })
    const containersScanned = ref(true)
    const cache = useLastKnownPackages(livePackages, containersScanned)

    livePackages.value = {}
    await nextTick()

    expect(cache.packages.value).toEqual({})
    expect(cache.lastKnownPackages.value).toEqual({})
    expect(cache.isUsingLastKnownPackages.value).toBe(false)
  })
})
