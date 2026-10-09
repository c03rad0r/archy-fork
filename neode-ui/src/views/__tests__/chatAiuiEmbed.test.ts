// 02-07: the AIUI embed URL must be byte-stable across re-renders, a
// simulated viewport resize, and a KeepAlive deactivate/reactivate cycle —
// any runtime-varying input would change the iframe `src` and force a full
// AIUI reload on the next tab switch, giving back the entire benefit 02-04
// established for the Chat tab. Also covers the two D-14 presentation
// flags (chatExpanded, mobileChat), origin validation being unchanged, and
// aiuiConnected surviving deactivation (AIUI's 'ready' message is not
// re-sent on re-entry).
import { flushPromises, mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Chat from '../Chat.vue'

const providerSync = vi.hoisted(() => vi.fn(async () => {}))
vi.mock('@/components/AIConnectionModal.vue', () => ({ default: { template: '<div />', methods: { checkNeeded: async () => false, syncSelection: providerSync } } }))

const routerBackMock = vi.fn()
const routerPushMock = vi.fn()
const routerReplaceMock = vi.fn()

// Chat reads route.query.ask/askedAt to receive a ⌘K "Talk to AIUI about it"
// handoff, and route.path when it strips those params back off. Kept empty by
// default so the byte-stability assertions below see no ask in play.
const routeMock = { path: '/dashboard/chat', query: {} as Record<string, string> }

vi.mock('vue-router', () => ({
  useRouter: () => ({ back: routerBackMock, push: routerPushMock, replace: routerReplaceMock }),
  useRoute: () => routeMock,
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

// IS_DEMO is a build-time constant in the real module; mock it so these
// tests exercise the plain VITE_AIUI_URL branch deterministically.
vi.mock('@/composables/useDemoIntro', () => ({ IS_DEMO: false }))

// ContextBroker pulls in several Pinia stores (app/container/aiPermissions)
// unrelated to this test's concern (URL stability + origin validation) —
// mocked at the module boundary, mirroring MarketplaceRefresh.test.ts's
// convention for isolating a view from its heavier dependencies.
vi.mock('@/services/contextBroker', () => ({
  ContextBroker: class {
    start = vi.fn()
    stop = vi.fn()
  },
}))

/** Mount Chat.vue behind a real <KeepAlive> so onActivated/onDeactivated fire. */
function mountChatInKeepAlive() {
  const show = ref(true)
  const Host = defineComponent({
    setup() {
      return () => h(KeepAlive, null, {
        default: () => (show.value ? h(Chat) : h('div', 'other-tab')),
      })
    },
  })
  const wrapper = mount(Host)
  return { wrapper, show }
}

function iframeSrc(wrapper: ReturnType<typeof mount>): string | undefined {
  return wrapper.find('iframe').attributes('src')
}

describe('Chat / AIUI embed URL stability + D-14 defaults (02-07)', () => {
  beforeEach(() => {
    providerSync.mockClear()
    vi.stubEnv('VITE_AIUI_URL', 'http://localhost:5173')
  })

  afterEach(() => {
    vi.unstubAllEnvs()
    routeMock.query = {}
    routerReplaceMock.mockClear()
  })

  it('carries embedded=true, hideClose=true, and both D-14 flags', () => {
    const { wrapper } = mountChatInKeepAlive()
    const src = iframeSrc(wrapper)
    expect(src).toBeTruthy()
    expect(src).toContain('embedded=true')
    expect(src).toContain('hideClose=true')
    expect(src).toContain('chatExpanded=true')
    expect(src).toContain('mobileChat=true')
    wrapper.unmount()
  })

  // ⌘K → "Talk to AIUI about it" hands the typed text to AIUI. It must travel
  // by postMessage: putting it in the URL would give aiuiUrl a reactive
  // dependency and reload AIUI on every question, which is precisely the
  // byte-stability property the rest of this file exists to protect.
  it('delivers a ⌘K ask by postMessage on ready, leaving the iframe src untouched', async () => {
    routeMock.query = { ask: 'why is bitcoin syncing slowly', askedAt: '111' }
    const { wrapper } = mountChatInKeepAlive()
    const before = iframeSrc(wrapper)
    expect(before).not.toContain('ask=')

    const frame = wrapper.find('iframe').element as HTMLIFrameElement
    const post = vi.fn()
    Object.defineProperty(frame, 'contentWindow', { configurable: true, value: { postMessage: post } })

    window.dispatchEvent(new MessageEvent('message', {
      source: (wrapper.find('iframe').element as HTMLIFrameElement).contentWindow,
      origin: 'http://localhost:5173',
      data: { type: 'ready' },
    }))
    await flushPromises()

    expect(post).toHaveBeenCalledWith(
      { type: 'chat:prefill', text: 'why is bitcoin syncing slowly' },
      'http://localhost:5173',
    )
    // src must be byte-identical after the ask round-trip
    expect(iframeSrc(wrapper)).toBe(before)
    // and the params are stripped so a refresh does not silently re-ask
    expect(routerReplaceMock).toHaveBeenCalled()
    const replaceArg = routerReplaceMock.mock.calls[0]![0]
    expect(replaceArg.query.ask).toBeUndefined()
    expect(replaceArg.query.askedAt).toBeUndefined()
    wrapper.unmount()
  })

  it('does not post a prefill when there is no ask in the route', async () => {
    const { wrapper } = mountChatInKeepAlive()
    const frame = wrapper.find('iframe').element as HTMLIFrameElement
    const post = vi.fn()
    Object.defineProperty(frame, 'contentWindow', { configurable: true, value: { postMessage: post } })

    window.dispatchEvent(new MessageEvent('message', {
      source: (wrapper.find('iframe').element as HTMLIFrameElement).contentWindow,
      origin: 'http://localhost:5173',
      data: { type: 'ready' },
    }))
    await flushPromises()

    expect(post).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('is string-equal before and after a simulated viewport resize across the mobile breakpoint', async () => {
    const { wrapper } = mountChatInKeepAlive()
    const before = iframeSrc(wrapper)

    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 375 })
    window.dispatchEvent(new Event('resize'))
    await wrapper.vm.$nextTick()

    const after = iframeSrc(wrapper)
    expect(after).toBe(before)
    wrapper.unmount()
  })

  it('is string-equal before and after a deactivate/reactivate cycle', async () => {
    const { wrapper, show } = mountChatInKeepAlive()
    const before = iframeSrc(wrapper)

    show.value = false
    await wrapper.vm.$nextTick()
    show.value = true
    await wrapper.vm.$nextTick()

    const after = iframeSrc(wrapper)
    expect(after).toBe(before)
    wrapper.unmount()
  })

  it('does not set aiuiConnected for a message from a foreign origin', async () => {
    const { wrapper } = mountChatInKeepAlive()

    window.dispatchEvent(new MessageEvent('message', {
      source: (wrapper.find('iframe').element as HTMLIFrameElement).contentWindow,
      data: { type: 'ready' },
      origin: 'http://evil.example',
    }))
    await flushPromises()

    // aiuiConnected stays false: the loading overlay is still shown and the
    // connected indicator (title="chat.aiuiConnected") is absent.
    expect(wrapper.find('.chat-loading').exists()).toBe(true)
    expect(wrapper.find('[title="chat.aiuiConnected"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('ignores ready and setup messages from a different same-origin frame', async () => {
    const { wrapper } = mountChatInKeepAlive()
    for (const type of ['ready', 'ai:setup-request']) window.dispatchEvent(new MessageEvent('message', {
      source: window, origin: 'http://localhost:5173', data: { type },
    }))
    await flushPromises()
    expect(wrapper.find('.chat-loading').exists()).toBe(true)
    expect(wrapper.find('[title="chat.aiuiConnected"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('aiuiConnected survives a deactivate/reactivate cycle once set by a same-origin ready message', async () => {
    const { wrapper, show } = mountChatInKeepAlive()

    window.dispatchEvent(new MessageEvent('message', {
      source: (wrapper.find('iframe').element as HTMLIFrameElement).contentWindow,
      data: { type: 'ready' },
      origin: 'http://localhost:5173',
    }))
    await flushPromises()

    expect(wrapper.find('[title="chat.aiuiConnected"]').exists()).toBe(true)
    expect(wrapper.find('.chat-loading').exists()).toBe(false)

    expect(providerSync).toHaveBeenCalledTimes(1)
    show.value = false
    await wrapper.vm.$nextTick()
    show.value = true
    await wrapper.vm.$nextTick()
    await flushPromises()

    expect(providerSync).toHaveBeenCalledTimes(2)
    // No second 'ready' message is sent on reactivation — aiuiConnected must
    // not have been reset to false by the deactivate/reactivate cycle.
    expect(wrapper.find('[title="chat.aiuiConnected"]').exists()).toBe(true)
    expect(wrapper.find('.chat-loading').exists()).toBe(false)
    wrapper.unmount()
  })

  // Belt-and-suspenders backstop added after live testing found the overlay
  // could wedge the UI when the 'ready' handshake never arrives (a real bug,
  // separately fixed at its root cause in AIUI's archyBridge.ts) — this
  // proves the archy side never depends on that fix alone.
  it('dismisses the loading overlay after a bounded timeout even if no ready message ever arrives', async () => {
    vi.useFakeTimers()
    try {
      const { wrapper } = mountChatInKeepAlive()
      expect(wrapper.find('.chat-loading').exists()).toBe(true)

      await vi.advanceTimersByTimeAsync(7999)
      expect(wrapper.find('.chat-loading').exists()).toBe(true)

      await vi.advanceTimersByTimeAsync(1)
      expect(wrapper.find('.chat-loading').exists()).toBe(false)
      // The connection indicator must NOT falsely report connected — the
      // timeout only dismisses the blocking overlay, it does not fabricate
      // a successful handshake.
      expect(wrapper.find('[title="chat.aiuiConnected"]').exists()).toBe(false)

      wrapper.unmount()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not dismiss the loading overlay before the timeout elapses', async () => {
    vi.useFakeTimers()
    try {
      const { wrapper } = mountChatInKeepAlive()
      await vi.advanceTimersByTimeAsync(4000)
      expect(wrapper.find('.chat-loading').exists()).toBe(true)
      wrapper.unmount()
    } finally {
      vi.useRealTimers()
    }
  })
})
