// Served-dashboard qualification. Signing responses are isolated fixtures; no real keys or payments.
const fs=require('fs');
const {chromium,expect}=require(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
(async()=>{
 const node=process.env.QUALIFICATION_LABEL || 'qualification-node';
 const origin=process.env.QUALIFICATION_ORIGIN, cookieFile=process.env.QUALIFICATION_COOKIES;
 if(!origin || !cookieFile)throw new Error('Set QUALIFICATION_ORIGIN and QUALIFICATION_COOKIES for an authorized private node');
 const url=new URL(origin), octets=url.hostname.split('.').map(Number);
 const privateHost=url.hostname==='localhost' || (octets.length===4 && octets.every(n=>Number.isInteger(n)&&n>=0&&n<=255)
  && (octets[0]===127 || octets[0]===10 || (octets[0]===192&&octets[1]===168)
   || (octets[0]===172&&octets[1]>=16&&octets[1]<=31) || (octets[0]===100&&octets[1]>=64&&octets[1]<=127)));
 if(!privateHost || !['http:','https:'].includes(url.protocol) || url.username || url.password)throw new Error('Refusing to send qualification cookies outside a private node');
 const cookies=JSON.parse(fs.readFileSync(cookieFile,'utf8'));
 const browser=await chromium.connectOverCDP(process.env.BROWSER_CDP || 'http://127.0.0.1:32911');
 for(const width of [390,1440]){
  const context=await browser.newContext({viewport:{width,height:900},serviceWorkers:'block'});
  try{
   await context.addCookies(Object.entries(cookies).map(([name,value])=>({name,value,url:origin,httpOnly:name!=='csrf_token'})));
   await context.addInitScript(()=>{
    localStorage.setItem('neode-auth','true');
    localStorage.removeItem('archipelago_nostr_consent_v2');
    localStorage.setItem('archipelago_app_identity_indeedhub',JSON.stringify({id:'qualification-only',name:'Qualification identity',did:'did:key:qualification-only',pubkey:'a'.repeat(64),nostr_pubkey:'b'.repeat(64)}));
   });
   let signed=0, frameLoads=0; const signedContents=[];
   await context.route('**/rpc/v1',async route=>{
    let request;try{request=route.request().postDataJSON()}catch{return route.continue()}
    if(request?.method==='identity.sign'&&request.params?.id==='qualification-only'){
     return route.fulfill({contentType:'application/json',body:JSON.stringify({jsonrpc:'2.0',id:request.id,result:{signature:'qualification-fixture'}})});
    }
    if(request?.method==='identity.nostr-sign'){
     if(request.params?.id!=='qualification-only'||!['qualification-first','qualification-second'].includes(request.params?.event?.content))throw new Error('Unexpected signer request in isolated fixture');
     signed++; signedContents.push(request.params.event.content);
     return route.fulfill({contentType:'application/json',body:JSON.stringify({jsonrpc:'2.0',id:request.id,result:{id:'a'.repeat(64),content:request.params.event.content}})});
    }
    return route.continue();
   });
   await context.route('**:7778/**',async route=>{
    if(route.request().resourceType()!=='document')return route.abort();
    frameLoads++;
    return route.fulfill({contentType:'text/html',body:`<!doctype html><html><body><p id="result">waiting</p><script>
     const responses=[];
     addEventListener('message',e=>{if(e.data?.type==='nostr-response'){responses.push({id:e.data.id,ok:!!e.data.result,error:e.data.error});document.querySelector('#result').textContent=JSON.stringify(responses)}});
     setTimeout(()=>{for(const id of ['first','second'])parent.postMessage({type:'nostr-request',id,method:'signEvent',params:{event:{kind:27235,content:'qualification-'+id}}},${JSON.stringify(origin)})},500);
    </script></body></html>`});
   });
   const page=await context.newPage();
   await page.goto(origin+'/dashboard/app-session/indeedhub',{waitUntil:'domcontentloaded'});
   const approve=page.getByRole('button',{name:'Approve',exact:true});
   try { await expect(approve).toBeVisible({timeout:30000}); } catch(error) {
    console.log(JSON.stringify({node,width,path:new URL(page.url()).pathname,headings:await page.locator('h1').allTextContents(),frames:await page.locator('iframe').evaluateAll(items=>items.map(item=>{const u=new URL(item.src);return {origin:u.origin,path:u.pathname}}))}));
    throw error;
   }
   await approve.click();
   await expect.poll(()=>signed,{timeout:10000}).toBe(1);
   await expect(approve).toBeVisible({timeout:10000});
   await approve.click();
   await expect.poll(()=>signed,{timeout:10000}).toBe(2);
   const frame=page.frameLocator('iframe[src*="7778"]');
   await expect(frame.locator('#result')).toContainText('"id":"first","ok":true');
   try { await expect(frame.locator('#result')).toContainText('"id":"second","ok":true'); } catch(error) { console.log(JSON.stringify({node,width,frameLoads,signedContents})); throw error; }
   await page.waitForTimeout(800); // allow the required 675ms completion presentation to finish
   await expect(approve).toHaveCount(0);
   expect(frameLoads).toBe(1);
   expect(signedContents).toEqual(['qualification-first','qualification-second']);
   console.log(JSON.stringify({node,width,result:'PASS',scope:'served dashboard, concurrent iframe requests and consent responses; signing RPC isolated with fixtures; no real signing/payment'}));
  }finally{await context.close()}
 }
 process.exit(0);
})().catch(e=>{console.error(String(e.message).split('Call log:')[0]);process.exit(1)});
