import { onUnmounted } from 'vue'

/**
 * Memory leak prevention composable.
 * Tracks intervals, timeouts, listeners, and observers
 * for automatic cleanup on component unmount.
 */
export function useCleanup() {
  const intervals: ReturnType<typeof setInterval>[] = []
  const timeouts: ReturnType<typeof setTimeout>[] = []
  const listeners: { target: EventTarget; event: string; handler: EventListenerOrEventListenerObject }[] = []
  const observers: { disconnect: () => void }[] = []

  function addInterval(fn: () => void, ms: number): ReturnType<typeof setInterval> {
    const id = setInterval(fn, ms)
    intervals.push(id)
    return id
  }

  function addTimeout(fn: () => void, ms: number): ReturnType<typeof setTimeout> {
    const id = setTimeout(fn, ms)
    timeouts.push(id)
    return id
  }

  function addListener(target: EventTarget, event: string, handler: EventListenerOrEventListenerObject, options?: AddEventListenerOptions) {
    target.addEventListener(event, handler, options)
    listeners.push({ target, event, handler })
  }

  function addObserver(observer: { disconnect: () => void }) {
    observers.push(observer)
    return observer
  }

  onUnmounted(() => {
    for (const id of intervals) clearInterval(id)
    for (const id of timeouts) clearTimeout(id)
    for (const { target, event, handler } of listeners) {
      target.removeEventListener(event, handler)
    }
    for (const obs of observers) obs.disconnect()
  })

  return {
    addInterval,
    addTimeout,
    addListener,
    addObserver,
  }
}
