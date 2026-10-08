import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import AppsUninstallModal from '../AppsUninstallModal.vue'
import en from '@/locales/en.json'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string>) => {
      const [group, name] = key.split('.')
      const messages = en as Record<string, Record<string, string>>
      const text = messages[group!]?.[name!] ?? key
      return params?.name ? text.replace('{name}', params.name) : text
    },
  }),
}))

vi.mock('@/composables/useModalKeyboard', () => ({ useModalKeyboard: vi.fn() }))

const mounted: VueWrapper[] = []
function dialog(appId = 'filebrowser') {
  const wrapper = mount(AppsUninstallModal, {
    props: { show: true, appId, appTitle: 'File Browser', uninstalling: false },
    global: { stubs: { teleport: true } },
  })
  mounted.push(wrapper)
  return wrapper
}
afterEach(() => { mounted.splice(0).forEach(wrapper => wrapper.unmount()) })

describe('AppsUninstallModal', () => {
  it.each(['filebrowser', 'archy-filebrowser'])('warns for %s and preserves files by default', async appId => {
    const wrapper = dialog(appId)
    expect(wrapper.get('[role="note"]').text()).toContain('Cloud needs File Browser')
    expect(wrapper.text()).toContain('Installing Nextcloud or Immich does not replace this connection yet.')
    expect(wrapper.get<HTMLInputElement>('input[type="checkbox"]').element.checked).toBe(false)
    await wrapper.get('button.glass-button-danger').trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([[false]])
  })

  it('cancels without authorizing removal', async () => {
    const wrapper = dialog()
    const keep = wrapper.findAll('button').find(button => button.text() === 'Keep File Browser')!
    await keep.trigger('click')
    expect(wrapper.emitted('close')).toHaveLength(1)
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('requires a fresh delete-data choice after closing or switching apps', async () => {
    const wrapper = dialog()
    await wrapper.get('input').setValue(true)
    await wrapper.get('button.glass-button-danger').trigger('click')
    expect(wrapper.emitted('confirm')?.[0]).toEqual([true])
    await wrapper.setProps({ show: false })
    await wrapper.setProps({ show: true })
    expect(wrapper.get<HTMLInputElement>('input').element.checked).toBe(false)
    await wrapper.get('input').setValue(true)
    await wrapper.setProps({ appId: 'nextcloud', appTitle: 'Nextcloud' })
    expect(wrapper.get<HTMLInputElement>('input').element.checked).toBe(false)
    expect(wrapper.find('[role="note"]').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('Keep File Browser')
  })

  it('does not infer dependencies from a display name or allow repeat submission while busy', async () => {
    const wrapper = dialog('unrelated-app')
    expect(wrapper.find('[role="note"]').exists()).toBe(false)
    await wrapper.setProps({ uninstalling: true })
    await wrapper.get('button.glass-button-danger').trigger('click')
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })
})
