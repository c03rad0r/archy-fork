// Local component fixtures only. Start loopback Vite on32915; never use a live node URL.
const fs=require('node:fs');const path=require('node:path');
const repo=path.resolve(__dirname,'../..');
const fixtureDir=path.join(repo,'neode-ui/.qualification');const fixturePath=path.join(fixtureDir,'native-payment-fixture.js');
const {chromium,expect}=require(path.join(repo,'neode-ui/node_modules/@playwright/test'));
const origin='http://127.0.0.1:32915';
const boot=String.raw`
import {createApp,h,reactive,nextTick} from 'vue';
import {createPinia} from 'pinia';
import {createRouter,createMemoryHistory} from 'vue-router';
import Rental from '/src/components/RentalPurchaseConsent.vue';
import Signer from '/src/components/NostrSignConsent.vue';
import PeerFiles from '/src/views/PeerFiles.vue';
import {rpcClient} from '/src/api/rpc-client.ts';
import '/src/style.css';
window.fixtureCalls=[];
rpcClient.call=async ({method})=>{window.fixtureCalls.push(method);if(method==='content.purchase-pending')return {attempts:[]};if(method==='wallet.ecash-balance')return {cashu_sats:100,fedimint_sats:0,ark_sats:0};return {items:[]}};
rpcClient.federationListNodes=async()=>({nodes:[]});
rpcClient.payLightningInvoice=async()=>{throw Error('No fixture payment is permitted')};
const quote={network:'testnet',mint_url:'https://original-mint-with-long-name.example.test/cashu',wallet_debit_sats:10,gross_token_sats:9,seller_net_sats:8,operation_id:'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',envelope_sha256:'b'.repeat(64),expires_at:2000000000};
window.layout=reactive({kind:'rental',phase:'confirm',error:'',signing:false});
const offer={title:'A long film title with a story about independent creators',terms:{nodeDid:'did:key:fixture',contentId:'registered_fixture',sha256:'a'.repeat(64),priceSats:8,viewingSeconds:3600}};
const rental=createApp({render(){return h('div',{style:'position:fixed;inset:24px 0 0'},[
!window.layout.signing&&h(Rental,{request:offer,quote,phase:window.layout.phase,error:window.layout.error,onCancel:()=>window.layout.cancelled=true}),
h(Signer,{show:window.layout.signing,appName:'Fixture IndeeHub',method:'signEvent',phase:'review',content:'Fixture signature; no RPC',onDeny:()=>window.layout.signing=false})])}}).mount('#rental');
const router=createRouter({history:createMemoryHistory(),routes:[]});
let peerApp=null,vm=null;
window.showFixture=async(kind,phase,error='')=>{
 document.querySelector('#rental').style.display=kind==='rental'?'block':'none';document.querySelector('#peer').style.display=kind==='peer'?'block':'none';
 window.layout.phase=phase;window.layout.error=error;window.layout.signing=phase==='signer';
 if(vm)vm.closePayModal();
 if(kind!=='peer'&&peerApp){peerApp.unmount();peerApp=null;vm=null}
 if(kind==='peer'){
 if(!peerApp){peerApp=createApp(PeerFiles,{peerId:'fixture.onion'}).use(createPinia()).use(router);const peer=peerApp.mount('#peer');vm=peer.$.setupState;window.peerVm=vm;}
 vm.openPayModal({id:'fixture-file',filename:'A meaningful independent documentary.mp4',mime_type:'video/mp4',size_bytes:1024,access:{paid:{price_sats:8,accepted:['ecash']}}});
 vm.payMode='ecash-confirm';vm.ecashPlan={cashu:100,fedimint:0,ark:0,total:100,chosen:'cashu'};vm.cashuQuote=quote;vm.cashuRecoveryRequired=true;vm.purchaseError=error;vm.ecashPreparing=phase==='paying';
 }
 await nextTick();
};
await window.showFixture('rental','confirm');window.fixtureReady=true;
`;
fs.mkdirSync(fixtureDir,{recursive:true});fs.writeFileSync(fixturePath,boot);
(async()=>{const browser=await chromium.launch({headless:true});try{for(const width of [320,390,1440]){const context=await browser.newContext({viewport:{width,height:width===320?568:844},reducedMotion:'reduce'});let blocked=0;const errors=[];await context.routeWebSocket('**/*',socket=>socket.close());
 await context.route('**/*',r=>{const u=new URL(r.request().url());if(u.origin!==origin||u.pathname==='/rpc/v1'){blocked++;return r.abort()}return r.continue()});
 await context.route(origin+'/native-payment-fixture',r=>r.fulfill({contentType:'text/html',body:'<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"></head><body style="background:#11151a"><div style="position:fixed;top:0;z-index:999;color:white;background:#553;font:12px sans-serif;width:100%;height:24px">LOCAL LAYOUT FIXTURE · no payment or live wallet</div><div id="rental"></div><div id="peer"></div><script type="module" src="/.qualification/native-payment-fixture.js"></script></body></html>'}));
 const page=await context.newPage();page.on('pageerror',e=>{errors.push(e.message);console.error('FIXTURE PAGE ERROR',e.message)});page.on('response',r=>{if(r.status()>=400)console.error('FIXTURE RESPONSE',r.status(),new URL(r.url()).pathname)});await page.goto(origin+'/native-payment-fixture',{waitUntil:'domcontentloaded',timeout:60000});await page.waitForFunction(()=>window.fixtureReady,undefined,{timeout:30000});
 for(const [kind,phase,error] of [['rental','confirm',''],['rental','paying',''],['rental','review','The response was lost. Recover the original purchase without paying again.'],['rental','signer',''],['peer','confirm',''],['peer','paying',''],['peer','confirm','Payment status could not be confirmed. Keep the original receipt and retry recovery.']]){
 await page.evaluate(([k,p,e])=>window.showFixture(k,p,e),[kind,phase,error]);
 await page.evaluate(async()=>{await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));await Promise.all(document.getAnimations().filter(a=>a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished.catch(()=>{})))});
 if(kind==='rental'&&phase==='signer'){await expect(page.getByRole('dialog',{name:'Confirm video rental'})).toHaveCount(0);await expect(page.getByRole('dialog')).toHaveCount(1)}
 if(phase!=='signer'){await expect(page.getByText('Cashu · Testnet',{exact:false}).locator('visible=true').first()).toBeVisible();await expect(page.getByText('Mint: https://original-mint-with-long-name.example.test/cashu',{exact:true}).locator('visible=true').first()).toBeVisible()}
 if(phase!=='signer')await page.getByRole('button',{name:kind==='peer'?(phase==='paying'?'Working…':'Pay'):(phase==='confirm'?'Pay 10 sats':'Check purchase'),exact:true}).scrollIntoViewIfNeeded().catch(()=>{});
 const result=await page.evaluate(()=>({height:innerHeight,overflow:document.documentElement.scrollWidth>innerWidth,buttons:[...document.querySelectorAll('button')].filter(b=>b.getBoundingClientRect().width>0&&getComputedStyle(b).visibility!=='hidden').map(b=>({text:b.textContent.trim(),disabled:b.disabled,x:b.getBoundingClientRect().x,y:b.getBoundingClientRect().y,w:b.getBoundingClientRect().width,h:b.getBoundingClientRect().height}))}));
 if(result.overflow)throw Error('horizontal overflow '+width+' '+kind+' '+phase);
 const actions=result.buttons.filter(b=>/^(Close|Pay|Pay 10 sats|Working…|Cancel unpaid quote|Back|Check purchase)$/.test(b.text));
 for(const b of actions)if(b.x<0||b.x+b.w>width+1||b.y<24||b.y+b.h>result.height)throw Error('clipped footer '+JSON.stringify({width,kind,phase,b}));
 if(phase==='paying'&&actions.some(b=>b.text!=='Close'&&!b.disabled))throw Error('Busy action enabled');
 const screenshot='/tmp/archy-payment-layout-'+width+'-'+kind+'-'+phase+(error?'-error':'')+'.png';await page.screenshot({path:screenshot});fs.chmodSync(screenshot,0o600);
 console.log(JSON.stringify({scope:'LOCAL COMPONENT FIXTURE ONLY',width,kind,phase,error:!!error,footerFits:true,buttons:actions}));
 }
 if(errors.length)throw Error(errors.join('\n'));console.log(JSON.stringify({width,blockedNetworkRequests:blocked,result:'PASS fixture layouts only'}));await context.close();
}}finally{await browser.close();fs.rmSync(fixturePath,{force:true})}})().catch(e=>{console.error(e.stack);process.exit(1)});
