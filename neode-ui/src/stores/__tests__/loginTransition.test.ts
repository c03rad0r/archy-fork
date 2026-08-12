import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useLoginTransitionStore } from '../loginTransition'

describe('useLoginTransitionStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('starts with all flags false', () => {
    const store = useLoginTransitionStore()
    expect(store.justLoggedIn).toBe(false)
    expect(store.justCompletedOnboarding).toBe(false)
    expect(store.pendingWelcomeTyping).toBe(false)
    expect(store.startWelcomeTyping).toBe(false)
  })

  it('setJustCompletedOnboarding updates justCompletedOnboarding', () => {
    const store = useLoginTransitionStore()
    store.setJustCompletedOnboarding(true)
    expect(store.justCompletedOnboarding).toBe(true)
    store.setJustCompletedOnboarding(false)
    expect(store.justCompletedOnboarding).toBe(false)
  })

  it('setJustLoggedIn updates justLoggedIn', () => {
    const store = useLoginTransitionStore()
    store.setJustLoggedIn(true)
    expect(store.justLoggedIn).toBe(true)
    store.setJustLoggedIn(false)
    expect(store.justLoggedIn).toBe(false)
  })

  it('setPendingWelcomeTyping updates pendingWelcomeTyping', () => {
    const store = useLoginTransitionStore()
    store.setPendingWelcomeTyping(true)
    expect(store.pendingWelcomeTyping).toBe(true)
    store.setPendingWelcomeTyping(false)
    expect(store.pendingWelcomeTyping).toBe(false)
  })

  it('setStartWelcomeTyping updates startWelcomeTyping', () => {
    const store = useLoginTransitionStore()
    store.setStartWelcomeTyping(true)
    expect(store.startWelcomeTyping).toBe(true)
    store.setStartWelcomeTyping(false)
    expect(store.startWelcomeTyping).toBe(false)
  })

  it('flags are independent of each other', () => {
    const store = useLoginTransitionStore()
    store.setJustLoggedIn(true)
    store.setPendingWelcomeTyping(true)
    expect(store.startWelcomeTyping).toBe(false)

    store.setStartWelcomeTyping(true)
    store.setJustLoggedIn(false)
    expect(store.pendingWelcomeTyping).toBe(true)
    expect(store.startWelcomeTyping).toBe(true)
  })
})
