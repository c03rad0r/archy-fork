import {mount} from '@vue/test-utils'
import {defineComponent,shallowRef} from 'vue'
import {afterEach,describe,expect,it,vi} from 'vitest'
import {useCompanionVideoPip} from '../useCompanionVideoPip'
afterEach(()=>{delete (window as any).ArchipelagoCloudVideo;delete (window as any).ArchipelagoNative})
function fixture() {
 const messages:any[]=[]
 const bridge:any={onmessage:null,postMessage:vi.fn((raw:string)=>{
  const message=JSON.parse(raw);messages.push(message)
  queueMicrotask(()=>bridge.onmessage?.({data:JSON.stringify({id:message.id,...(message.action==='arm'?{session:message.id}:message.action==='enter'?{active:true}:{})})}))
 })}
 ;(window as any).ArchipelagoCloudVideo=bridge
 const video=document.createElement('video')
 Object.defineProperties(video,{videoWidth:{value:1920},videoHeight:{value:1080},requestFullscreen:{value:vi.fn(async()=>{})}})
 video.pause=vi.fn();video.play=vi.fn(async()=>{})
 let pip!:ReturnType<typeof useCompanionVideoPip>
 const wrapper=mount(defineComponent({setup(){pip=useCompanionVideoPip(shallowRef(video));return()=>null}}))
 return {bridge,messages,video,pip,wrapper}
}
describe('retained companion Cloud video PiP',()=>{
 it('enters only after actual video fullscreen and sends no stream or credentials',async()=>{
  const f=fixture();await f.pip.enter()
  expect(f.video.requestFullscreen).toHaveBeenCalledOnce();expect(f.messages.map(v=>v.action)).toEqual(['arm','enter'])
  expect(f.messages[0]).toMatchObject({width:1920,height:1080})
  expect(f.messages.every(v=>!('url' in v)&&!('token' in v)&&!('cookie' in v))).toBe(true)
  expect(f.pip.active.value).toBe(true)
  f.bridge.onmessage({data:JSON.stringify({type:'event',session:f.messages[0].id,state:'command',command:'pause'})})
  expect(f.video.pause).toHaveBeenCalled()
  f.bridge.onmessage({data:JSON.stringify({type:'event',session:f.messages[0].id,state:'restored'})})
  expect(f.pip.active.value).toBe(false);expect(f.messages[f.messages.length - 1].action).toBe('release');f.wrapper.unmount()
 })
 it('ignores another session controls and stops original video on native dismissal',async()=>{
  const f=fixture();await f.pip.enter();vi.mocked(f.video.pause).mockClear()
  f.bridge.onmessage({data:JSON.stringify({type:'event',session:'foreign',state:'command',command:'pause'})})
  expect(f.video.pause).not.toHaveBeenCalled()
  f.bridge.onmessage({data:JSON.stringify({type:'event',session:f.messages[0].id,state:'closed'})})
  expect(f.video.pause).toHaveBeenCalledOnce();expect(f.pip.active.value).toBe(false);f.wrapper.unmount()
 })
 it('reports unavailable support without handing off or losing the video',async()=>{
  const f=fixture();delete (window as any).ArchipelagoCloudVideo
  await f.pip.enter();expect(f.pip.error.value).toContain('unavailable');expect(f.video.requestFullscreen).not.toHaveBeenCalled();f.wrapper.unmount()
 })
})
