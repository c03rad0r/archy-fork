import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import ChangePasswordSection from '../ChangePasswordSection.vue'

const { changePassword } = vi.hoisted(() => ({
  changePassword: vi.fn(),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    changePassword,
  },
}))

vi.mock('@/composables/useModalKeyboard', () => ({
  useModalKeyboard: vi.fn(),
}))

describe('ChangePasswordSection', () => {
  beforeEach(() => {
    changePassword.mockReset()
  })

  it('shows a warning when SSH update fails after web password update succeeds', async () => {
    changePassword.mockResolvedValueOnce({
      success: true,
      ssh_updated: false,
      ssh_error: 'sudo unavailable',
    })
    const wrapper = mount(ChangePasswordSection, {
      global: {
        stubs: {
          Teleport: true,
        },
      },
    })

    await wrapper.find('button').trigger('click')
    const inputs = wrapper.findAll('input')
    await inputs[0]!.setValue('password123')
    await inputs[1]!.setValue('MyP@ssw0rd!123')
    await inputs[2]!.setValue('MyP@ssw0rd!123')
    await wrapper.find('form').trigger('submit.prevent')

    expect(changePassword).toHaveBeenCalledWith({
      currentPassword: 'password123',
      newPassword: 'MyP@ssw0rd!123',
      alsoChangeSsh: true,
    })
    expect(wrapper.text()).toContain('settings.passwordUpdatedSuccess')
    expect(wrapper.text()).toContain('settings.passwordUpdatedSshFailed sudo unavailable')
    expect(wrapper.text()).not.toContain('settings.passwordChangeFailed')
  })
})
