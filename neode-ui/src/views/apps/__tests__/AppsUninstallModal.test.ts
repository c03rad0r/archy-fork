import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import AppsUninstallModal from '../AppsUninstallModal.vue'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string>) => {
      if (params?.name) return `${key} ${params.name}`
      return key
    },
  }),
}))

vi.mock('@/composables/useModalKeyboard', () => ({
  useModalKeyboard: vi.fn(),
}))

describe('AppsUninstallModal', () => {
  it('emits the delete-data choice when uninstall is confirmed', async () => {
    const wrapper = mount(AppsUninstallModal, {
      props: {
        show: true,
        appTitle: 'File Browser',
        uninstalling: false,
      },
    })

    const checkbox = document.body.querySelector<HTMLInputElement>('input[type="checkbox"]')
    expect(checkbox).not.toBeNull()
    checkbox?.click()
    const confirmButton = document.body.querySelector<HTMLButtonElement>('button.glass-button-danger')
    expect(confirmButton).not.toBeNull()
    confirmButton?.click()

    expect(wrapper.emitted('confirm')?.[0]).toEqual([true])
  })
})
