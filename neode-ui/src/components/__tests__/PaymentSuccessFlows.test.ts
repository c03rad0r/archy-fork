import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import SendBitcoinModal from '../SendBitcoinModal.vue'
import ReceiveBitcoinModal from '../ReceiveBitcoinModal.vue'
import i18n from '@/i18n'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    payLightningInvoice: vi.fn(),
  },
}))

vi.mock('qrcode', () => ({
  toCanvas: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@/composables/useLightningRequired', () => ({
  useLightningRequired: () => ({
    requireLightningReady: vi.fn().mockResolvedValue(true),
    handleLightningFailure: vi.fn().mockReturnValue(false),
  }),
}))

afterEach(() => {
  vi.clearAllMocks()
  document.body.innerHTML = ''
})

describe('payment completion receipts', () => {
  it('replaces the Cashu send form and keeps the token copyable', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ token: 'cashuB-test-token' } as never)
    const wrapper = mount(SendBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
      global: { plugins: [i18n] },
    })
    const vm = wrapper.vm as unknown as {
      sendMethod: string
      amountEntry: number
      send: () => Promise<void>
    }
    vm.sendMethod = 'ecash'
    vm.amountEntry = 2100

    await vm.send()
    await flushPromises()

    expect(document.body.textContent).toContain('2,100')
    expect(document.body.textContent).toContain('SENT')
    expect(document.body.textContent).toContain('cashuB-test-token')
    expect(document.body.textContent).toContain('Token to share')
    expect(document.body.querySelector('textarea')).toBeNull()
    expect(document.body.querySelector('input[type="number"]')).toBeNull()
    wrapper.unmount()
  })

  it('replaces the ecash receive form and preserves the redeemed token and mint', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({
      received_sats: 42,
      kind: 'cashu',
      mint_url: 'https://mint.example',
    } as never)
    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
      global: { plugins: [i18n] },
    })
    const vm = wrapper.vm as unknown as {
      receiveMethod: string
      ecashToken: string
      receive: () => Promise<void>
    }
    vm.receiveMethod = 'ecash'
    vm.ecashToken = 'cashuB-redeemed-token'

    await vm.receive()
    await flushPromises()

    expect(document.body.textContent).toContain('42')
    expect(document.body.textContent).toContain('RECEIVED')
    expect(document.body.textContent).toContain('https://mint.example')
    expect(document.body.textContent).toContain('cashuB-redeemed-token')
    expect(document.body.querySelector('textarea')).toBeNull()
    wrapper.unmount()
  })
})
