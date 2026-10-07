import test from 'node:test'
import assert from 'node:assert/strict'
import { attachAudioBridge } from './archipelago-audio.mjs'
function fixture() {
  const sent=[], handlers=new Map(), calls=[]
  const parent={postMessage:(data,origin)=>sent.push({data,origin})}
  const host={parent,addEventListener:(name,fn)=>handlers.set(name,fn),removeEventListener:name=>handlers.delete(name)}
  let unlocked=true
  const state={title:'Own track',artist:'Artist',playing:true,shuffle:false,position:4,duration:30}
  const actions=Object.fromEntries(['play','pause','next','previous','shuffle','seek'].map(name=>[name,async value=>calls.push([name,value])]))
  const bridge=attachAudioBridge({parentOrigin:'https://node.test',host,snapshot:()=>state,unlocked:()=>unlocked,actions})
  const receive=(data,origin='https://node.test',source=parent)=>handlers.get('message')?.({source,origin,data:{version:1,...data}})
  return {sent,handlers,calls,state,bridge,receive,lock:()=>{unlocked=false}}
}
test('validates parent and nonce; delegates real controls with bounded seeks',async()=>{
  const f=fixture(),session='a'.repeat(32)
  f.receive({type:'archipelago:media-connect',session},'https://foreign.test')
  assert.equal(f.sent.length,1)
  f.receive({type:'archipelago:media-connect',session})
  f.receive({type:'archipelago:media-control',session:'b'.repeat(32),command:'play'})
  f.receive({type:'archipelago:media-control',session,command:'seek',position:99})
  f.receive({type:'archipelago:media-control',session,command:'previous'})
  await new Promise(setImmediate)
  assert.deepEqual(f.calls,[['seek',30],['previous',undefined]])
  assert(f.sent.every(message=>message.origin==='https://node.test'))
  f.lock();f.bridge.publish();assert.equal(f.sent.at(-1).data.available,false)
  assert.equal('title' in f.sent.at(-1).data,false)
  f.bridge.dispose();assert.equal(f.handlers.size,0)
})
test('stale queued commands do not control a replacement handshake',async()=>{
  const f=fixture(),session='a'.repeat(32)
  f.receive({type:'archipelago:media-connect',session})
  f.receive({type:'archipelago:media-control',session,command:'next'})
  f.receive({type:'archipelago:media-connect',session:'b'.repeat(32)})
  await new Promise(setImmediate)
  assert.deepEqual(f.calls,[])
  f.bridge.dispose()
})

test('dispose releases listeners even if the app pause action rejects',async()=>{
  const handlers=new Map()
  const host={parent:{postMessage(){}},addEventListener:(name,fn)=>handlers.set(name,fn),removeEventListener:name=>handlers.delete(name)}
  const bridge=attachAudioBridge({parentOrigin:'https://node.test',host,snapshot:()=>null,actions:{pause:async()=>{throw Error('player already closed')}}})
  bridge.dispose();await new Promise(setImmediate);assert.equal(handlers.size,0)
})
