import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import BumpFeeModal from '../BumpFeeModal.vue'
import TransactionsModal from '../TransactionsModal.vue'
import { rpcClient } from '@/api/rpc-client'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
const explorer = vi.hoisted(() => vi.fn())
vi.mock('@/composables/useTxExplorer', () => ({ useTxExplorer: () => ({ openTx: explorer }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (s: string) => s }) }))
const quote = { quote_id: 'q', txid: 'a'.repeat(64), expires_at: Math.floor(Date.now() / 1000) + 60, method: 'cpfp', recipient_sats: 161650, current_fee_sats: 144, additional_fee_sats: 618, total_fee_sats: 762, budget_sats: 618, rate_sat_vb: 3 }
const stubs = { BaseModal: { template: '<div><slot /></div>' } }
function open() {
  const wrapper = mount(BumpFeeModal, { props: { show: true, txid: quote.txid }, global: { stubs } })
  return { wrapper, vm: (wrapper.vm as any).$.setupState }
}
beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'lnd.bump-status') return { status: 'none' } as never
    return { ...quote } as never
  })
})
describe('Bump review', () => {
  it('defaults to next block and clearly explains CPFP with additional/total preview', async () => {
    const { wrapper } = open(); await flushPromises()
    expect(wrapper.text()).toContain('CPFP · Spend change')
    expect(wrapper.text()).toContain('RBF is unavailable')
    expect(wrapper.text()).toContain('618 sats')
    expect(wrapper.text()).toContain('762 sats')
    expect(rpcClient.call).not.toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.bump-submit' }))
    wrapper.unmount()
  })
  it('shows RBF only for a backend-supported sweep', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => (method === 'lnd.bump-status' ? { status: 'none' } : { ...quote, method: 'rbf' }) as never)
    const { wrapper } = open(); await flushPromises()
    expect(wrapper.text()).toContain('RBF · Replace sweep')
    expect(wrapper.text()).not.toContain('RBF is unavailable')
    wrapper.unmount()
  })
  it('invalidates the old quote when custom settings change', async () => {
    const { wrapper, vm } = open(); await flushPromises()
    vm.mode = 'custom'; await flushPromises()
    expect(vm.quote).toBeNull()
    vm.customRate = 0.5; await flushPromises(); await vm.getQuote()
    expect(vm.error).toContain('whole-number')
    vm.customRate = 10; await flushPromises(); await vm.getQuote()
    expect(rpcClient.call).toHaveBeenLastCalledWith(expect.objectContaining({ method: 'lnd.bump-quote', params: { txid: quote.txid, sat_per_vbyte: 10 } }))
    wrapper.unmount()
  })
  it('blocks expired quotes and duplicate clicks; never retries a mutation', async () => {
    const { wrapper, vm } = open(); await flushPromises()
    vm.quote.expires_at = 1; await vm.submit()
    expect(rpcClient.call).not.toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.bump-submit' }))
    vm.quote.expires_at = Math.floor(Date.now()/1000) + 60
    let finish!: (value: any) => void
    vi.mocked(rpcClient.call).mockImplementation(({ method }) => method === 'lnd.bump-submit' ? new Promise(resolve => { finish = resolve }) as never : Promise.resolve({ status: 'registered', message: 'Waiting for broadcast' }) as never)
    const pending = vm.submit(); await vm.submit()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([r]) => r.method === 'lnd.bump-submit')).toHaveLength(1)
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'lnd.bump-submit', maxRetries: 1 }))
    finish({ status: 'registered', message: 'Waiting for broadcast' }); await pending
    expect(wrapper.text()).toContain('Waiting for broadcast')
    wrapper.unmount()
  })
  it('keeps unknown submission outcome from becoming a second payment', async () => {
    const { wrapper, vm } = open(); await flushPromises()
    vi.mocked(rpcClient.call).mockRejectedValue(new Error('Network timeout'))
    await vm.submit()
    expect(vm.operation.status).toBe('unknown')
    expect(wrapper.text()).not.toContain('Confirm bump')
    expect(wrapper.text()).toContain('Check status')
    wrapper.unmount()
  })
  it('reconciles a previous submission when reopening', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ status: 'mempool', message: 'Accepted, awaiting confirmation', bump_txid: 'b'.repeat(64), actual_sweep_fee_sats: 618 } as never)
    const { wrapper } = open(); await flushPromises()
    expect(wrapper.text()).toContain('BUMP BROADCAST')
    expect(wrapper.text()).toContain('Awaiting confirmation')
    expect(rpcClient.call).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).not.toContain('Confirm bump')
    wrapper.unmount()
  })
  it('uses the existing green success animation only after verified acceptance', async () => {
    const { wrapper, vm } = open(); await flushPromises()
    vm.operation = { status: 'registered', message: 'Waiting for broadcast' }; await flushPromises()
    expect(wrapper.find('.send-success-badge').exists()).toBe(false)
    vm.operation = { status: 'mempool', message: 'Accepted', bump_txid: 'b', quote: { method: 'rbf' } }; await flushPromises()
    expect(wrapper.find('.send-success-badge').exists()).toBe(true)
    expect(wrapper.text()).toContain('BUMP BROADCAST')
    expect(wrapper.text()).toContain('Awaiting confirmation')
    vm.operation.status = 'confirmed'; await flushPromises()
    expect(wrapper.text()).toContain('CONFIRMED')
    expect(wrapper.text()).not.toContain('Awaiting confirmation')
    wrapper.unmount()
  })
  it('offers Bump only on pending on-chain rows and stops explorer navigation', async () => {
    const tx = { tx_hash: quote.txid, amount_sats: 161794, direction: 'outgoing', num_confirmations: 0, time_stamp: 1, total_fees: 144, dest_addresses: [], label: '', block_height: 0 }
    const wrapper = mount(TransactionsModal, { props: { show: true, transactions: [tx, { ...tx, tx_hash: 'b', num_confirmations: 1 }, { ...tx, tx_hash: 'c', kind: 'lightning' }] as any }, global: { stubs: { ...stubs, BumpFeeModal: true } } })
    const buttons = wrapper.findAll('button').filter(b => b.text() === 'Bump')
    expect(buttons).toHaveLength(1)
    await buttons[0]!.trigger('click')
    expect(explorer).not.toHaveBeenCalled()
    expect(wrapper.findComponent({ name: 'BumpFeeModal' }).props('txid')).toBe(quote.txid)
    wrapper.unmount()
  })
})

describe('Grouped fee-bump history', () => {
  it('preserves recipient amount, counts only active extra fees, and opens the replacement bump', async () => {
    const parent = { tx_hash: 'a'.repeat(64), amount_sats: 100144, total_fees: 144, direction: 'outgoing' as const,
      num_confirmations: 0, time_stamp: 1, dest_addresses: [], label: '', block_height: 0,
      bump_fee_sats: 600, fee_bump_txid: 'c'.repeat(64), fee_bump_confirmations: 0,
      fee_bump_history: [{ tx_hash: 'b'.repeat(64), fee_sats: 300, status: 'replaced' },
        { tx_hash: 'c'.repeat(64), fee_sats: 600, status: 'mempool' }] }
    const wrapper = mount(TransactionsModal, { props: { show: true, transactions: [parent] }, global: { stubs } })
    expect(wrapper.text()).toContain('-100,000 sats')
    expect(wrapper.text()).toContain('fee 744 sats')
    expect(wrapper.text()).toContain('Replaced bump')
    await wrapper.get(`button[aria-label="Bump transaction ${parent.tx_hash}"]`).trigger('click')
    await flushPromises()
    expect(wrapper.findComponent(BumpFeeModal).props('txid')).toBe(parent.fee_bump_txid)
    wrapper.unmount()
  })
})
