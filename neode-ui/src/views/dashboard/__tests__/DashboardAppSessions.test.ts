import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, nextTick, onMounted, onUnmounted } from 'vue'
vi.mock('@/stores/appLauncher', async () => {
  const {reactive} = await import('vue')
  const state = reactive({panelAppId:null as string|null, mediaAppId:null as string|null, panelPath:null as string|null,
    closePanel() {state.panelAppId=null;state.panelPath=null}})
  return {useAppLauncherStore:()=>state}
})
import { useAppLauncherStore } from '@/stores/appLauncher'
import DashboardAppSessions from '../DashboardAppSessions.vue'
describe('stable dashboard app frame ownership', () => {
  const mounted=vi.fn(), unmounted=vi.fn()
  const Session=defineComponent({props:{appIdProp:String,pathProp:String,suspended:Boolean},setup(props) {
    onMounted(()=>mounted(props.appIdProp));onUnmounted(()=>unmounted(props.appIdProp))
    return ()=>h('iframe',{'data-app':props.appIdProp,src:props.pathProp ?? '/browse','data-suspended':String(props.suspended)})
  }})
  beforeEach(()=> {vi.clearAllMocks();const store=useAppLauncherStore();store.panelAppId=null;store.mediaAppId=null;store.panelPath=null})
  it('keeps the exact playing frame and deep link through late promotion, close and reopen',async()=>{
    const store=useAppLauncherStore();store.panelAppId='music';store.panelPath='/browse/album/one'
    const wrapper=mount(DashboardAppSessions,{global:{stubs:{AppSession:Session,TransitionGroup:false}}})
    const frame=wrapper.get('iframe').element as HTMLIFrameElement
    frame.dataset.playbackPosition='17'
    store.mediaAppId='music';await nextTick()
    expect(wrapper.get('iframe').element).toBe(frame)
    expect(mounted).toHaveBeenCalledTimes(1);expect(unmounted).not.toHaveBeenCalled()
    store.closePanel();await nextTick()
    expect(wrapper.get('iframe').element).toBe(frame)
    expect(frame.getAttribute('src')).toBe('/browse/album/one')
    expect(frame.dataset.suspended).toBe('true')
    store.panelAppId='music';await nextTick()
    expect(wrapper.get('iframe').element).toBe(frame)
    expect(frame.dataset.playbackPosition).toBe('17')
    expect(frame.getAttribute('src')).toBe('/browse/album/one')
    expect(frame.dataset.suspended).toBe('false')
    expect(unmounted).not.toHaveBeenCalled()
    wrapper.unmount();expect(unmounted).toHaveBeenCalledOnce()
  })
  it('keeps retained music while another app opens and promotes that app without recreating it',async()=>{
    const store=useAppLauncherStore();store.panelAppId='music';store.panelPath='/first'
    const wrapper=mount(DashboardAppSessions,{global:{stubs:{AppSession:Session,TransitionGroup:false}}})
    const music=wrapper.get('iframe[data-app="music"]').element
    store.mediaAppId='music';await nextTick()
    store.panelPath='/station/two';store.panelAppId='radio';await nextTick()
    const radio=wrapper.get('iframe[data-app="radio"]').element
    expect(wrapper.get('iframe[data-app="music"]').element).toBe(music)
    expect(music.getAttribute('src')).toBe('/first')
    store.mediaAppId='radio';await nextTick()
    expect(wrapper.get('iframe[data-app="radio"]').element).toBe(radio)
    expect(mounted.mock.calls.map(call=>call[0])).toEqual(['music','radio'])
    expect(unmounted).toHaveBeenCalledWith('music')
    expect(unmounted).not.toHaveBeenCalledWith('radio')
    wrapper.unmount()
  })
})
