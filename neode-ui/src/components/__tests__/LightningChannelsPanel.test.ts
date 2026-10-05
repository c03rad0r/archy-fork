import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import LightningChannelsPanel from '../LightningChannelsPanel.vue'
import { rpcClient } from '@/api/rpc-client'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('@/composables/useTxExplorer', () => ({ useTxExplorer: () => ({ openTx: vi.fn() }) }))
vi.mock('@/composables/useCachedResource', async () => {
  const { ref } = await import('vue')
  return { useCachedResource: () => ({ data: ref(null), loadState: ref('ready'), error: ref(null), refresh: vi.fn().mockResolvedValue(undefined) }) }
})
const channel = { chan_id: 'test', remote_pubkey: '02' + 'a'.repeat(64), channel_point: 'b'.repeat(64) + ':0', capacity: 100000, local_balance: 50000, remote_balance: 50000, active: true }
function open() {
  const wrapper = mount(LightningChannelsPanel, { global: { stubs: { Teleport: true } } })
  const vm = (wrapper.vm as any).$.setupState
  vm.confirmClose(channel)
  return { wrapper, vm }
}
beforeEach(() => { vi.clearAllMocks(); vi.mocked(rpcClient.call).mockResolvedValue({ success: true } as never) })
describe('channel closing fee choice', () => {
  it('defaults to next block, preserves explicit choices, and resets a new close to Fast', async () => {
    const { wrapper, vm } = open()
    expect(vm.closeForm.feePreset).toBe('fast')
    await vm.closeChannel()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ params: { channel_point: channel.channel_point, target_conf: 1 } }))
    vm.confirmClose(channel)
    vm.closeForm.feePreset = 'standard'
    await vm.closeChannel()
    expect(rpcClient.call).toHaveBeenLastCalledWith(expect.objectContaining({ params: { channel_point: channel.channel_point, target_conf: 6 } }))
    vm.confirmClose(channel)
    expect(vm.closeForm.feePreset).toBe('fast')
    expect(vm.openForm.feePreset).toBe('fast')
    wrapper.unmount()
  })
  it.each([['standard', 6], ['medium', 3], ['fast', 1]])('forwards %s target and never automatically retries the mutation', async (preset, target) => {
    const { wrapper, vm } = open(); vm.closeForm.feePreset = preset
    await vm.closeChannel()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.closechannel', params: { channel_point: channel.channel_point, target_conf: target }, maxRetries: 1 }))
    expect(vm.closeTarget).toBeNull(); wrapper.unmount()
  })
  it('sends the custom rate instead of a confirmation target', async () => {
    const { wrapper, vm } = open(); vm.closeForm.feePreset = 'custom'; vm.closeForm.customSatPerVbyte = 25; vm.closeForm.customConfTarget = 3
    await vm.closeChannel()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ params: { channel_point: channel.channel_point, sat_per_vbyte: 25 } }))
    wrapper.unmount()
  })
  it.each([0.5, -1, 5001, NaN, Infinity])('rejects invalid custom rate %s before RPC', async rate => {
    const { wrapper, vm } = open(); vm.closeForm.feePreset = 'custom'; vm.closeForm.customSatPerVbyte = rate
    await vm.closeChannel(); expect(rpcClient.call).not.toHaveBeenCalled(); expect(vm.closeError).toBeTruthy(); wrapper.unmount()
  })
  it('requires a custom value and accepts a custom confirmation target', async () => {
    const { wrapper, vm } = open(); vm.closeForm.feePreset = 'custom'
    await vm.closeChannel(); expect(rpcClient.call).not.toHaveBeenCalled()
    vm.closeForm.customConfTarget = 2; await vm.closeChannel()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ params: { channel_point: channel.channel_point, target_conf: 2 } })); wrapper.unmount()
  })
  it('keeps the chosen fee and error visible when LND rejects a close', async () => {
    vi.mocked(rpcClient.call).mockRejectedValue(new Error('Peer is offline'))
    const { wrapper, vm } = open(); vm.closeForm.feePreset = 'fast'
    await vm.closeChannel(); expect(vm.closeTarget).not.toBeNull(); expect(vm.closeError).toBe('Peer is offline'); expect(vm.closeForm.feePreset).toBe('fast'); wrapper.unmount()
  })
  it('prevents duplicate submits while a close is pending', async () => {
    let finish!: (value: unknown) => void
    vi.mocked(rpcClient.call).mockImplementation(() => new Promise(resolve => { finish = resolve }) as never)
    const { wrapper, vm } = open(); const pending = vm.closeChannel(); await vm.closeChannel()
    expect(rpcClient.call).toHaveBeenCalledTimes(1); finish({ success: true }); await pending; await flushPromises(); wrapper.unmount()
  })
})
