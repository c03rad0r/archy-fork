import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import ContainerAppDetails from '../ContainerAppDetails.vue'

const fixture = vi.hoisted(() => ({
  confirm: vi.fn(), remove: vi.fn(), push: vi.fn(),
}))
vi.mock('vue-router', () => ({
  useRoute: () => ({ params: { id: 'filebrowser' } }),
  useRouter: () => ({ push: fixture.push }),
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/composables/useAppConfirm', () => ({ appConfirm: fixture.confirm }))
vi.mock('@/stores/container', () => ({ useContainerStore: () => ({
  getContainerStatus: async () => ({ id: 'fixture', image: 'fixture', state: 'running', created: '2026-10-08' }),
  getContainerLogs: async () => [], fetchHealthStatus: async () => {}, getHealthStatus: () => 'healthy',
  removeContainer: fixture.remove,
}) }))
let wrapper: VueWrapper | undefined
afterEach(() => { wrapper?.unmount(); vi.clearAllMocks() })

describe('legacy File Browser removal route', () => {
  it.each([false, true])('requires the Cloud warning confirmation (%s)', async accepted => {
    fixture.confirm.mockResolvedValue(accepted)
    wrapper = mount(ContainerAppDetails, { global: { stubs: { BackButton: true, ContainerStatus: true } } })
    await flushPromises()
    await wrapper.findAll('button').find(button => button.text() === 'common.remove')!.trigger('click')
    await flushPromises()
    expect(fixture.confirm).toHaveBeenCalledWith(expect.objectContaining({
      title: 'apps.fileBrowserWarningTitle', message: 'apps.fileBrowserWarning',
      confirmLabel: 'apps.removeFileBrowser', cancelLabel: 'apps.keepFileBrowser', danger: true,
    }))
    expect(fixture.remove).toHaveBeenCalledTimes(accepted ? 1 : 0)
    if (accepted) expect(fixture.remove).toHaveBeenCalledWith('filebrowser')
  })
})
