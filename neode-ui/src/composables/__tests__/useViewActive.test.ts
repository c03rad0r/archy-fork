import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h, KeepAlive, ref, Teleport } from 'vue'
import { useViewActive } from '../useViewActive'

/**
 * Teleported chrome must not outlive the screen that raised it.
 *
 * Main tabs are KeepAlive'd, so navigating away deactivates a view instead of
 * unmounting it. Anything Teleported to <body> is outside the view's subtree
 * and therefore survives that deactivation — Mesh's mobile tab bar and the
 * shared BackButton stayed pinned above the bottom bar on every other screen.
 */
const ViewWithTeleportedChrome = defineComponent({
  name: 'ViewWithTeleportedChrome',
  setup() {
    const isViewActive = useViewActive()
    return () =>
      h('div', [
        isViewActive.value
          ? h(Teleport, { to: 'body' }, [h('button', { class: 'leaky-chrome' }, 'Back')])
          : null,
      ])
  },
})

const Other = defineComponent({ name: 'Other', setup: () => () => h('div', 'other screen') })

function chromeCount() {
  return document.body.querySelectorAll('.leaky-chrome').length
}

describe('useViewActive', () => {
  it('removes teleported chrome when the view is deactivated, and restores it on return', async () => {
    const showFirst = ref(true)
    const host = mount(
      defineComponent({
        setup: () => () =>
          h(KeepAlive, null, {
            default: () => (showFirst.value ? h(ViewWithTeleportedChrome) : h(Other)),
          }),
      }),
      { attachTo: document.body },
    )

    expect(chromeCount()).toBe(1)

    // Navigate away: KeepAlive DEACTIVATES rather than unmounts.
    showFirst.value = false
    await host.vm.$nextTick()
    expect(chromeCount()).toBe(0)

    // Returning must bring it back — the whole point of KeepAlive is that the
    // instance survived, so the chrome has to come back with it.
    showFirst.value = true
    await host.vm.$nextTick()
    expect(chromeCount()).toBe(1)

    host.unmount()
  })

  it('keeps the instance alive across the round trip (performance is not sacrificed)', async () => {
    const showFirst = ref(true)
    const seen: number[] = []
    const Counting = defineComponent({
      name: 'Counting',
      setup() {
        const isViewActive = useViewActive()
        const uid = Math.random()
        seen.push(uid)
        return () => h('div', [isViewActive.value ? h(Teleport, { to: 'body' }, [h('i', { class: 'leaky-chrome' })]) : null])
      },
    })

    const host = mount(
      defineComponent({
        setup: () => () =>
          h(KeepAlive, null, { default: () => (showFirst.value ? h(Counting) : h(Other)) }),
      }),
      { attachTo: document.body },
    )

    showFirst.value = false
    await host.vm.$nextTick()
    showFirst.value = true
    await host.vm.$nextTick()

    // setup() ran once: the view was cached, not re-created. If this ever
    // becomes 2, the fix has been "solved" by throwing away the perf work.
    expect(seen.length).toBe(1)
    expect(chromeCount()).toBe(1)

    host.unmount()
  })

  it('defaults to active outside a KeepAlive boundary', () => {
    // Neither hook fires here. A component used both ways — or mounted bare in
    // a test — must render normally rather than stay invisible forever.
    const host = mount(ViewWithTeleportedChrome, { attachTo: document.body })
    expect(chromeCount()).toBe(1)
    host.unmount()
  })
})
