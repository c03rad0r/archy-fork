vi.mock('@/services/installPublishingApp', () => ({ installPublishingApp: vi.fn() }))
import { installPublishingApp } from '@/services/installPublishingApp'
import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
const app = vi.hoisted(() => ({ installPackage: vi.fn(), data: { 'package-data': {} as Record<string, unknown> } }))
vi.mock('@/stores/app', () => ({ useAppStore: () => app }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
import { rpcClient } from '@/api/rpc-client'
import PublicWebGateway from '../PublicWebGateway.vue'
const gateway = { configured: false, routes: [] }
beforeEach(() => { vi.clearAllMocks(); app.data['package-data'] = {}; vi.mocked(rpcClient.call).mockResolvedValue({}) })
describe('private gateway walkthrough', () => {
  it('imports credentials privately and requires deliberate enrollment consent', async () => {
    const wrapper = mount(PublicWebGateway, { props: { gateway } })
    const enrollment = { host: 'gateway.example', domains: ['site.example'], enrollment_token: 'synthetic-private-value' }
    const input = wrapper.get('input[type=file]')
    Object.defineProperty(input.element, 'files', { value: [{ size: 100, text: async () => JSON.stringify(enrollment) }] })
    await input.trigger('change'); await flushPromises()
    expect(wrapper.text()).toContain('gateway.example')
    expect(wrapper.text()).not.toContain('synthetic-private-value')
    const save = wrapper.findAll('button').find(b => b.text() === 'Save private gateway connection')!
    expect(save.attributes('disabled')).toBeDefined()
    expect(rpcClient.call).not.toHaveBeenCalled()
    await wrapper.findAll('input[type=checkbox]')[0]!.setValue(true)
    await save.trigger('click'); await flushPromises()
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'publishing.gateway-configure', params: { enrollment, certificate_mode: 'public', acknowledge: true }, maxRetries: 0 }))
    expect(wrapper.findAll('button').some(b => b.text() === 'Save private gateway connection')).toBe(false)
    expect(wrapper.emitted('refresh')).toHaveLength(1)
  })
  it('uses the normal app installer without enabling any route', async () => {
    const wrapper = mount(PublicWebGateway, { props: { gateway } })
    await wrapper.findAll('button').find(b => b.text() === 'Install Public Web Router')!.trigger('click')
    await flushPromises()
    expect(installPublishingApp).toHaveBeenCalledWith('public-web-router')
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('requests a selected app route without supplying a raw upstream or granting guest credentials', async () => {
    app.data['package-data']['public-web-router'] = { state: 'installed' }
    const wrapper = mount(PublicWebGateway, { props: { gateway: { ...gateway, configured: true, domains: ['app.example'] }, app: { id: 'photoprism', name: 'PhotoPrism' } } })
    await wrapper.get('input[maxlength="253"]').setValue('app.example')
    await wrapper.findAll('button').find(b => b.text() === 'Connect this app through its gate')!.trigger('click')
    await flushPromises()
    expect(rpcClient.call).toHaveBeenCalledTimes(1)
    expect(rpcClient.call).toHaveBeenCalledWith({ method: 'publishing.gateway-app-route', params: { app_id: 'photoprism', domain: 'app.example', enabled: true }, maxRetries: 0 })
  })
  it('reuses saved enrollment and clearly labels test certificates', () => {
    app.data['package-data']['public-web-router'] = { status: 'running' }
    const wrapper = mount(PublicWebGateway, { props: { gateway: { ...gateway, configured: true, host: 'gateway.example', domains: ['site.example'], certificate_mode: 'test' } } })
    expect(wrapper.text()).toContain('Ordinary browsers will not trust this route')
    expect(wrapper.text()).not.toContain('Install Public Web Router')
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
})
