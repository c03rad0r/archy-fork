import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import BackupSection from '../BackupSection.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function makeBackup() {
  return {
    id: 'backup-one',
    created_at: '2026-06-10T10:00:00Z',
    size_bytes: 2048,
    encrypted: true,
    description: 'Before upgrade',
  }
}

describe('BackupSection', () => {
  it('keeps backups visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({ backups: [makeBackup()] })

    const wrapper = mount(BackupSection, {
      global: {
        stubs: {
          Teleport: true,
        },
      },
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Before upgrade')
    expect(wrapper.text()).toContain('2.0 KB')

    const pending = deferred<{ backups: [] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadBackups: () => Promise<void> }).loadBackups()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Before upgrade')
    expect(wrapper.text()).toContain('Refreshing backups...')
    expect(wrapper.text()).not.toContain('settings.loadingBackups')
    expect(wrapper.text()).not.toContain('settings.noBackups')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('Before upgrade')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing backups...')
    expect(wrapper.text()).not.toContain('settings.noBackups')
  })
})
