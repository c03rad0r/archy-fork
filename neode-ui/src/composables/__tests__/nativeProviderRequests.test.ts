import { readFileSync } from 'node:fs'
import { runInNewContext } from 'node:vm'
import { describe, expect, it, vi } from 'vitest'
const source=readFileSync('public/nostr-provider.js','utf8')
describe('native provider on ordinary HTTP node origins',()=>{
 it('uses secure randomness without randomUUID for both rental and registration requests',async()=>{
  const listeners:((event:unknown)=>void)[]=[]
  const parent={postMessage:vi.fn()}
  const window:any={top:{},parent,location:{href:'http://node.local:7778/browse',pathname:'/browse',port:'7778',origin:'http://node.local:7778'},addEventListener:(_name:string,handler:(event:unknown)=>void)=>listeners.push(handler)}
  const timers=new Set<unknown>();let count=0
  runInNewContext(source,{window,document:{currentScript:{hasAttribute:()=>true},readyState:'complete'},crypto:{getRandomValues:(bytes:Uint8Array)=>{bytes.fill(++count);return bytes}},Uint8Array,URL,console,
    setTimeout:(fn:unknown)=>{timers.add(fn);return fn},clearTimeout:(fn:unknown)=>timers.delete(fn)})
  expect(window.archipelagoRental.playbackProtocol).toBe(2)
  expect(Object.getOwnPropertyDescriptor(window.archipelagoRental,'playbackProtocol')?.writable).toBe(false)
  await expect(window.archipelagoRental.request({title:'Legacy'})).rejects.toThrow('protocol 2')
  expect(parent.postMessage).not.toHaveBeenCalled()
  const rental=window.archipelagoRental.request({title:'Film'},{playbackProtocol:2})
  const registration=window.archipelagoMediaRegistration.request('resume',{intent:{requestId:'existing'}})
  const requests=parent.postMessage.mock.calls.map(([message])=>message)
  expect(requests).toHaveLength(2)
  expect(requests[0].id).toMatch(/^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/)
  expect(requests[1].id).not.toBe(requests[0].id)
  for(const message of requests) for(const receive of listeners) receive({source:parent,origin:'http://node.local',data:{type:message.type.replace('-request','-response'),id:message.id,result:{ok:true}}})
  await expect(rental).resolves.toEqual({ok:true});await expect(registration).resolves.toEqual({ok:true})
  expect(timers.size).toBe(0)
  parent.postMessage.mockClear()
  const stop = new AbortController()
  const closed = window.archipelagoRental.request({title:'Closed'}, {playbackProtocol:2,signal:stop.signal})
  const closedCheck = expect(closed).rejects.toThrow('Rental request closed')
  const retained = window.archipelagoRental.request({title:'Other'}, {playbackProtocol:2})
  const messages=parent.postMessage.mock.calls.map(([message])=>message)
  stop.abort();await closedCheck
  expect(parent.postMessage.mock.lastCall![0]).toMatchObject({action:'cancel',id:messages[0].id,playbackProtocol:2})
  expect(timers.size).toBe(1)
  for(const receive of listeners)receive({source:parent,origin:'http://node.local',data:{type:'archipelago-rental-response',id:messages[1].id,result:{playbackProtocol:2,handle:'d'.repeat(64)}}})
  await expect(retained).resolves.toMatchObject({playbackProtocol:2});expect(timers.size).toBe(0)
  parent.postMessage.mockClear()
  await expect(window.archipelagoRental.request({title:'Already closed'},{playbackProtocol:2,signal:stop.signal})).rejects.toThrow('closed')
  expect(parent.postMessage).not.toHaveBeenCalled()
 })
 it('validates and forwards the explicit rental renewal choice',async()=>{
  const listeners:((event:unknown)=>void)[]=[]
  const parent={postMessage:vi.fn()}
  const window:any={top:{},parent,location:{href:'http://node.local:7778/browse',pathname:'/browse',port:'7778',origin:'http://node.local:7778'},addEventListener:(_name:string,handler:(event:unknown)=>void)=>listeners.push(handler)}
  const timers=new Set<unknown>();let count=0
  runInNewContext(source,{window,document:{currentScript:{hasAttribute:()=>true},readyState:'complete'},crypto:{getRandomValues:(bytes:Uint8Array)=>{bytes.fill(++count);return bytes}},Uint8Array,URL,console,
    setTimeout:(fn:unknown)=>{timers.add(fn);return fn},clearTimeout:(fn:unknown)=>timers.delete(fn)})
  await expect(window.archipelagoRental.request({title:'Bad'},{playbackProtocol:2,renewExpired:'yes' as unknown as boolean})).rejects.toThrow('Invalid rental renewal request.')
  expect(parent.postMessage).not.toHaveBeenCalled()
  const renewal=window.archipelagoRental.request({title:'Expired'},{playbackProtocol:2,renewExpired:true})
  const fresh=window.archipelagoRental.request({title:'Fresh'},{playbackProtocol:2,renewExpired:false})
  const omitted=window.archipelagoRental.request({title:'Omitted'},{playbackProtocol:2})
  const messages=parent.postMessage.mock.calls.map(([message])=>message)
  expect(messages).toHaveLength(3)
  expect(messages[0]).toMatchObject({type:'archipelago-rental-request',action:'request',playbackProtocol:2,renewExpired:true})
  expect(messages[1]).toMatchObject({type:'archipelago-rental-request',action:'request',playbackProtocol:2,renewExpired:false})
  expect(messages[2]).toMatchObject({type:'archipelago-rental-request',action:'request',playbackProtocol:2,renewExpired:false})
  for(const message of messages) for(const receive of listeners) receive({source:parent,origin:'http://node.local',data:{type:'archipelago-rental-response',id:message.id,result:{ok:true}}})
  await expect(renewal).resolves.toEqual({ok:true})
  await expect(fresh).resolves.toEqual({ok:true})
  await expect(omitted).resolves.toEqual({ok:true})
  expect(timers.size).toBe(0)
 })
})
