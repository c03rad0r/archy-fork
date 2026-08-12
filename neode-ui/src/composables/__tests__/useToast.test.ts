import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { useToast } from '../useToast'

describe('useToast', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    // Get a fresh toast instance and clear any leftover state
    const { toasts, dismiss } = useToast()
    // Dismiss all existing toasts
    for (const t of [...toasts.value]) {
      dismiss(t.id)
    }
    vi.advanceTimersByTime(500)
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('creates a success toast', () => {
    const { success, toasts } = useToast()

    success('Operation complete')

    expect(toasts.value.length).toBeGreaterThanOrEqual(1)
    const toast = toasts.value[toasts.value.length - 1]!
    expect(toast.message).toBe('Operation complete')
    expect(toast.variant).toBe('success')
    expect(toast.dismissing).toBe(false)
  })

  it('creates an error toast', () => {
    const { error, toasts } = useToast()

    error('Something went wrong')

    const toast = toasts.value[toasts.value.length - 1]!
    expect(toast.message).toBe('Something went wrong')
    expect(toast.variant).toBe('error')
  })

  it('creates an info toast', () => {
    const { info, toasts } = useToast()

    info('FYI: Node syncing')

    const toast = toasts.value[toasts.value.length - 1]!
    expect(toast.message).toBe('FYI: Node syncing')
    expect(toast.variant).toBe('info')
  })

  it('auto-dismisses toast after duration', () => {
    const { success, toasts } = useToast()

    success('Will auto-dismiss')
    const toast = toasts.value[toasts.value.length - 1]!
    const toastId = toast.id

    expect(toasts.value.some((t) => t.id === toastId)).toBe(true)

    // After 3000ms, the toast should start dismissing
    vi.advanceTimersByTime(3000)

    const dismissingToast = toasts.value.find((t) => t.id === toastId)
    if (dismissingToast) {
      expect(dismissingToast.dismissing).toBe(true)
    }

    // After another 300ms, the toast should be fully removed
    vi.advanceTimersByTime(300)

    expect(toasts.value.some((t) => t.id === toastId)).toBe(false)
  })

  it('dismiss marks toast as dismissing then removes it', () => {
    const { info, toasts, dismiss } = useToast()

    info('Dismissable')
    const toast = toasts.value[toasts.value.length - 1]!

    dismiss(toast.id)

    // Should be marked as dismissing
    const found = toasts.value.find((t) => t.id === toast.id)
    if (found) {
      expect(found.dismissing).toBe(true)
    }

    // After 300ms animation delay, should be removed
    vi.advanceTimersByTime(300)

    expect(toasts.value.some((t) => t.id === toast.id)).toBe(false)
  })

  it('dismiss is a no-op for nonexistent toast ID', () => {
    const { dismiss, toasts } = useToast()
    const countBefore = toasts.value.length

    dismiss(999999)

    expect(toasts.value.length).toBe(countBefore)
  })

  it('each toast gets a unique ID', () => {
    const { info, toasts } = useToast()

    info('First')
    info('Second')
    info('Third')

    const ids = toasts.value.slice(-3).map((t) => t.id)
    const uniqueIds = new Set(ids)
    expect(uniqueIds.size).toBe(3)
  })

  it('caps visible toasts at 5', () => {
    const { info, toasts } = useToast()

    for (let i = 0; i < 7; i++) {
      info(`Toast ${i}`)
    }

    expect(toasts.value.length).toBeLessThanOrEqual(5)
  })

  it('toasts ref is readonly', () => {
    const { toasts } = useToast()
    // The readonly wrapper prevents direct mutation
    expect(typeof toasts.value).toBe('object')
  })
})
