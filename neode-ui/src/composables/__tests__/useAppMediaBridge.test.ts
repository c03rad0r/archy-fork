import { mount } from '@vue/test-utils'
import { defineComponent, nextTick, ref, shallowRef } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
const fixture = vi.hoisted(() => ({
  allowed: true,
  player: { updateExternal: vi.fn(), detachExternal: vi.fn(), releaseExternal: vi.fn(), setExternalVisible: vi.fn() },
  launcher: { mediaAppId: 'node-demo-v4v' as string | null, panelAppId: null as string | null, openSession: vi.fn() },
}))
vi.mock('../useAudioPlayer', () => ({ useAudioPlayer: () => fixture.player }))
vi.mock('@/stores/appLauncher', () => ({ useAppLauncherStore: () => fixture.launcher }))
vi.mock('@/views/discover/curatedApps', () => ({ appHasMediaBridge: () => fixture.allowed }))
import { useAppMediaBridge } from '../useAppMediaBridge'

describe('app media session boundary', () => {
  beforeEach(() => { vi.clearAllMocks(); fixture.allowed = true; fixture.launcher.mediaAppId = 'node-demo-v4v'; fixture.launcher.panelAppId = null })
  it('checks frame, origin, nonce and finite state before attaching controls', async () => {
    const child = { postMessage: vi.fn() }
    const visible = ref(false)
    let bridge!: ReturnType<typeof useAppMediaBridge>
    const wrapper = mount(defineComponent({ setup() {
      bridge = useAppMediaBridge(ref('node-demo-v4v'), ref('https://node.test:7475/'), shallowRef({ contentWindow: child } as unknown as HTMLIFrameElement), visible)
      return () => null
    } }))
    bridge.connect()
    const hello = child.postMessage.mock.calls[0]![0]
    expect(child.postMessage.mock.calls[0]![1]).toBe('https://node.test:7475')
    const state = { type: 'archipelago:media-state', version: 1, session: hello.session, available: true, title: 'Song', artist: 'Artist', playing: true, position: 22, duration: 100 }
    const message = (data = state, origin = 'https://node.test:7475', source: unknown = child) => bridge.handle({ data, origin, source } as MessageEvent)
    message(state, 'http://node.test:7475'); message(state, 'https://evil.test'); message(state, 'https://node.test:7475', {})
    message({ ...state, session: 'wrong' }); message({ ...state, duration: NaN })
    expect(fixture.player.updateExternal).not.toHaveBeenCalled()
    message()
    expect(fixture.player.updateExternal).toHaveBeenCalledOnce()
    const [controller, snapshot, shown] = fixture.player.updateExternal.mock.calls[0]!
    expect(snapshot.position).toBe(22); expect(shown).toBe(false)
    controller.seek(30)
    expect(child.postMessage.mock.calls.at(-1)![0]).toMatchObject({ command: 'seek', position: 30, session: hello.session })
    controller.open(); expect(fixture.launcher.openSession).toHaveBeenCalledWith('node-demo-v4v')
    visible.value = true; await nextTick()
    expect(fixture.player.setExternalVisible).toHaveBeenCalledWith(controller.id, true)
    fixture.allowed = false; message()
    expect(fixture.player.updateExternal).toHaveBeenCalledOnce()
    fixture.allowed = true
    wrapper.unmount()
    expect(fixture.player.releaseExternal).toHaveBeenCalledWith(controller.id)
    expect(child.postMessage.mock.calls.at(-1)![0]).toMatchObject({ command: 'pause' })
  })
})
