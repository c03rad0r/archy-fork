import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref, nextTick } from 'vue'
import { useContainersScanTimeout } from '../useContainersScanTimeout'

describe('useContainersScanTimeout', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('reflects the real scanned flag when it arrives before the timeout', async () => {
    const scanned = ref(false)
    const loaded = ref(true)
    const { effectiveContainersScanned, scanTimedOut } = useContainersScanTimeout(scanned, loaded, 20_000)

    expect(effectiveContainersScanned.value).toBe(false)
    scanned.value = true
    await nextTick()
    expect(effectiveContainersScanned.value).toBe(true)
    expect(scanTimedOut.value).toBe(false)
  })

  it('does not start the timeout until initial data has loaded', async () => {
    const scanned = ref(false)
    const loaded = ref(false)
    const { effectiveContainersScanned } = useContainersScanTimeout(scanned, loaded, 20_000)

    vi.advanceTimersByTime(60_000)
    expect(effectiveContainersScanned.value).toBe(false)

    loaded.value = true
    await nextTick()
    vi.advanceTimersByTime(20_000)
    expect(effectiveContainersScanned.value).toBe(true)
  })

  it('falls through after the timeout even if the flag never arrives', async () => {
    const scanned = ref(false)
    const loaded = ref(true)
    const { effectiveContainersScanned, scanTimedOut } = useContainersScanTimeout(scanned, loaded, 20_000)

    vi.advanceTimersByTime(19_999)
    expect(effectiveContainersScanned.value).toBe(false)
    vi.advanceTimersByTime(1)
    expect(effectiveContainersScanned.value).toBe(true)
    expect(scanTimedOut.value).toBe(true)
  })

  it('cancels the escape hatch when the real flag arrives', async () => {
    const scanned = ref(false)
    const loaded = ref(true)
    const { scanTimedOut } = useContainersScanTimeout(scanned, loaded, 20_000)

    vi.advanceTimersByTime(10_000)
    scanned.value = true
    await nextTick()
    vi.advanceTimersByTime(60_000)
    expect(scanTimedOut.value).toBe(false)
  })
})
