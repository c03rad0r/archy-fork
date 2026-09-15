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

  it('signals readiness only after authenticated setup is finished and clears the words', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'wallet.ecash-seed-status') {
        return { active: false, can_activate: true, derivable_from_node_seed: true, source: null } as never
      }
      if (method === 'wallet.ecash-seed-reveal') {
        return { words: [...Array(23).fill('abandon'), 'art'], source: 'node-seed' } as never
      }
      throw new Error('unexpected request')
    })
    wrapper = mount(EcashSeedBackup, { props: { setupOnly: true }, attachTo: document.body })
    await flushPromises()
    await wrapper.get('button').trigger('click')
    const cancel = Array.from(document.body.querySelectorAll('button')).find(b => b.textContent === 'Cancel')!
    cancel.click()
    await flushPromises()
    expect(wrapper.emitted('ready')).toBeUndefined()
    await wrapper.get('button').trigger('click')
    const password = document.body.querySelector<HTMLInputElement>('input[autocomplete="current-password"]')!
    password.value = 'test-password'
    password.dispatchEvent(new Event('input', { bubbles: true }))
    document.body.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushPromises()
    expect(wrapper.emitted('ready')).toBeUndefined()
    Array.from(document.body.querySelectorAll('button')).find(b => b.textContent === 'Done')!.click()
    await flushPromises()
    expect(wrapper.emitted('ready')).toEqual([[]])
    expect(document.body.querySelector('[aria-labelledby="reveal-ecash-seed-title"]')).toBeNull()
    expect(document.body.textContent).not.toContain('abandon')
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
