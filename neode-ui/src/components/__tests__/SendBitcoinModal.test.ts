import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import SendBitcoinModal from '../SendBitcoinModal.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (s: string) => s }) }))
vi.mock('@/composables/useLightningRequired', () => ({ useLightningRequired: () => ({}) }))

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'lnd.getinfo') return { balance_sats: 100000 } as never
    if (method === 'lnd.estimatefee') return { fee_sat: 500, sat_per_vbyte: 3 } as never
    return { txid: 'a'.repeat(64) } as never
  })
})
function open() {
  const wrapper = mount(SendBitcoinModal, { props: { show: true }, global: { stubs: { BaseModal: { template: '<div><slot /></div>' } } } })
  const vm = (wrapper.vm as any).$.setupState
  vm.sendMethod = 'onchain'; vm.dest = 'bc1qtestaddress'; vm.amount = 1000
  return { wrapper, vm }
}
describe('on-chain fee defaults', () => {
  it.each([NaN, Infinity, 0.5, -1, 5001])('rejects invalid custom rate %s before preview or submission', async rate => {
    const { wrapper, vm } = open()
    vm.feePreset = 'custom'; vm.customSatPerVbyte = rate
    await vm.review()
    expect(vm.confirming).toBe(false)
    expect(vm.error).toContain('whole number')
    expect(rpcClient.call).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it.each([['fast', 1], ['standard', 6], ['slow', 144]])('uses %s consistently in preview and submission', async (preset, target) => {
    const { wrapper, vm } = open()
    expect(vm.feePreset).toBe('fast')
    vm.feePreset = preset
    await vm.review(); await flushPromises()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.estimatefee', params: expect.objectContaining({ target_conf: target }) }))
    await vm.send()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.sendcoins', params: expect.objectContaining({ target_conf: target }) }))
    wrapper.unmount()
  })
  it('preserves an explicit custom rate and resets reopened/new sends to Fast', async () => {
    const { wrapper, vm } = open()
    vm.feePreset = 'custom'; vm.customSatPerVbyte = 17
    await vm.review(); await flushPromises(); await vm.send()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.sendcoins', params: expect.objectContaining({ sat_per_vbyte: 17 }) }))
    expect(rpcClient.call).not.toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.estimatefee' }))
    vm.sendAnother(); expect(vm.feePreset).toBe('fast'); expect(vm.customSatPerVbyte).toBeNull()
    vm.feePreset = 'slow'
    await wrapper.setProps({ show: false }); await wrapper.setProps({ show: true })
    expect(vm.feePreset).toBe('fast'); expect(vm.resolvedFeeParams).toEqual({})
    wrapper.unmount()
  })
})
