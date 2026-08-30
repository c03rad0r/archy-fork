import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import { flushPromises, mount } from '@vue/test-utils'

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn() }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

import { rpcClient } from '@/api/rpc-client'
import OpenWrtGateway from './OpenWrtGateway.vue'

describe('OpenWrtGateway stale cached router recovery (#103)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    sessionStorage.clear()
  })

  it('offers reconfiguration when the saved router can no longer connect', async () => {
    vi.mocked(rpcClient.call).mockRejectedValue(new Error('Connection timed out'))
    const wrapper = mount(OpenWrtGateway, {
      global: { plugins: [createPinia()] },
    })
    await flushPromises()

    const reconfigure = wrapper.findAll('button').find(button => button.text() === 'Reconfigure router')
    expect(reconfigure).toBeDefined()
    await reconfigure!.trigger('click')

    expect(wrapper.text()).toContain('Connect to Router')
    expect(wrapper.text()).not.toContain('Connection timed out')
    wrapper.unmount()
  })
})
