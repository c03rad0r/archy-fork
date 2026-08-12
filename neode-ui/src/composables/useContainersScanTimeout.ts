// Escape hatch for the "Checking containers…" / "Checking..." states.
//
// If the server never flips `containers-scanned` to true (e.g. the UI missed
// a websocket broadcast), views gating on it would spin forever. This starts
// a timeout once initial data has loaded and, if the scan flag is still false
// when it fires, treats the scan as complete so the UI falls through to its
// real empty/install states. A periodic store-level resync exists too — this
// is the belt-and-suspenders guarantee that the spinner is always bounded.

import { computed, getCurrentInstance, onBeforeUnmount, ref, watch, type Ref } from 'vue'

const DEFAULT_SCAN_TIMEOUT_MS = 20_000

export function useContainersScanTimeout(
  containersScanned: Ref<boolean>,
  hasLoadedInitialData: Ref<boolean>,
  timeoutMs: number = DEFAULT_SCAN_TIMEOUT_MS,
) {
  const scanTimedOut = ref(false)
  let timer: ReturnType<typeof setTimeout> | undefined

  function clearTimer(): void {
    if (timer !== undefined) {
      clearTimeout(timer)
      timer = undefined
    }
  }

  watch(
    [containersScanned, hasLoadedInitialData],
    ([scanned, loaded]) => {
      if (scanned) {
        // Real signal arrived — cancel the escape hatch.
        clearTimer()
        scanTimedOut.value = false
        return
      }
      if (loaded && timer === undefined && !scanTimedOut.value) {
        timer = setTimeout(() => {
          scanTimedOut.value = true
          timer = undefined
        }, timeoutMs)
      }
    },
    { immediate: true },
  )

  if (getCurrentInstance()) onBeforeUnmount(clearTimer)

  /** True once the server reports the scan done OR the timeout has elapsed. */
  const effectiveContainersScanned = computed(
    () => containersScanned.value || scanTimedOut.value,
  )

  return { effectiveContainersScanned, scanTimedOut }
}
