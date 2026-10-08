// Run against the disposable packaged UI forwarded to 127.0.0.1:48191.
// The signer and upload transport are mocked; no real keys or public endpoints.
const { chromium } = require('../../../neode-ui/node_modules/@playwright/test');
const { createHash } = require('node:crypto');
(async () => {
 const browser = await chromium.launch({headless:true});
 const page = await browser.newPage({viewport:{width:390,height:844}});
 page.on('console',m=>console.log('browser:',m.text())); page.on('pageerror',e=>console.log('page error:',e.message));
 const outgoing=[]; let uploads=0;
 await page.route('**/*', async route => {
  const u=new URL(route.request().url());
  if(u.origin!=='http://127.0.0.1:48191'){outgoing.push(u.origin);return route.abort();}
  if(u.pathname==='/nostr-provider.js')return route.fulfill({contentType:'application/javascript',body:`window.signCalls=[];window.chooseCalls=0;window.deny=true;window.archipelagoNostr={selectIdentity:async()=>{window.chooseCalls++}};window.nostr={getPublicKey:async()=>'${'a'.repeat(64)}',signEvent:async e=>{window.signCalls.push(e);if(window.deny)throw new Error('User declined signing');return {...e,pubkey:'${'a'.repeat(64)}',id:'${'b'.repeat(64)}',sig:'${'c'.repeat(128)}'}}};`});
  if(u.pathname==='/upload'){
   uploads++; const body=route.request().postDataBuffer();
   const token=JSON.parse(Buffer.from(route.request().headers().authorization.slice(6),'base64').toString());
   const hash=createHash('sha256').update(body).digest('hex');
   if(!token.tags.some(t=>t[0]==='x'&&t[1]===hash)||!token.tags.some(t=>t[0]==='server'&&t[1]==='127.0.0.1'))throw Error('Auth scope mismatch');
   return route.fulfill({contentType:'application/json',body:JSON.stringify({sha256:hash,size:body.length})});
  }
  return route.continue();
 });
 await page.goto('http://127.0.0.1:48191/');
 await page.waitForFunction(()=>typeof window.nostr==='object');
 if(!await page.locator('#upload').isDisabled())throw Error('Upload enabled before consent');
 await page.locator('#identity').click();
 await page.waitForFunction(()=>document.querySelector('#pubkey').textContent==='a'.repeat(64));
 await page.locator('#file').setInputFiles({name:'local-fixture.txt',mimeType:'text/plain',buffer:Buffer.from('Synthetic local file')});
 await page.locator('#approve').check(); await page.locator('#upload').click();
 await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('User declined'));
 if(uploads!==0)throw Error('Uploaded despite signing refusal');
 await page.evaluate(()=>window.deny=false);
 await page.locator('#upload').click();
 await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('Stored on this node'));
 if(uploads!==1 || outgoing.length)throw Error('Unexpected upload or external request');
 if(await page.locator('#approve').isChecked())throw Error('Approval was retained after upload');
 if(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth))throw Error('Mobile horizontal overflow');
 console.log('PASS packaged local UI: identity chooser, explicit consent, signer denial, scoped upload, consent reset, mobile width, no external requests (mock signer/transport; real signer still pending)');
 await browser.close();
})().catch(e=>{console.error(e);process.exit(1)});
