// Real, operator-authorized local IndeeHub login. Only NIP-98 session signatures are allowed.
// Uses isolated browser contexts, does not publish events or perform wallet operations.
const fs=require('fs');const {chromium,expect}=require(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
(async()=>{
 if(process.env.ALLOW_REAL_AUTH_SIGNING!=='1')throw new Error('Set ALLOW_REAL_AUTH_SIGNING=1 only for an authorized real IndeeHub login check');
 const origin=process.env.QUALIFICATION_ORIGIN,cookieFile=process.env.QUALIFICATION_COOKIES,node=process.env.QUALIFICATION_LABEL||'node';
 if(!origin||!cookieFile)throw new Error('Set private node origin and cookie file');
 const u=new URL(origin),octets=u.hostname.split('.').map(Number);
 const privateHost=u.hostname==='localhost'||(octets.length===4&&octets.every(n=>Number.isInteger(n)&&n>=0&&n<=255)&&(octets[0]===127||octets[0]===10||(octets[0]===192&&octets[1]===168)||(octets[0]===172&&octets[1]>=16&&octets[1]<=31)||(octets[0]===100&&octets[1]>=64&&octets[1]<=127)));
 if(!privateHost||!['http:','https:'].includes(u.protocol)||u.username||u.password||u.pathname!=='/'||u.search||u.hash)throw new Error('Refusing non-private node origin');
 u.port='7778';const appOrigin=u.origin;
 const b=await chromium.connectOverCDP(process.env.BROWSER_CDP||'http://127.0.0.1:32911');
 for(const width of [390,1440]){
 const c=await b.newContext({viewport:{width,height:900},serviceWorkers:'block'});
 let page;
 const proof={node,width,stage:'setup',realLogin:true,sessionResponses:[],profileResponses:[],signedAuthRequests:0,blockedUnexpectedRequest:false};
 try{
  const cookies=JSON.parse(fs.readFileSync(cookieFile,'utf8'));await c.addCookies(Object.entries(cookies).map(([name,value])=>({name,value,url:origin,httpOnly:name!=='csrf_token'})));
  await c.addInitScript(()=>{localStorage.setItem('neode-auth','true');localStorage.setItem('lnd-seed-backup-prompt-snooze-until',String(Date.now()+3600000));sessionStorage.setItem('indeedhub_splash_shown','true');});
  await c.route('**/rpc/v1',async route=>{
   let r;try{r=route.request().postDataJSON()}catch{return route.continue()}
   if(['identity.nostr-sign','node.nostr-sign'].includes(r?.method)){
    const e=r.params?.event;let safe=false;try{const u=new URL(e.tags.find(t=>t[0]==='u')[1]);safe=e.kind===27235&&u.origin===appOrigin&&u.pathname==='/api/auth/nostr/session'&&e.tags.some(t=>t[0]==='method'&&t[1]==='POST')}catch{}
    if(!safe){proof.blockedUnexpectedRequest=true;return route.abort()}
    proof.signedAuthRequests++;
   } else if(r?.method==='identity.sign') {
    if(typeof r.params?.message!=='string'||!/^archipelago-identity:\d+$/.test(r.params.message)){proof.blockedUnexpectedRequest=true;return route.abort()}
   } else if(/nostr-(encrypt|decrypt)|\.(pay|send|spend|melt|withdraw)(-|$)/.test(r?.method||'')){
    proof.blockedUnexpectedRequest=true;return route.abort();
   }
   return route.continue();
  });
  const p=await c.newPage();page=p;p.on('response',r=>{const u=new URL(r.url());if(u.origin!==appOrigin)return;if(u.pathname.endsWith('/auth/nostr/session'))proof.sessionResponses.push(r.status());if(u.pathname.endsWith('/auth/me'))proof.profileResponses.push(r.status())});
  await p.goto(origin+'/dashboard/app-session/indeedhub',{waitUntil:'domcontentloaded'});
  proof.stage='identity selection';
  const authenticate=p.getByRole('button',{name:'Authenticate',exact:true});await expect(authenticate).toBeEnabled({timeout:30000});await authenticate.click();
  proof.stage='open app sign-in';
  const frame=p.frameLocator('iframe[src*="7778"]');
  const signIn=frame.getByRole('button',{name:'Sign In',exact:true});await expect(signIn).toBeVisible({timeout:30000});await signIn.click();
  proof.stage='choose native signer';
  const extension=frame.getByRole('button',{name:'Extension',exact:true});
  try{await expect(extension).toBeVisible({timeout:10000});await extension.click()}catch(e){console.log(JSON.stringify({...proof,stage:'auth choices',buttons:await frame.getByRole('button').allTextContents()}));throw e}
  proof.stage='approve login consent';
  const deadline=Date.now()+45000;let approved=0;
  while(Date.now()<deadline){
   if(proof.blockedUnexpectedRequest)throw new Error('An unexpected signing/payment request was blocked');
   if(proof.sessionResponses.some(s=>s===200||s===201)&&proof.profileResponses.includes(200))break;
   const approve=p.getByRole('button',{name:'Approve',exact:true});
   if(await approve.isVisible()){if(++approved>6)throw new Error('Unexpected repeated consent');await approve.click();await p.waitForTimeout(800)}else await p.waitForTimeout(250);
  }
  expect(proof.signedAuthRequests).toBeGreaterThan(0);expect(proof.sessionResponses.some(s=>s===200||s===201)).toBe(true);expect(proof.profileResponses).toContain(200);
  await expect(signIn).toHaveCount(0,{timeout:10000});
  proof.stage='reload authenticated app';
  const profilesBeforeReload=proof.profileResponses.filter(s=>s===200).length;
  await p.reload({waitUntil:'domcontentloaded'});
  await expect.poll(()=>proof.profileResponses.filter(s=>s===200).length,{timeout:30000}).toBeGreaterThan(profilesBeforeReload);
  await expect(frame.getByRole('button',{name:'Sign In',exact:true})).toHaveCount(0,{timeout:10000});
  proof.reloadRetainedLogin=true;proof.result='PASS';console.log(JSON.stringify(proof));
 }catch(e){console.log(JSON.stringify({...proof,result:'FAIL',path:page?new URL(page.url()).pathname:null,buttons:page?await page.getByRole('button').allTextContents():[]}));if(page)await page.screenshot({path:'/tmp/archy-indeehub-login-'+node+'-'+width+'-failure.png'});throw e}finally{await c.close()}
 }
 process.exit(0)
})().catch(e=>{console.error(String(e.message).split('Call log:')[0]);process.exit(1)});
