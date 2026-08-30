import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

import { rpcClient } from '@/api/rpc-client'
import EcashSeedBackup from '../EcashSeedBackup.vue'

let wrapper: VueWrapper | null = null

describe('EcashSeedBackup reveal credentials (#127)', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  afterEach(() => {
    wrapper?.unmount()
    wrapper = null
    document.body.innerHTML = ''
  })

  it('asks for a separate backup passphrase only after password decryption fails', async () => {
    vi.mocked(rpcClient.call)
      .mockResolvedValueOnce({
        active: false,
        source: null,
        can_activate: true,
        derivable_from_node_seed: true,
      })
      .mockRejectedValueOnce(new Error(
        'Could not decrypt the saved seed. If you set a separate backup passphrase during setup, enter that passphrase.',
      ))

    wrapper = mount(EcashSeedBackup, { attachTo: document.body })
    await flushPromises()
    await wrapper.get('button').trigger('click')

    expect(document.body.textContent).not.toContain('Separate backup passphrase')
    const password = document.body.querySelector<HTMLInputElement>('input[autocomplete="current-password"]')!
    password.value = 'login-password'
    password.dispatchEvent(new Event('input', { bubbles: true }))
    document.body.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushPromises()

    expect(document.body.textContent).toContain('Separate backup passphrase')
    expect(document.body.textContent).toContain('Your login password did not unlock the saved seed')
    expect(document.body.querySelector('input[placeholder="Passphrase chosen during setup"]')).not.toBeNull()
  })
})
