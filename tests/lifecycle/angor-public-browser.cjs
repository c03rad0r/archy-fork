#!/usr/bin/env node
/* Opt-in, read-only public acceptance. Uses a disposable browser profile; no wallet
 * actions, transaction broadcast, relay publication, or browser TLS bypass. */
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const path = require('node:path');
const { createRequire } = require('node:module');
const requireUi = createRequire(path.resolve(__dirname, '../../neode-ui/package.json'));
const { chromium } = requireUi('playwright');
const WebSocket = requireUi('ws');

const indexer = process.env.ANGOR_TEST_INDEXER;
const relay = process.env.ANGOR_TEST_RELAY;
const projectId = 'angor1qryhse38vcyqnp0j6976q9f00a9jpj2ary03nlc';
const address = 'bc1qryhse38vcyqnp0j6976q9f00a9jpj2ar62vfzd';
const fundingTx = '72d14227b78a260c9410a65585cb49e81e35eaa95b1b23ec702eb0512ca016a7';
const eventId = 'adbf94d6152097f3d503cd68cc00dee34c1e1007914c00cfc3d6698d1ee01834';
const relays = JSON.parse(process.env.ANGOR_TEST_RELAYS || JSON.stringify([relay]));
const app = process.env.ANGOR_TEST_APP || 'https://angor.io';
assert(indexer && new URL(indexer).protocol === 'https:', 'Set ANGOR_TEST_INDEXER to an HTTPS origin');
assert(relay && new URL(relay).protocol === 'wss:', 'Set ANGOR_TEST_RELAY to a WSS origin');
assert(relays.some(url => new URL(url).href === new URL(relay).href),
  'Discovery relays must include the relay under test');

async function read(endpoint, json = true) {
  const response = await fetch(new URL(endpoint, indexer), { signal: AbortSignal.timeout(15000) });
  assert.equal(response.status, 200, `${endpoint}: ${response.status}`);
  return json ? response.json() : response.text();
}
function relayEvent() {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(relay, { handshakeTimeout: 15000, origin: new URL(app).origin });
    const timer = setTimeout(() => finish(Error('Relay project event timeout')), 15000);
    function finish(err, value) {
      clearTimeout(timer);
      ws.removeAllListeners();
      ws.on('error', () => {});
      ws.close();
      err ? reject(err) : resolve(value);
    }
    ws.on('error', err => finish(err));
    ws.on('open', () => ws.send(JSON.stringify(['REQ', 'acceptance', { ids: [eventId] }])));
    ws.on('message', raw => {
      try {
        const m = JSON.parse(raw.toString());
        if (m[0] === 'EVENT' && m[1] === 'acceptance') finish(null, m[2]);
        if (m[0] === 'EOSE') finish(Error('Relay lacks the known signed project fixture; provision public history first'));
        if (m[0] === 'CLOSED') finish(Error(`Relay subscription rejected: ${m[2]}`));
      } catch (err) { finish(err); }
    });
  });
}

(async () => {
  assert.match((await read('/health', false)).trim(), /^\d+$/);
  assert(Number.isFinite((await read('/api/v1/fees/recommended')).fastestFee));
  const txs = await read(`/api/v1/address/${address}/txs`);
  const tx = txs.find(t => t.txid === fundingTx);
  assert(tx?.status.confirmed, 'Known project funding transaction missing');
  const event = await relayEvent();
  assert.equal(event.id, eventId);
  assert.equal(event.kind, 3030);
  assert.equal(createHash('sha256').update(JSON.stringify([0, event.pubkey, event.created_at,
    event.kind, event.tags, event.content])).digest('hex'), eventId);
  assert(tx.vout.some(v => v.scriptpubkey_type === 'op_return' && v.scriptpubkey.includes(eventId)),
    'Project announcement must match the immutable funding commitment');
  assert.equal(JSON.parse(event.content).projectIdentifier, projectId);
  // This checks a known immutable fixture's hash/chain binding, not a general
  // Schnorr verifier. Signed publish and invalid-signature rejection are separate gates.
  console.log('PASS public TLS, indexed funding data, relay WSS, immutable project/event commitment');

  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const failures = [];
    let browserIndexerRead = false;
    page.on('requestfailed', r => {
      if (r.url().startsWith(new URL(indexer).origin)) failures.push(`${r.url()}: ${r.failure()?.errorText}`);
    });
    page.on('response', r => {
      if (r.url().startsWith(new URL(indexer).origin) && r.url().includes(`/address/${address}/txs`) && r.status() === 200)
        browserIndexerRead = true;
    });
    await page.addInitScript(({ indexer, relays }) => {
      localStorage.setItem('angor-network', 'main');
      localStorage.setItem('angor-indexers', JSON.stringify({ mainnet: [{ url: indexer.replace(/\/?$/, '/'), isPrimary: true }], testnet: [] }));
      localStorage.setItem('angor-hub-relays', JSON.stringify(relays));
    }, { indexer, relays });
    await page.goto(new URL('/explore', app).href, { waitUntil: 'domcontentloaded' });
    const link = page.locator(`a[href*="/project/${projectId}"]`).first();
    await link.waitFor({ state: 'visible', timeout: 60000 });
    assert(browserIndexerRead, 'Browser did not validate the project using the configured indexer');
    assert.deepEqual(failures, [], 'Browser indexer/CORS requests failed');
    console.log('PASS official Explore page discovers the known project with the configured services');
    await page.goto(new URL(`/project/${projectId}`, app).href, { waitUntil: 'domcontentloaded' });
    await page.getByText(projectId, { exact: true }).first().waitFor({ state: 'visible', timeout: 60000 });
    await page.getByText('Project Statistics', { exact: true }).waitFor({ state: 'visible', timeout: 30000 });
    assert.deepEqual(failures, [], 'Browser detail/indexer requests failed');
    console.log('PASS official project detail and statistics render; no funds sent');
  } finally {
    await browser.close();
  }
})().catch(err => { console.error(err); process.exitCode = 1; });
