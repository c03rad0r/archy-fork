import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { reactive, nextTick } from 'vue'

const fake = reactive<{ packages: Record<string, unknown> }>({ packages: {} })
vi.mock('../sync', () => ({ useSyncStore: () => fake }))
vi.mock('../../api/rpc-client', () => ({ rpcClient: {} }))
import { useServerStore } from '../server'

function installing(phase = 'preparing-app') {
  return { state: 'installing', manifest: { title: 'Git Workshop' }, 'install-progress': { phase, size: 0, downloaded: 0 } }
}

describe('installation state after hard refresh', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    fake.packages = {}
  })

  it('restores an in-flight install from an already-loaded server snapshot', () => {
    fake.packages = { 'archipelago-source': installing() }
    const store = useServerStore()
    expect(store.isInstalling('archipelago-source')).toBe(true)
    expect(store.installingApps.get('archipelago-source')).toMatchObject({
      progress: 20,
      message: 'Downloading, building and starting app…',
    })
  })

  it('keeps a long download visible and clears it on terminal success', async () => {
    const store = useServerStore()
    fake.packages = { 'nginx-proxy-manager': installing() }
    await nextTick()
    expect(store.isInstalling('nginx-proxy-manager')).toBe(true)
    fake.packages = { 'nginx-proxy-manager': installing() }
    await nextTick()
    expect(store.installingApps.get('nginx-proxy-manager')?.progress).toBe(20)
    fake.packages = { 'nginx-proxy-manager': { state: 'running' } }
    await nextTick()
    expect(store.isInstalling('nginx-proxy-manager')).toBe(false)
  })
})
