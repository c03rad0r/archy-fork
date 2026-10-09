import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import OnboardingSeedGenerate from '../OnboardingSeedGenerate.vue'
import { rpcClient } from '@/api/rpc-client'

const WORDS = Array.from({ length: 24 }, (_, i) => `word${i + 1}`)

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn(() => Promise.resolve()) }),
}))

vi.mock('@/composables/useNavSounds', () => ({
  playNavSound: vi.fn(),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

// Set the scroll region's and confirmation label's geometry directly — jsdom
// has no layout engine, so scrollHeight/clientHeight/scrollTop are normally 0
// and getBoundingClientRect() always returns an all-zero rect, so both must
// be driven explicitly.
function setGeometry(
  container: HTMLElement,
  label: HTMLElement,
  opts: { scrollHeight: number; clientHeight: number; scrollTop: number; containerBottom: number; labelBottom: number },
) {
  Object.defineProperty(container, 'scrollHeight', { value: opts.scrollHeight, writable: true, configurable: true })
  Object.defineProperty(container, 'clientHeight', { value: opts.clientHeight, writable: true, configurable: true })
  Object.defineProperty(container, 'scrollTop', { value: opts.scrollTop, writable: true, configurable: true })
  container.getBoundingClientRect = () => ({ bottom: opts.containerBottom } as DOMRect)
  label.getBoundingClientRect = () => ({ bottom: opts.labelBottom } as DOMRect)
}

describe('OnboardingSeedGenerate scroll cue (UIFIX-03)', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', class {
      observe = vi.fn()
      disconnect = vi.fn()
    })
    vi.mocked(rpcClient.call).mockReset()
    Element.prototype.scrollIntoView = vi.fn()
  })

  async function mountWithWords() {
    vi.mocked(rpcClient.call).mockResolvedValue({ words: WORDS })
    const wrapper = mount(OnboardingSeedGenerate)
    await flushPromises()
    await wrapper.vm.$nextTick()
    return wrapper
  }

  it('renders no cue when the scroll region reports no overflow', async () => {
    const wrapper = await mountWithWords()
    const container = wrapper.get('.overflow-y-auto').element as HTMLElement
    const label = wrapper.get('label').element as HTMLElement

    setGeometry(container, label, {
      scrollHeight: 400,
      clientHeight: 400,
      scrollTop: 0,
      containerBottom: 400,
      labelBottom: 350,
    })
    await container.dispatchEvent(new Event('scroll'))
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).not.toContain('One more step below')
  })

  it('renders the cue when there is overflow and the tickbox is below the fold', async () => {
    const wrapper = await mountWithWords()
    const container = wrapper.get('.overflow-y-auto').element as HTMLElement
    const label = wrapper.get('label').element as HTMLElement

    setGeometry(container, label, {
      scrollHeight: 800,
      clientHeight: 400,
      scrollTop: 0,
      containerBottom: 400,
      labelBottom: 750,
    })
    await container.dispatchEvent(new Event('scroll'))
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('One more step below')
  })

  it('removes the cue once scrolling brings the tickbox into view', async () => {
    const wrapper = await mountWithWords()
    const container = wrapper.get('.overflow-y-auto').element as HTMLElement
    const label = wrapper.get('label').element as HTMLElement

    setGeometry(container, label, {
      scrollHeight: 800,
      clientHeight: 400,
      scrollTop: 0,
      containerBottom: 400,
      labelBottom: 750,
    })
    await container.dispatchEvent(new Event('scroll'))
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain('One more step below')

    // Scroll down: the container's own viewport rect doesn't move, but its
    // scrolled content does — the label's viewport-relative bottom shifts up
    // by the scroll delta, bringing it inside the visible window.
    setGeometry(container, label, {
      scrollHeight: 800,
      clientHeight: 400,
      scrollTop: 400,
      containerBottom: 400,
      labelBottom: 350,
    })
    await container.dispatchEvent(new Event('scroll'))
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).not.toContain('One more step below')
  })

  it('activating the cue scrolls the tickbox into view and never touches confirmed', async () => {
    const wrapper = await mountWithWords()
    const container = wrapper.get('.overflow-y-auto').element as HTMLElement
    const label = wrapper.get('label').element as HTMLElement

    setGeometry(container, label, {
      scrollHeight: 800,
      clientHeight: 400,
      scrollTop: 0,
      containerBottom: 400,
      labelBottom: 750,
    })
    await container.dispatchEvent(new Event('scroll'))
    await wrapper.vm.$nextTick()

    const cueButton = wrapper.findAll('button').find((b) => b.text().includes('One more step below'))
    expect(cueButton).toBeDefined()

    await cueButton!.trigger('click')

    expect(label.scrollIntoView).toHaveBeenCalledWith({ behavior: 'smooth', block: 'center' })
    const checkbox = wrapper.get('input[type="checkbox"]').element as HTMLInputElement
    expect(checkbox.checked).toBe(false)
  })

  it('never shows the cue while loading, regardless of overflow', async () => {
    let resolveCall: (v: { words: string[] }) => void = () => {}
    vi.mocked(rpcClient.call).mockReturnValue(new Promise((resolve) => { resolveCall = resolve }))
    const wrapper = mount(OnboardingSeedGenerate)
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).not.toContain('One more step below')
    expect(wrapper.text()).toContain('Generating your seed phrase')

    resolveCall({ words: WORDS })
    await flushPromises()
  })

  it('removes the cue once the tickbox is ticked', async () => {
    const wrapper = await mountWithWords()
    const container = wrapper.get('.overflow-y-auto').element as HTMLElement
    const label = wrapper.get('label').element as HTMLElement

    setGeometry(container, label, {
      scrollHeight: 800,
      clientHeight: 400,
      scrollTop: 0,
      containerBottom: 400,
      labelBottom: 750,
    })
    await container.dispatchEvent(new Event('scroll'))
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain('One more step below')

    await wrapper.get('input[type="checkbox"]').setValue(true)
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).not.toContain('One more step below')
  })
})
