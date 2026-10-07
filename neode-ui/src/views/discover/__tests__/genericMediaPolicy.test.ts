import {afterEach,describe,expect,it,vi} from 'vitest'
import {__setSignedCatalogForTests,appHasMediaBridge,appRequiresHostFrame,ensureAppLaunchPolicy} from '../curatedApps'
afterEach(()=>{__setSignedCatalogForTests(null);vi.unstubAllGlobals()})
describe('generic manifest media eligibility',()=>{
 it('accepts a verified ordinary catalog app at its installed version',()=>{
  __setSignedCatalogForTests({apps:{'public-audio':{version:'2',manifest:{app:{id:'public-audio',metadata:{launch:{media_controls:'archipelago-v1',requires_host_frame:true}}}}}}})
  expect(appHasMediaBridge('public-audio','2')).toBe(true);expect(appRequiresHostFrame('public-audio','2')).toBe(true)
  expect(appHasMediaBridge('public-audio','1')).toBe(false)
 })
 it('does not grant a public catalog authority over node-only demos or conflicting launch policy',()=>{
  __setSignedCatalogForTests({apps:{'node-demo-injected':{version:'1',manifest:{app:{id:'node-demo-injected',metadata:{launch:{media_controls:'archipelago-v1'}}}}},bad:{version:'1',manifest:{app:{id:'bad',metadata:{launch:{media_controls:'archipelago-v1',open_in_new_tab:true}}}}}}})
  expect(appHasMediaBridge('node-demo-injected','1')).toBe(false);expect(appHasMediaBridge('bad','1')).toBe(false)
 })
 it('loads only the authenticated verified endpoint before first launch',async()=>{
  const fetcher=vi.fn(async(_url:string,_init?:RequestInit)=>({ok:true,json:async()=>({apps:{music:{version:'1',manifest:{app:{id:'music',metadata:{launch:{media_controls:'archipelago-v1'}}}}}}})}))
  vi.stubGlobal('fetch',fetcher);await Promise.all([ensureAppLaunchPolicy(),ensureAppLaunchPolicy()])
  expect(fetcher).toHaveBeenCalledTimes(1);expect(fetcher.mock.calls[0]?.[0]).toBe('/api/app-catalog');expect(appHasMediaBridge('music','1')).toBe(true)
 })
})
