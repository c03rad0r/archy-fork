const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const html = fs.readFileSync('docker/lnd-ui/index.html', 'utf8');
function extract(name) {
  const start = html.indexOf('        function '+name+'(');
  const end = html.indexOf('\n        }', start)+10;
  assert.ok(start >= 0 && end > start);
  return html.slice(start, end);
}
function fixture() {
  const elements = new Map();
  const el = id => { if (!elements.has(id)) elements.set(id,{style:{},textContent:'',className:''}); return elements.get(id) };
  const ctx = {state:{info:null,readiness:null}, document:{getElementById:el}, setText:(id,v)=>el(id).textContent=v,fmtCount:String};
  vm.createContext(ctx);
  vm.runInContext(extract('renderHeader')+'\n'+extract('validBalance'),ctx);
  return {ctx,el};
}
test('missing, starting and syncing Bitcoin each show waiting and recover',()=>{
  const {ctx,el}=fixture();
  for (const [state,message] of [['waiting_install','Waiting for Bitcoin to be installed'],['waiting_start','Waiting for Bitcoin to start'],['waiting_sync','Waiting for Bitcoin to sync']]) {
    ctx.state.readiness={state,message}; ctx.renderHeader();
    assert.equal(el('headerStatusText').textContent,message);
    assert.equal(el('syncCard').style.display,'');
    assert.match(el('syncSubtitle').textContent,/automatically/);
    assert.equal(el('syncPercent').textContent,'');
  }
  ctx.state.readiness={state:'bitcoin_ready'};
  ctx.state.info={synced_to_chain:true,synced_to_graph:true};
  ctx.renderHeader();
  assert.equal(el('headerStatusText').textContent,'Running');
  assert.equal(el('syncCard').style.display,'none');
  ctx.state.info=null;ctx.state.readiness=null;ctx.renderHeader();
  assert.equal(el('headerStatusText').textContent,'Connecting to LND');
});
test('balances reject missing, malformed, fractional and negative values; real zero remains valid',()=>{
  const {ctx}=fixture();
  for (const v of [null,undefined,'',{},false,-1,'-1','garbage',1.5,'1.5',Infinity,Number.MAX_SAFE_INTEGER+1]) assert.equal(ctx.validBalance(v),false,String(v));
  for(const v of [0,'0',123,'123']) assert.equal(ctx.validBalance(v),true,String(v));
});

test('failed and partial balance polls retain known balances and label them stale; recovery clears flags',async()=>{
  const {ctx,el}=fixture();
  el('refreshIcon').classList={add(){},remove(){}};
  const start=html.indexOf('        async function refreshAll()');
  const end=html.indexOf('\n        }',start)+10;
  vm.runInContext(html.slice(start,end),ctx);
  let responses={
    '/v1/getinfo':{synced_to_chain:true},
    '/v1/balance/blockchain':{confirmed_balance:'500',unconfirmed_balance:'0'},
    '/v1/balance/channels':{local_balance:{sat:'250'}},
  };
  ctx.lndSafe=async(path,fallback)=>responses[path]??fallback;
  ctx.renderAll=()=>{};
  await ctx.refreshAll();
  assert.equal(ctx.state.onchain.confirmed_balance,'500');
  assert.equal(ctx.state.chanbal.local_balance.sat,'250');
  responses={};await ctx.refreshAll();
  assert.equal(ctx.state.onchain.confirmed_balance,'500');
  assert.equal(ctx.state.chanbal.local_balance.sat,'250');
  assert.equal(ctx.state.onchainStale,true);assert.equal(ctx.state.chanbalStale,true);
  responses={'/v1/balance/blockchain':{confirmed_balance:'0'},'/v1/balance/channels':{error:'locked'}};
  await ctx.refreshAll();
  assert.equal(ctx.state.onchain.confirmed_balance,'0');
  assert.equal(ctx.state.chanbal.local_balance.sat,'250');
  assert.equal(ctx.state.onchainStale,false);assert.equal(ctx.state.chanbalStale,true);
  responses={'/v1/balance/blockchain':{confirmed_balance:'600'},'/v1/balance/channels':{local_balance:{sat:'300'}}};
  await ctx.refreshAll();
  assert.equal(ctx.state.onchain.confirmed_balance,'600');
  assert.equal(ctx.state.chanbal.local_balance.sat,'300');
  assert.equal(ctx.state.onchainStale,false);assert.equal(ctx.state.chanbalStale,false);
});

test('waiting renders promptly without querying unavailable LND endpoints, then resumes after Bitcoin sync',async()=>{
  const {ctx,el}=fixture();
  el('refreshIcon').classList={add(){},remove(){}};
  const start=html.indexOf('        async function refreshAll()');
  vm.runInContext(html.slice(start,html.indexOf('\n        }',start)+10),ctx);
  let readiness={state:'waiting_sync',message:'Waiting for Bitcoin to sync'};
  const calls=[];let renders=0;
  ctx.lndSafe=async(path,fallback)=>{calls.push(path);return path==='/archy-status'?readiness:fallback};
  ctx.renderAll=()=>{renders++;ctx.renderHeader()};
  await ctx.refreshAll();
  assert.deepEqual(calls,['/archy-status']);
  assert.equal(renders,1);
  assert.equal(el('headerStatusText').textContent,'Waiting for Bitcoin to sync');
  assert.equal(ctx.state.onchain,undefined);
  assert.equal(ctx.state.refreshing,false);
  readiness={state:'bitcoin_ready',message:'Bitcoin is ready'};
  await ctx.refreshAll();
  assert.ok(calls.includes('/v1/getinfo'));
  assert.ok(calls.includes('/v1/balance/blockchain'));
});
