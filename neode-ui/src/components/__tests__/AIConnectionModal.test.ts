import { mount, flushPromises } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import AIConnectionModal from '../AIConnectionModal.vue'
import ReceiveBitcoinModal from '../ReceiveBitcoinModal.vue'
import WalletScanModal from '../WalletScanModal.vue'
import { rpcClient } from '@/api/rpc-client'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('../WalletScanModal.vue', () => ({ default: { props: ['show'], template: '<div />' } }))
vi.mock('../ReceiveBitcoinModal.vue', () => ({ default: { props: ['show', 'initialMethod', 'manageHistory'], template: '<div />' } }))
vi.mock('@/views/settings/RoutstrBudgetSection.vue', () => ({ default: { template: '<div />' } }))
const state = () => ({ schema: 1, settings: { provider: 'auto', openai_model: '' }, claude_configured: false, openai_configured: false, local_ready: false, routstr_remaining_sats: 0 })
function mountModal() { return mount(AIConnectionModal, { props: { show: false }, global: { stubs: { BaseModal: { props: ['show'], template: '<div v-if="show"><slot /></div>' } } } }) }
function button(w: ReturnType<typeof mountModal>, label: string) { return w.findAll('button').find(b => b.text() === label)! }
beforeEach(() => { vi.clearAllMocks(); vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'system.settings.get' ? { value: state() } : {}) })
describe('AI connection setup', () => {
  it('opens on Routstr with a top-up action without authorizing spending', async () => {
    const w = mountModal(); await w.setProps({ show: true }); await flushPromises()
    expect(button(w, 'Routstr · sats').attributes('aria-pressed')).toBe('true')
    expect(button(w, 'Top up with ecash').exists()).toBe(true)
    expect(vi.mocked(rpcClient.call).mock.calls.every(([r]) => !['assistant.budget-set', 'system.settings.set'].includes(r.method))).toBe(true)
    await button(w, 'Top up with ecash').trigger('click')
    expect(w.findComponent(ReceiveBitcoinModal).props()).toMatchObject({ show: true, initialMethod: 'ecash', manageHistory: false })
    w.findComponent(ReceiveBitcoinModal).vm.$emit('scan'); await flushPromises()
    expect(w.findComponent(WalletScanModal).props('show')).toBe(true)
    expect(w.findComponent(ReceiveBitcoinModal).props('show')).toBe(false)
    w.findComponent(WalletScanModal).vm.$emit('close'); await flushPromises()
    expect(w.findComponent(ReceiveBitcoinModal).props('show')).toBe(true)
    w.unmount()
  })

  it('detects absent configuration without treating a failed status query as missing keys', async () => {
    const w = mountModal()
    expect(await (w.vm as any).checkNeeded()).toBe(true)
    vi.mocked(rpcClient.call).mockRejectedValueOnce(new Error('offline'))
    expect(await (w.vm as any).checkNeeded()).toBe(false)
    expect(rpcClient.call).not.toHaveBeenCalledWith(expect.objectContaining({ method: 'system.settings.set' }))
    w.unmount()
  })
  it('sends a key only to private settings, clears the field, and selects the persisted model', async () => {
    const w = mountModal(); await w.setProps({ show: true }); await flushPromises()
    await button(w, 'OpenAI API').trigger('click')
    await w.get('#ai-connection-key').setValue('test-private-key')
    await w.get('#ai-connection-model').setValue('test-chat-model')
    await w.get('form').trigger('submit'); await flushPromises()
    const writes = vi.mocked(rpcClient.call).mock.calls.map(([r]) => r).filter(r => r.method === 'system.settings.set')
    expect(writes.map(r => r.params)).toEqual([{ key: 'openai_api_key', value: 'test-private-key' }, { key: 'ai_provider', value: JSON.stringify({ provider: 'openai', openai_model: 'test-chat-model' }) }])
    expect((w.get('#ai-connection-key').element as HTMLInputElement).value).toBe('')
    expect(w.emitted('configured')).toEqual([['openai', 'test-chat-model']])
    expect(JSON.stringify(w.emitted())).not.toContain('test-private-key')
    w.unmount()
  })
  it('clears unsaved keys when switching provider and closing', async () => {
    const w = mountModal(); await w.setProps({ show: true }); await flushPromises()
    await button(w, 'Claude API').trigger('click'); await w.get('#ai-connection-key').setValue('unsaved')
    await button(w, 'OpenAI API').trigger('click')
    expect((w.get('#ai-connection-key').element as HTMLInputElement).value).toBe('')
    await w.get('#ai-connection-key').setValue('unsaved-again'); await w.setProps({ show: false }); await w.setProps({ show: true })
    expect((w.get('#ai-connection-key').element as HTMLInputElement).value).toBe('')
    expect(w.emitted('configured')).toBeUndefined(); w.unmount()
  })
  it('does not reuse an OpenAI model ID when restoring a Routstr connection', async () => {
    const value = { ...state(), settings: { provider: 'routstr', openai_model: 'previous-openai-model' } }
    vi.mocked(rpcClient.call).mockResolvedValue({ value })
    const w = mountModal(); await (w.vm as any).syncSelection()
    expect(w.emitted('configured')).toEqual([['routstr', undefined]])
    w.unmount()
  })

  it('does not authorize Routstr spending from setup when allowance is zero', async () => {
    const w = mountModal(); await w.setProps({ show: true }); await flushPromises()
    await button(w, 'Routstr · sats').trigger('click'); await flushPromises()
    await button(w, 'Use Routstr').trigger('click'); await flushPromises()
    expect(w.text()).toContain('Set a spending allowance')
    expect(w.emitted('configured')).toBeUndefined()
    expect(vi.mocked(rpcClient.call).mock.calls.every(([r]) => !['assistant.budget-set', 'system.settings.set'].includes(r.method))).toBe(true)
    w.unmount()
  })
})
