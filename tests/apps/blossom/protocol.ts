// Run inside a disposable Blossom container with ONLY the synthetic profile below allowed.
// No real identity, external server or public relay is used. Retains one fixture for lifecycle checks.
import { finalizeEvent, getPublicKey } from 'nostr-tools';
const base = 'http://127.0.0.1:3000';
const key = new Uint8Array(32).fill(1);
const other = new Uint8Array(32).fill(2);
const body = '<!doctype html><script>throw new Error("must not execute")</script><p>Blossom qualification, synthetic data only.</p>';
const bytes = new TextEncoder().encode(body);
const hash = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), x => x.toString(16).padStart(2,'0')).join('');
function auth(action: string, secret=key, server='127.0.0.1', expires=300) {
 const now = Math.floor(Date.now()/1000);
 return 'Nostr ' + btoa(JSON.stringify(finalizeEvent({ kind:24242,created_at:now,content:'Local synthetic qualification only',tags:[['t',action],['x',hash],['server',server],['expiration',String(now+expires)]]},secret)));
}
async function check(label: string, expected: number, path: string, init={}) {
 const r=await fetch(base+path,init);
 if(r.status!==expected) throw new Error(`${label}: expected ${expected}, got ${r.status}: ${await r.text()}`);
 console.log(`PASS ${label}: ${r.status}`);return r;
}
const upload=(token?:string)=>({method:'PUT',headers:{'content-type':'text/html',...(token?{authorization:token}:{})},body});
await check('unauthenticated upload denied',401,'/upload',upload());
await check('unlisted identity denied',401,'/upload',upload(auth('upload',other)));
await check('wrong host denied',401,'/upload',upload(auth('upload',key,'wrong.invalid')));
await check('expired token denied',401,'/upload',upload(auth('upload',key,'127.0.0.1',-300)));
const stored=await (await check('signed profile upload',201,'/upload',upload(auth('upload')))).json();
if(stored.sha256!==hash || stored.size!==bytes.length) throw new Error('Wrong descriptor');
const read=await check('read stored bytes',200,'/'+hash);
if(await read.text()!==body) throw new Error('Stored bytes differ');
if(!read.headers.get('content-security-policy')?.includes('sandbox') || read.headers.get('content-disposition')!=='attachment') throw new Error('Active content not sandboxed');
console.log('PASS exact bytes and sandboxed attachment');
await check('anonymous list denied',401,'/list/'+getPublicKey(key));
await check('other identity cannot list owner',403,'/list/'+getPublicKey(key),{headers:{authorization:auth('list',other)}});
await check('owner list',200,'/list/'+getPublicKey(key),{headers:{authorization:auth('list')}});
await check('mirror disabled',403,'/mirror',{method:'PUT',headers:{authorization:auth('upload')}});
await check('canonical signer provider',200,'/nostr-provider.js');
await check('health',200,'/healthz');
console.log('PRESERVE_HASH '+hash);
