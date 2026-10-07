import { mount } from '@vue/test-utils'
import { defineComponent, nextTick, ref, shallowRef } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
const fixture = vi.hoisted(() => ({
  allowed: true,
  player: { updateExternal: vi.fn(), detachExternal: vi.fn(), releaseExternal: vi.fn(), setExternalVisible: vi.fn() },
  launcher: { mediaAppId: 'node-demo-v4v' as string | null, panelAppId: null as string | null, openSession: vi.fn() },
}))
vi.mock('@/stores/app', () => ({useAppStore:()=>({data:{'package-data':{'node-demo-v4v':{manifest:{version:'1'}}}}})}))
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
    const state = { type: 'archipelago:media-state', version: 1, session: hello.session, available: true, title: 'Song', artist: 'Artist', playing: true, position: 22, duration: 100, artwork: '/media/cover.jpg', shuffle: true }
    const message = (data = state, origin = 'https://node.test:7475', source: unknown = child) => bridge.handle({ data, origin, source } as MessageEvent)
    message(state, 'http://node.test:7475'); message(state, 'https://evil.test'); message(state, 'https://node.test:7475', {})
    message({ ...state, session: 'wrong' }); message({ ...state, duration: NaN })
    expect(fixture.player.updateExternal).not.toHaveBeenCalled()
    message()
    expect(fixture.player.updateExternal).toHaveBeenCalledOnce()
    const [controller, snapshot, shown] = fixture.player.updateExternal.mock.calls[0]!
    expect(snapshot.artwork).toBe('https://node.test:7475/media/cover.jpg'); expect(snapshot.shuffle).toBe(true);
    controller.next(); controller.previous(); controller.shuffle();
    expect(child.postMessage.mock.calls.slice(-3).map(call => call[0].command)).toEqual(['next', 'previous', 'shuffle']);
    expect(snapshot.position).toBe(22); expect(shown).toBe(false)
    controller.seek(30)
    expect(child.postMessage.mock.calls[child.postMessage.mock.calls.length - 1]![0]).toMatchObject({ command: 'seek', position: 30, session: hello.session })
    controller.open(); expect(fixture.launcher.openSession).toHaveBeenCalledWith('node-demo-v4v')
    visible.value = true; await nextTick()
    expect(fixture.player.setExternalVisible).toHaveBeenCalledWith(controller.id, true)
    fixture.allowed = false; message()
    expect(fixture.player.updateExternal).toHaveBeenCalledOnce()
    fixture.allowed = true
    wrapper.unmount()
    expect(fixture.player.releaseExternal).toHaveBeenCalledWith(controller.id)
    expect(child.postMessage.mock.calls[child.postMessage.mock.calls.length - 1]![0]).toMatchObject({ command: 'pause' })
  })
})

it('retains controls and cleanup after catalog expiry but does not admit a new session',()=>{
 const child={postMessage:vi.fn()};let bridge!:ReturnType<typeof useAppMediaBridge>
 const wrapper=mount(defineComponent({setup(){bridge=useAppMediaBridge(ref('node-demo-v4v'),ref('https://node.test:7475/'),shallowRef({contentWindow:child} as unknown as HTMLIFrameElement),ref(false));return()=>null}}))
 fixture.allowed=true;bridge.connect();const session=child.postMessage.mock.calls[0]![0].session
 fixture.allowed=false
 bridge.handle({source:child,origin:'https://node.test:7475',data:{type:'archipelago:media-state',version:1,session,available:true,title:'Song',artist:'Artist',playing:true,position:1,duration:10}} as unknown as MessageEvent)
 const controller=fixture.player.updateExternal.mock.lastCall![0];controller.pause();expect(child.postMessage.mock.lastCall![0].command).toBe('pause')
 wrapper.unmount();expect(child.postMessage.mock.lastCall![0].command).toBe('pause')
 const outsider={postMessage:vi.fn()};const denied=mount(defineComponent({setup(){const b=useAppMediaBridge(ref('node-demo-v4v'),ref('https://node.test:7475/'),shallowRef({contentWindow:outsider} as unknown as HTMLIFrameElement),ref(false));b.connect();return()=>null}}))
 expect(outsider.postMessage).not.toHaveBeenCalled();denied.unmount()
})
