import { describe, it, expect, vi } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'
import { useModalHistory } from '../useModalHistory'

describe('modal history transitions', () => {
  it('replaces a setup dialog with its funding dialog without backing out of it', async () => {
    window.history.replaceState({ position: 5 }, '')
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => {})
    const push = vi.spyOn(window.history, 'pushState')
    const first = ref(false), second = ref(false)
    const closeFirst = vi.fn(() => { first.value = false })
    const closeSecond = vi.fn(() => { second.value = false })
    const scope = effectScope()
    scope.run(() => { useModalHistory(first, closeFirst); useModalHistory(second, closeSecond) })
    first.value = true
    await nextTick()
    expect(push).toHaveBeenCalledTimes(1)
    first.value = false
    second.value = true
    await nextTick()
    await Promise.resolve()
    expect(back).not.toHaveBeenCalled()
    expect(push).toHaveBeenCalledTimes(1)
    expect(window.history.state).toEqual({ position: 5, __archyModal: 1 })
    window.history.replaceState({ position: 5 }, '')
    window.dispatchEvent(new PopStateEvent('popstate', { state: { position: 5 } }))
    await nextTick()
    expect(closeSecond).toHaveBeenCalledOnce()
    expect(closeFirst).not.toHaveBeenCalled()
    expect(back).not.toHaveBeenCalled()
    scope.stop()
    vi.restoreAllMocks()
  })
})
