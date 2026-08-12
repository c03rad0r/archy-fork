import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { KeepAlive, defineComponent, h, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useCachedResource } from '../useCachedResource'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

describe('useCachedResource', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    sessionStorage.clear()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('does not refetch on reactivation within the TTL, and refetches exactly once after the TTL lapses', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date(2030, 0, 1, 0, 0, 0))
    const fetcher = vi.fn().mockResolvedValue('v1')

    const Consumer = defineComponent({
      setup() {
        const resource = useCachedResource<string>({
          key: 'test.reactivation-key',
          fetcher,
          ttlMs: 1000,
          persist: false,
        })
        return () => h('div', resource.data.value ?? '')
      },
    })
    const Other = defineComponent({ render: () => h('div', 'other') })

    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(Consumer, { key: 'consumer' }) : h(Other, { key: 'other' }),
        )
      },
    })

    const wrapper = mount(Host)
    await flushPromises()
    expect(fetcher).toHaveBeenCalledTimes(1)

    // Deactivate then reactivate inside the TTL — no additional fetch.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(fetcher).toHaveBeenCalledTimes(1)

    // Deactivate, advance past the TTL, reactivate — exactly one more fetch.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    vi.setSystemTime(new Date(2030, 0, 1, 0, 0, 2)) // +2s, past the 1s TTL
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(fetcher).toHaveBeenCalledTimes(2)
  })

  it('mounts and fetches without throwing outside any KeepAlive boundary', async () => {
    const fetcher = vi.fn().mockResolvedValue('bare')
    const Consumer = defineComponent({
      setup() {
        const resource = useCachedResource<string>({ key: 'test.bare-key', fetcher, persist: false })
        return () => h('div', resource.data.value ?? '')
      },
    })

    const wrapper = mount(Consumer)
    await flushPromises()

    expect(fetcher).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).toBe('bare')
  })

  it('keeps last-known data and sets error on a rejected refresh, moving loadState ready -> refreshing (not loading)', async () => {
    const first = deferred<string>()
    const fetcher = vi.fn().mockReturnValueOnce(first.promise)
    let resource: ReturnType<typeof useCachedResource<string>> | null = null

    const Consumer = defineComponent({
      setup() {
        resource = useCachedResource<string>({ key: 'test.error-key', fetcher, ttlMs: 1000, persist: false })
        return () => h('div', resource!.data.value ?? '')
      },
    })

    mount(Consumer)
    await Promise.resolve()
    first.resolve('v1')
    await flushPromises()

    expect(resource!.data.value).toBe('v1')
    expect(resource!.loadState.value).toBe('ready')

    const second = deferred<string>()
    fetcher.mockReturnValueOnce(second.promise)
    const refreshCall = resource!.refresh()
    await Promise.resolve()
    // Sticky-ready: a refresh on already-'ready' data moves to 'refreshing',
    // never back to 'loading' — content stays on screen while it runs.
    expect(resource!.loadState.value).toBe('refreshing')

    second.reject(new Error('offline'))
    await refreshCall
    await flushPromises()

    expect(resource!.data.value).toBe('v1') // keep-last-known-value
    expect(resource!.error.value).toBe('offline')
  })

  // 02-04: found while auditing Cloud.vue/Server.vue's lazy (`immediate:
  // false`) resources ahead of adding their routes to KEEP_ALIVE_PATHS.
  // Without this guard, onActivated's refreshIfStale() would treat a
  // never-fetched entry as stale and eagerly fire the "fetch on first use"
  // resource the moment the tab is first activated, even though the caller
  // never explicitly requested it (e.g. a tab-gated Paid Files fetch that
  // should wait until that sub-tab is opened).
  it('does not eagerly fetch an immediate:false resource on activation before it has been explicitly requested, but does revalidate it once it has', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date(2030, 0, 1, 0, 0, 0))
    const fetcher = vi.fn().mockResolvedValue('lazy-v1')
    let resource: ReturnType<typeof useCachedResource<string>> | null = null

    const Consumer = defineComponent({
      setup() {
        resource = useCachedResource<string>({
          key: 'test.lazy-key',
          fetcher,
          ttlMs: 1000,
          persist: false,
          immediate: false,
        })
        return () => h('div', resource!.data.value ?? '')
      },
    })
    const Other = defineComponent({ render: () => h('div', 'other') })

    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(Consumer, { key: 'consumer' }) : h(Other, { key: 'other' }),
        )
      },
    })

    const wrapper = mount(Host)
    await flushPromises()
    expect(fetcher).not.toHaveBeenCalled() // immediate: false — not fetched on mount

    // Deactivate then reactivate — still never explicitly requested, so
    // activation must not be the thing that fetches it.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(fetcher).not.toHaveBeenCalled()

    // The caller explicitly requests it now (e.g. the user opened the tab).
    await resource!.refresh()
    expect(fetcher).toHaveBeenCalledTimes(1)

    // Deactivate within the TTL, reactivate — no additional fetch.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(fetcher).toHaveBeenCalledTimes(1)

    // Deactivate, advance past the TTL, reactivate — now it revalidates,
    // because it has been fetched before.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    vi.setSystemTime(new Date(2030, 0, 1, 0, 0, 2)) // +2s, past the 1s TTL
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(fetcher).toHaveBeenCalledTimes(2)
  })
})
