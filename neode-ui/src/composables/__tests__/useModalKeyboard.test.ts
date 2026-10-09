import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref, nextTick } from 'vue'
import { mount } from '@vue/test-utils'
import { useModalKeyboard } from '../useModalKeyboard'
import { defineComponent } from 'vue'

// We need to test the composable inside a component
function createTestComponent(onCloseFn: () => void) {
  return defineComponent({
    setup() {
      const containerRef = ref<HTMLElement | null>(null)
      const isOpen = ref(false)
      const restoreFocusRef = ref<HTMLElement | null>(null)

      useModalKeyboard(containerRef, isOpen, onCloseFn, {
        restoreFocusRef,
      })

      return { containerRef, isOpen, restoreFocusRef }
    },
    template: `
      <div>
        <button id="trigger">Trigger</button>
        <div v-if="isOpen" ref="containerRef">
          <button id="btn1">One</button>
          <button id="btn2">Two</button>
          <button id="btn3">Three</button>
        </div>
      </div>
    `,
  })
}

describe('useModalKeyboard', () => {
  let closeFn: ReturnType<typeof vi.fn<() => void>>

  beforeEach(() => {
    closeFn = vi.fn<() => void>()
  })

  it('calls onClose when Escape is pressed and modal is open', async () => {
    const Comp = createTestComponent(closeFn)
    const wrapper = mount(Comp, { attachTo: document.body })

    wrapper.vm.isOpen = true
    await nextTick()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))

    expect(closeFn).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('does not call onClose when modal is closed', () => {
    const Comp = createTestComponent(closeFn)
    const wrapper = mount(Comp, { attachTo: document.body })

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))

    expect(closeFn).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('cleans up listener on unmount', () => {
    const removeSpy = vi.spyOn(window, 'removeEventListener')
    const Comp = createTestComponent(closeFn)
    const wrapper = mount(Comp, { attachTo: document.body })

    wrapper.unmount()

    expect(removeSpy).toHaveBeenCalledWith('keydown', expect.any(Function), true)
    removeSpy.mockRestore()
  })
})
