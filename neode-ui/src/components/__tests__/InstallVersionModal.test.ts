import { mount, flushPromises } from '@vue/test-utils'
import { describe, it, expect, vi } from 'vitest'
import { createI18n } from 'vue-i18n'
import InstallVersionModal from '../InstallVersionModal.vue'
const versions = vi.hoisted(() => vi.fn())
vi.mock('../../api/rpc-client', () => ({ rpcClient: { getPackageVersions: versions } }))
const i18n = createI18n({ legacy: false, locale: 'en', missingWarn: false, fallbackWarn: false, messages: { en: { common: { install: 'Install', cancel: 'Cancel' } } } })
function modal(id = 'bitcoin-core') {
  return mount(InstallVersionModal, {
    props: { show: true, appId: id, app: { id, title: id } },
    global: { plugins: [i18n], stubs: { BaseModal: { template: '<div><slot/><slot name="footer"/></div>' } } },
  })
}
describe('Bitcoin install storage choice', () => {
  it.each(['bitcoin-core', 'bitcoin-knots'])('sends chosen version and explicit pruning for %s', async id => {
    versions.mockResolvedValue({ bitcoinPrune: false, default: 'latest', versions: [{ version: 'latest' }, { version: '28.4' }] })
    const wrapper = modal(id)
    await flushPromises()
    await wrapper.get('select').setValue('28.4')
    await wrapper.get('input[type=checkbox]').setValue(true)
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([['28.4', true]])
    expect(wrapper.text()).toContain('automatic pruning')
    expect(wrapper.text()).toContain('Mempool')
  })
  it('keeps automatic disk selection by default and resets on reopening', async () => {
    versions.mockResolvedValue({ bitcoinPrune: false, versions: [{ version: 'latest' }] })
    const wrapper = modal()
    await flushPromises()
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([['latest', false]])
    await wrapper.get('input').setValue(true)
    await wrapper.setProps({ show: false })
    await wrapper.setProps({ show: true })
    await flushPromises()
    expect((wrapper.get('input').element as HTMLInputElement).checked).toBe(false)
  })
  it('still allows choosing pruning when version lookup fails', async () => {
    versions.mockRejectedValue(new Error('offline'))
    const wrapper = modal()
    await flushPromises()
    await wrapper.get('input').setValue(true)
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([['latest', true]])
  })
  it('remembers the node pruning preference when reinstalling or switching Bitcoin variants', async () => {
    versions.mockResolvedValue({ bitcoinPrune: true, versions: [{ version: 'latest' }] })
    const wrapper = modal('bitcoin-knots')
    await flushPromises()
    expect((wrapper.get('input').element as HTMLInputElement).checked).toBe(true)
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([['latest', true]])
  })
  it('does not turn off a saved pruning preference when its lookup fails', async () => {
    versions.mockRejectedValue(new Error('offline'))
    const wrapper = modal()
    await flushPromises()
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([['latest', undefined]])
  })
  it('does not offer Bitcoin settings for other apps', async () => {
    versions.mockResolvedValue({ bitcoinPrune: false, versions: [{ version: 'latest' }] })
    const wrapper = modal('other')
    await flushPromises()
    expect(wrapper.find('input').exists()).toBe(false)
  })
})
