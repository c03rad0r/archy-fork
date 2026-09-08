import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import ReceiveBitcoinModal from '../ReceiveBitcoinModal.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-router', () => ({
  useRoute: () => ({ fullPath: '/dashboard' }),
  useRouter: () => ({ push: vi.fn() }),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string, params?: Record<string, unknown>) => (params ? `${key}:${JSON.stringify(params)}` : key) }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

vi.mock('@/composables/useLightningRequired', () => ({
  useLightningRequired: () => ({
    requireLightningReady: vi.fn().mockResolvedValue(true),
    handleLightningFailure: vi.fn().mockReturnValue(false),
  }),
}))

// Guards an operator report (2026-09-08): clicking the Ecash tab appeared to
// close the whole Receive modal. Not reproduced here — the tab switch alone
// (success or failure of wallet.ecash-lnaddress) never emits `close` or
// unmounts the dialog — but the RPC-eager tab switch is exactly the kind of
// path a future change could regress, so it's worth pinning down.
describe('ReceiveBitcoinModal — ecash tab click', () => {
  it('does not close/emit when the ecash tab is clicked and the RPC succeeds', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ address: 'someone@minibits.cash' } as never)

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
    })
    await flushPromises()

    const tabs = Array.from(document.body.querySelectorAll('button'))
    const ecashTab = tabs.find((b) => b.textContent?.toLowerCase().includes('ecash'))
    expect(ecashTab).toBeTruthy()

    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()

    expect(wrapper.emitted('close')).toBeFalsy()
    expect(document.body.querySelector('[role="dialog"]')).toBeTruthy()
    wrapper.unmount()
  })

  it('does not close/emit when the ecash tab is clicked and the RPC fails', async () => {
    vi.mocked(rpcClient.call).mockRejectedValue(new Error('boom'))

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
    })
    await flushPromises()

    const tabs = Array.from(document.body.querySelectorAll('button'))
    const ecashTab = tabs.find((b) => b.textContent?.toLowerCase().includes('ecash'))
    expect(ecashTab).toBeTruthy()

    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()

    expect(wrapper.emitted('close')).toBeFalsy()
    expect(document.body.querySelector('[role="dialog"]')).toBeTruthy()
    wrapper.unmount()
  })
})
