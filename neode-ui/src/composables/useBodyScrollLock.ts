import { onBeforeUnmount, watch, type Ref } from 'vue'

let activeLocks = 0
let previousOverflow = ''

function lock() {
  if (typeof document === 'undefined') return
  if (activeLocks === 0) {
    previousOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
  }
  activeLocks += 1
}

function unlock() {
  if (typeof document === 'undefined' || activeLocks === 0) return
  activeLocks -= 1
  if (activeLocks === 0) {
    document.body.style.overflow = previousOverflow
    previousOverflow = ''
  }
}

export function useBodyScrollLock(active: Ref<boolean>) {
  watch(
    active,
    (isActive, wasActive) => {
      if (isActive && !wasActive) lock()
      if (!isActive && wasActive) unlock()
    },
    { immediate: true }
  )

  onBeforeUnmount(() => {
    if (active.value) unlock()
  })
}
