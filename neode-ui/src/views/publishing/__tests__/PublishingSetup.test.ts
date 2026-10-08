vi.mock('@/services/installPublishingApp', () => ({ installPublishingApp: vi.fn() }))
import { installPublishingApp } from '@/services/installPublishingApp'
import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
const api = vi.hoisted(() => ({ status: vi.fn(), update: vi.fn(), dns: vi.fn(), generate: vi.fn(), verifyHttps: vi.fn() }))
const page = vi.hoisted(() => ({ name: 'external-access' }))
const appStore = vi.hoisted(() => ({ installPackage: vi.fn(), data: { 'package-data': {} as Record<string, unknown> } }))
vi.mock('@/stores/app', () => ({ useAppStore: () => appStore }))
vi.mock('vue-router', () => ({ useRoute: () => page, useRouter: () => ({ push: vi.fn() }), RouterLink: { props: ['to'], template: '<a :href="to"><slot /></a>' } }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('@/components/AIConnectionModal.vue', () => ({ default: { props: ['show'], template: '<div data-testid="ai-connection" :data-open="show" />', methods: { showRoutstr() {} } } }))
vi.mock('@/services/publishing', async (original) => ({ ...await original<typeof import('@/services/publishing')>(), publishing: api }))
import PublishingSetup from '../PublishingSetup.vue'
import { rpcClient } from '@/api/rpc-client'

const state = () => ({ schema: 1, version: 2, connections: ['fips'], projects: {} })
beforeEach(() => {
  vi.clearAllMocks(); page.name = 'external-access'
  appStore.data['package-data'] = {}
  api.status.mockResolvedValue({ state: state(), fips_address: null, apps: [], publication_enabled: false, notice: 'Saving does not publish.' })
  api.update.mockResolvedValue({ state: { ...state(), version: 3 }, project_id: null })
})
describe('publishing setup', () => {
  it('offers provider setup and credit from website design without starting generation', async () => {
    page.name = 'publish-website'
    appStore.data['package-data'].blossom = { state: 'installed' }
    const wrapper = mount(PublishingSetup); await flushPromises()
    expect(wrapper.text()).toContain('Use Routstr by default')
    expect(wrapper.text()).not.toContain('Installed Ollama model')
    await wrapper.findAll('button').find(b => b.text() === 'Routstr credit and top up')!.trigger('click')
    expect(wrapper.get('[data-testid="ai-connection"]').attributes('data-open')).toBe('true')
    expect(api.generate).not.toHaveBeenCalled()
    expect(api.update).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('revokes a local nsite asset through the publishing action without announcing anything', async () => {
    page.name = 'publish-website'
    appStore.data['package-data'].blossom = { state: 'installed' }
    api.status.mockResolvedValue({ state: { ...state(), projects: { site: { id: 'site', name: 'Site', draft: '<h1>Public</h1>', routes: ['nostr', 'public-web'], domain: { hostname: 'site.example' }, revisions: [], fips_publication: { html: '<h1>Public</h1>', port: 32000, nsite_asset: { html: '<h1>Reviewed</h1>', receipt: { sha256: 'a'.repeat(64), size: 17 } } } } } }, apps: [], publication_enabled: true })
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.get('[aria-controls="setup-step-publish"]').trigger('click')
    await wrapper.findAll('button').find(b => b.text() === 'Stop sharing the local nsite file')!.trigger('click')
    await flushPromises()
    expect(api.update).toHaveBeenCalledWith(2, { action: 'unshare-nsite-asset', id: 'site' })
    expect(rpcClient.call).not.toHaveBeenCalledWith(expect.objectContaining({ method: 'identity.nostr-sign' }))
    expect(wrapper.text()).toContain('Local nsite file sharing stopped')
    wrapper.unmount()
  })

  it('keeps unpublish controls available when a published route is deselected', async () => {
    page.name = 'publish-website'
    appStore.data['package-data'].blossom = { state: 'installed' }
    api.status.mockResolvedValue({ state: { ...state(), projects: { site: { id: 'site', name: 'Site', draft: '<h1>Public</h1>', routes: ['tor'], domain: null, revisions: [], tor_publication: { html: '<h1>Public</h1>', port: 32100 } } } }, apps: [], publication_enabled: true })
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.get('[aria-controls="setup-step-connections"]').trigger('click')
    await wrapper.get('input[value="tor"]').setValue(false)
    await wrapper.get('[aria-controls="setup-step-publish"]').trigger('click')
    expect(wrapper.findAll('button').some(button => button.text() === 'Unpublish from Tor')).toBe(true)
    expect(api.update).not.toHaveBeenCalled()
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('reuses saved routes and advances the walkthrough without publishing or signing', async () => {
    page.name = 'publish-website'
    appStore.data['package-data'].blossom = { state: 'installed' }
    const wrapper = mount(PublishingSetup); await flushPromises()
    expect(wrapper.get('[aria-controls="setup-step-design"]').attributes('aria-expanded')).toBe('true')
    expect(wrapper.get('[aria-controls="setup-step-connections"]').attributes('aria-expanded')).toBe('false')
    expect(wrapper.findAll('button').find(b => b.text().startsWith('Save and continue'))!.attributes('disabled')).toBeDefined()
    await wrapper.get('[aria-controls="setup-step-publish"]').trigger('click')
    expect(wrapper.get('[aria-controls="setup-step-publish"]').attributes('aria-expanded')).toBe('true')
    expect(api.update).not.toHaveBeenCalled()
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('saves before advancing and keeps failed saves on the same step', async () => {
    const wrapper = mount(PublishingSetup); await flushPromises()
    api.update.mockRejectedValueOnce(new Error('Connection choices could not be saved'))
    await wrapper.findAll('button').find(b => b.text().startsWith('Save and continue'))!.trigger('click'); await flushPromises()
    expect(wrapper.get('[aria-controls="setup-step-connections"]').attributes('aria-expanded')).toBe('true')
    expect(wrapper.get('[role="alert"]').text()).toContain('could not be saved')
    await wrapper.findAll('button').find(b => b.text().startsWith('Save and continue'))!.trigger('click'); await flushPromises()
    expect(wrapper.get('[aria-controls="setup-step-sharing"]').attributes('aria-expanded')).toBe('true')
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('carries the existing connection choices into a new website draft', async () => {
    page.name = 'publish-website'
    appStore.data['package-data'].blossom = { state: 'installed' }
    api.update.mockResolvedValueOnce({ state: { ...state(), version: 3, projects: { site: { id: 'site', name: 'My website', draft: '', routes: [], domain: null, revisions: [] } } }, project_id: 'site' })
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.findAll('button').find(b => b.text() === 'Create another website')!.trigger('click'); await flushPromises()
    await wrapper.get('[aria-controls="setup-step-connections"]').trigger('click')
    expect((wrapper.get('input[value="fips"]').element as HTMLInputElement).checked).toBe(true)
    expect(api.update).toHaveBeenCalledTimes(1)
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('checks public HTTPS only on request and clears verification when the domain changes', async () => {
    page.name = 'publish-website'
    api.status.mockResolvedValue({ state: { ...state(), projects: { site: { id: 'site', name: 'Site', draft: '<h1>Public</h1>', routes: ['public-web'], domain: { hostname: 'www.example.com', destination: '8.8.8.8' }, revisions: [], fips_publication: { html: '<h1>Public</h1>', port: 32000 } } } }, apps: [], fips_address: 'fd00::1', publication_enabled: true, notice: '' })
    api.verifyHttps.mockResolvedValue({ hostname: 'www.example.com', sha256: 'synthetic', checked_at: '2026-10-08T00:00:00Z' })
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.get('[aria-controls="setup-step-domain"]').trigger('click')
    expect(api.verifyHttps).not.toHaveBeenCalled()
    await wrapper.findAll('button').find(b => b.text() === 'Check public HTTPS')!.trigger('click'); await flushPromises()
    expect(api.verifyHttps).toHaveBeenCalledWith('site', 2)
    expect(wrapper.text()).toContain('valid TLS and exact published content')
    await wrapper.get('input[placeholder="www.yourdomain.com"]').setValue('other.example.com')
    expect(wrapper.text()).not.toContain('valid TLS and exact published content')
  })
  it('creates only an explicit app-scoped grant and supports revocation without showing other credentials', async () => {
    api.status.mockResolvedValue({ state: state(), fips_address: null, apps: [{ id: 'nextcloud', name: 'Nextcloud', port: 8080, guest_access: true }], grants: [{ id: 'external:test:Guest', label: 'Guest', apps: ['nextcloud'], expires_at: 2000000000 }], notice: '' })
    vi.mocked(rpcClient.call).mockResolvedValue({ id: 'external:test:Guest', token: 'synthetic-test-token', app_id: 'nextcloud', expires_at: 2000000000 })
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.get('[aria-controls="setup-step-sharing"]').trigger('click')
    expect(rpcClient.call).not.toHaveBeenCalled()
    await wrapper.get('button[aria-haspopup="listbox"]').trigger('click')
    await wrapper.get('input[role="combobox"]').setValue('not a supported app')
    expect(wrapper.text()).not.toContain('Create app-only access')
    expect(wrapper.text()).toContain('No matching apps')
    await wrapper.get('input[role="combobox"]').setValue('Nextcloud')
    await wrapper.get('[role="option"]').trigger('click')
    await wrapper.findAll('button').find(b => b.text() === 'Create app-only access')!.trigger('click'); await flushPromises()
    expect(rpcClient.call).toHaveBeenCalledWith({ method: 'publishing.access-create', params: { app_id: 'nextcloud', label: 'Guest', hours: 24 }, maxRetries: 0 })
    expect(wrapper.text()).toContain('synthetic-test-token')
    await wrapper.findAll('button').find(b => b.text() === 'Revoke access')!.trigger('click'); await flushPromises()
    expect(rpcClient.call).toHaveBeenLastCalledWith({ method: 'publishing.access-revoke', params: { id: 'external:test:Guest' }, maxRetries: 0 })
    expect(wrapper.text()).not.toContain('synthetic-test-token')
  })
  it('offers catalog installation and skips it when Blossom is already installed', async () => {
    page.name = 'publish-website'
    const wrapper = mount(PublishingSetup); await flushPromises()
    expect(wrapper.get('[data-testid="blossom-setup"]').text()).toContain('Install Blossom')
    await wrapper.findAll('button').find(b => b.text() === 'Install Blossom')!.trigger('click'); await flushPromises()
    expect(installPublishingApp).toHaveBeenCalledWith('blossom')
    wrapper.unmount()
    appStore.data['package-data'].blossom = { state: 'installed' }
    const installed = mount(PublishingSetup); await flushPromises()
    expect(installed.find('[aria-controls="setup-step-storage"]').exists()).toBe(false)
    expect(installed.get('[aria-controls="setup-step-design"]').attributes('aria-expanded')).toBe('true')
    expect(installed.get('[data-testid="blossom-setup"]').text()).toContain('skip installation')
    expect(installed.find('a[href="/dashboard/marketplace/blossom"]').exists()).toBe(false)
  })
  it('loads choices from the node and saves multiple routes without activating them', async () => {
    const wrapper = mount(PublishingSetup); await flushPromises()
    const inputs = wrapper.findAll('input[type="checkbox"][value]')
    expect((inputs[0]!.element as HTMLInputElement).checked).toBe(true)
    await inputs[2]!.setValue(true)
    await wrapper.findAll('button').find(b => b.text().startsWith('Save and continue'))!.trigger('click')
    await flushPromises()
    expect(api.update).toHaveBeenCalledWith(2, { action: 'connections', routes: ['fips', 'tor'] })
    expect(wrapper.text()).toContain('Existing app access has not changed')
  })
  it('keeps a failed save visible and does not pretend it succeeded', async () => {
    api.update.mockRejectedValue(new Error('Reload before saving'))
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.findAll('button').find(b => b.text().startsWith('Save and continue'))!.trigger('click'); await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('Reload before saving')
    expect(wrapper.text()).not.toContain('Connection preferences saved')
  })
  it('isolates saved HTML and presents Nostr as an independent choice', async () => {
    page.name = 'publish-website'
    api.status.mockResolvedValue({ state: { ...state(), projects: { site: { id: 'site', name: 'Site', draft: '<script>parent.fetch("/rpc")</script>', routes: ['fips', 'nostr'], domain: null, revisions: [] } } }, apps: [], fips_address: 'fd00::1', publication_enabled: false, notice: 'Saving does not publish.' })
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.get('[aria-controls="setup-step-design"]').trigger('click')
    expect(wrapper.get('iframe').attributes('sandbox')).toBe('')
    expect(wrapper.get('iframe').attributes('srcdoc')).toContain("default-src 'none'")
    await wrapper.get('[aria-controls="setup-step-connections"]').trigger('click')
    expect(wrapper.findAll('input[type="checkbox"][value]')).toHaveLength(4)
    expect(wrapper.text()).toContain('reachability still needs verification')
  })
})
