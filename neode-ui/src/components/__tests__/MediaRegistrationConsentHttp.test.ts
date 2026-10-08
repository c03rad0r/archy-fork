import { mount, flushPromises } from '@vue/test-utils'
import { afterEach, expect, it, vi } from 'vitest'
import { webcrypto } from 'node:crypto'
const files = vi.hoisted(() => ({ login: vi.fn(async () => true), listDirectory: vi.fn(async () => []) }))
vi.mock('@/api/filebrowser-client', () => ({ fileBrowserClient: files }))
import MediaRegistrationConsent from '../MediaRegistrationConsent.vue'
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks() })
it('mounts the inactive dashboard consent and opens its labelled dialog on LAN HTTP', async () => {
  vi.stubGlobal('crypto', { getRandomValues: webcrypto.getRandomValues.bind(webcrypto) })
  const wrapper = mount(MediaRegistrationConsent, { props: { request: null, phase: 'select', error: '' } })
  expect(files.login).not.toHaveBeenCalled()
  await wrapper.setProps({ request: { intent: { viewingSeconds: 3600, priceSats: 15 } } as any })
  await flushPromises()
  const dialog = wrapper.get('[role="dialog"]')
  const label = dialog.attributes('aria-labelledby')
  expect(label).toMatch(/^media-registration-[0-9a-f-]{36}$/)
  expect(wrapper.get('#' + label).text()).toBeTruthy()
  wrapper.unmount()
})
