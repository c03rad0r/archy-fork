// Disposable V4V fixture only: never point this test at a paid production catalog.
// See docs/container-store-ownership-followup.md for fixture isolation requirements.
const fs = require('node:fs');
const { chromium, expect } = require('../../neode-ui/node_modules/@playwright/test');
const cdp = process.env.ARCHY_TEST_CDP;
const passwordFile = process.env.V4V_TEST_PASSWORD_FILE;
const fixtureOrigin = new URL(process.env.V4V_TEST_ORIGIN || 'http://127.0.0.1:32908').origin;
const parentOrigin = new URL(process.env.ARCHY_TEST_PARENT_ORIGIN || 'http://127.0.0.1').origin;
if (!cdp || !passwordFile) throw Error('ARCHY_TEST_CDP and V4V_TEST_PASSWORD_FILE are required');
if (new URL(parentOrigin).hostname !== new URL(fixtureOrigin).hostname) throw Error('Fixture and parent must use the same hostname');
const fixturePassword = fs.readFileSync(passwordFile, 'utf8').trim();
(async()=>{const browser=await chromium.connectOverCDP(cdp);const contexts=[];
try{for(const width of [390,1440]){
 const context=await browser.newContext({viewport:{width,height:900},serviceWorkers:'block'});contexts.push(context); context.setDefaultTimeout(15000); context.setDefaultNavigationTimeout(30000); console.log("stage",width,"context");
 await context.addInitScript(()=>{window.__qualificationMedia=[];const play=HTMLMediaElement.prototype.play;HTMLMediaElement.prototype.play=function(...args){this.muted=true;if(!window.__qualificationMedia.includes(this))window.__qualificationMedia.push(this);return play.apply(this,args)}});
 const response=await context.request.post(fixtureOrigin + '/api/alpha/login',{data:{password:fixturePassword},timeout:30000});if(!response.ok())throw Error('Fixture login failed HTTP '+response.status());
 const harness=`<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"></head><body style="margin:0"><iframe title="V4V fixture" src="${fixtureOrigin}/" allow="autoplay; fullscreen" style="border:0;width:100vw;height:100vh"></iframe><script>window.session=crypto.randomUUID();window.mediaStates=[];const frame=document.querySelector('iframe');window.control=(command,position)=>frame.contentWindow.postMessage({type:'archipelago:media-control',version:1,session:window.session,command,position},${JSON.stringify(fixtureOrigin)});window.addEventListener('message',event=>{if(event.source!==frame.contentWindow||event.origin!==${JSON.stringify(fixtureOrigin)})return;if(event.data?.type==='archipelago:media-ready')frame.contentWindow.postMessage({type:'archipelago:media-connect',version:1,session:window.session},event.origin);if(event.data?.type==='archipelago:media-state'&&event.data.session===window.session){window.latestMediaState=event.data;window.mediaStates.push(event.data)}});<\/script></body></html>`;
 console.log('stage',width,'authenticated');const page=await context.newPage();page.on('pageerror',e=>console.log('pageerror',e.message.slice(0,200)));page.on('requestfailed',r=>console.log('requestfailed',new URL(r.url()).pathname,r.failure()?.errorText));await page.goto(parentOrigin + '/robots.txt',{waitUntil:'domcontentloaded',timeout:30000});await page.setContent(harness,{waitUntil:'domcontentloaded'});console.log('stage',width,'harness');
 const frame=page.frameLocator('iframe');const later=frame.getByRole('button',{name:'Not now',exact:true});
 await expect.poll(()=>page.frames().some(f=>f.url().startsWith(fixtureOrigin + '/')),{timeout:15000}).toBe(true);
 const app=page.frames().find(f=>f.url().startsWith(fixtureOrigin + '/'));if(!app)throw Error('App frame missing');
 await app.waitForFunction(()=>window.PulsewireLegacy?.getReactBridge?.(),undefined,{timeout:15000,polling:100});
 if(await later.isVisible())await later.click();
 await app.evaluate(()=>{const c=window.PulsewireLegacy.getReactBridge();const rows=c.getState().releases;const r=rows.find(r=>{try{return new URL(c.helpers.releasePlayableUrl(r),location.href).pathname.startsWith('/media/dev-audio/')}catch{return false}});if(!r)throw Error('No bundled playable track; paths: '+JSON.stringify(rows.slice(0,5).map(r=>({id:r.id,path:new URL(c.helpers.releasePlayableUrl(r)||'/',location.href).pathname}))));c.actions.play(r.id)});
 console.log('stage',width,'bundled track selected through app action');
 try { await page.waitForFunction(()=>window.latestMediaState?.available&&window.latestMediaState?.playing,undefined,{timeout:15000,polling:100}); } catch(e) { console.log('parent_diagnostic',JSON.stringify(await page.evaluate(()=>({latest:window.latestMediaState,states:window.mediaStates?.length}))));throw e; } console.log('stage',width,'parent playing');
 const appFrame=page.frames().find(f=>f.url().startsWith(fixtureOrigin + '/'));if(!appFrame)throw Error('App frame missing');
 try { await appFrame.waitForFunction(()=>window.__qualificationMedia.some(a=>!a.paused&&a.readyState>=2&&a.currentTime>0),undefined,{timeout:15000,polling:100}); } catch(e) {
 console.log('media_diagnostic',JSON.stringify(await appFrame.evaluate(()=>({media:window.__qualificationMedia?.map(a=>({paused:a.paused,time:a.currentTime,ready:a.readyState,error:a.error?.code,network:a.networkState,src:a.currentSrc?new URL(a.currentSrc).pathname:null})),elapsed:window.PulsewireLegacy?.getReactBridge?.().getState()?.ui?.elapsed,playing:window.PulsewireLegacy?.getReactBridge?.().getState()?.ui?.playing,title:window.PulsewireLegacy?.getReactBridge?.().helpers.activePlayerRelease()?.title}))));throw e;
 }
 console.log('stage',width,'actual media started');const before=await appFrame.evaluate(()=>window.__qualificationMedia.find(a=>!a.paused).currentTime);
 await page.locator('iframe').evaluate(el=>el.style.display='none');
 await appFrame.waitForFunction(t=>window.__qualificationMedia.some(a=>!a.paused&&a.currentTime>t+1),before,{timeout:15000,polling:100});
 console.log('stage',width,'hidden playback advanced');await page.evaluate(()=>window.control('pause'));await page.waitForFunction(()=>window.latestMediaState?.playing===false,undefined,{timeout:10000,polling:100});
 await appFrame.waitForFunction(()=>window.__qualificationMedia.every(a=>a.paused),undefined,{timeout:10000,polling:100});
 console.log('stage',width,'pause confirmed');await page.evaluate(()=>window.control('play'));await page.waitForFunction(()=>window.latestMediaState?.playing===true,undefined,{timeout:10000,polling:100});
 console.log('stage',width,'resume state');await page.locator('iframe').evaluate(el=>el.style.display='block');
 if(!page.frames().includes(appFrame))throw Error('App iframe recreated');
 await appFrame.waitForFunction(t=>window.__qualificationMedia.some(a=>!a.paused&&a.currentTime>t+1),before,{timeout:15000,polling:100});
 const state=await page.evaluate(()=>({title:window.latestMediaState.title,artist:window.latestMediaState.artist,playing:window.latestMediaState.playing,position:window.latestMediaState.position}));
 await page.evaluate(()=>window.control('pause'));
 console.log(JSON.stringify({passed:true,width,scope:'final isolated V4V image; real demo audio decode/time; parent bridge harness',hidden_playback:true,pause_resume:true,same_iframe:true,muted_for_testing:true,state}));
 await context.unrouteAll({behavior:'ignoreErrors'});await context.close();
}}finally{for(const c of contexts){await c.unrouteAll({behavior:'ignoreErrors'}).catch(()=>{});await c.close().catch(()=>{})}}process.exit(0)})().catch(e=>{console.error(String(e.message).split('Call log:')[0]);process.exit(1)});
