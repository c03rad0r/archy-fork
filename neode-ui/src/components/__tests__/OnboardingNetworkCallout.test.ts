import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import OnboardingNetworkCallout, { hasPhysicalLink } from '../OnboardingNetworkCallout.vue'
import { rpcClient } from '@/api/rpc-client'

// #145: a fresh install with no cable strands the user — the callout points
// at the WiFi picker, and ONLY when no physical link exists. Archipelago is
// offline-first, so "no internet" must never nag: only "no link at all".

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

const push = vi.fn()
vi.mock('vue-router', () => ({
  useRouter: () => ({ push }),
}))

const call = vi.mocked(rpcClient.call)

function mountCallout() {
  return mount(OnboardingNetworkCallout)
}

afterEach(() => {
  vi.clearAllMocks()
})

describe('hasPhysicalLink (pure decision)', () => {
  it('no interfaces at all → no link', () => {
    expect(hasPhysicalLink([])).toBe(false)
  })

  it('ethernet up → link', () => {
    expect(hasPhysicalLink([{ type: 'ethernet', state: 'up' }])).toBe(true)
  })

  it('wifi up → link', () => {
    expect(hasPhysicalLink([{ type: 'wifi', state: 'up' }])).toBe(true)
  })

  it('physical interface present but down → no link', () => {
    expect(
      hasPhysicalLink([
        { type: 'ethernet', state: 'down' },
        { type: 'wifi', state: 'down' },
      ]),
    ).toBe(false)
  })

  it('virtual interfaces that happen to be up do NOT count as a link', () => {
    expect(
      hasPhysicalLink([
        { type: 'bridge', state: 'up' },
        { type: 'loopback', state: 'up' },
      ]),
    ).toBe(false)
  })
})

describe('OnboardingNetworkCallout (component)', () => {
  beforeEach(() => {
    call.mockReset()
  })

  it('shows when the node has no physical link, and offers the WiFi picker', async () => {
    call.mockResolvedValue({
      interfaces: [
        { type: 'ethernet', state: 'down' },
        { type: 'wifi', state: 'down' },
      ],
    })
    const wrapper = mountCallout()
    await flushPromises()

    expect(wrapper.text()).toContain('No network connection')
    expect(wrapper.text()).toContain('Connect to WiFi')

    await wrapper.findAll('button').find(b => b.text() === 'Connect to WiFi')!.trigger('click')
    expect(push).toHaveBeenCalledWith('/dashboard/server?open=wifi')
  })

  it('stays hidden once any physical link exists — offline-first, no nagging', async () => {
    call.mockResolvedValue({ interfaces: [{ type: 'ethernet', state: 'up' }] })
    const wrapper = mountCallout()
    await flushPromises()

    expect(wrapper.find('div.fixed').exists()).toBe(false)
  })

  it('never shows on a failed probe — early onboarding, RPC not ready yet', async () => {
    call.mockRejectedValue(new Error('not ready'))
    const wrapper = mountCallout()
    await flushPromises()

    expect(wrapper.find('div.fixed').exists()).toBe(false)
  })

  it('hides when dismissed, even with no link', async () => {
    call.mockResolvedValue({ interfaces: [{ type: 'wifi', state: 'down' }] })
    const wrapper = mountCallout()
    await flushPromises()

    const dismiss = wrapper.findAll('button').find(b => b.text() === 'Dismiss')!
    expect(dismiss).toBeDefined()
    await dismiss.trigger('click')
    expect(wrapper.find('div.fixed').exists()).toBe(false)
  })
})
