#!/usr/bin/env node
// Demo/real RPC parity harness (FED-04).
//
// STATIC stage: cross-references every mesh.*/federation.* RPC method the UI
// actually calls against the cases mock-backend.js implements, and reports any
// the demo would answer with "Method not found".
//
// LIVE stage: boots mock-backend.js on an ephemeral port and drives a scripted
// JSON-RPC sequence, asserting the handlers mutate demo state rather than
// returning bare acknowledgements.
//
// Exits non-zero on any failure. There are deliberately no `|| fallback`
// escapes anywhere in here: a failed spawn, a failed fetch or a missing field
// must fail the run, never produce a green run that measured nothing.

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawn } from 'node:child_process'
import { createServer } from 'node:net'

const HERE = dirname(fileURLToPath(import.meta.url))
const UI_ROOT = resolve(HERE, '..')
const MOCK = join(UI_ROOT, 'mock-backend.js')
const SRC = join(UI_ROOT, 'src')

// Methods the UI calls that the mock is knowingly allowed to be missing.
// Task 2 emptied this: any gap now fails the run.
const KNOWN_GAPS = []

let failures = 0
const fail = (msg) => {
  console.error(`  ✗ ${msg}`)
  failures++
}
const pass = (msg) => console.log(`  ✓ ${msg}`)

// ── STATIC ──────────────────────────────────────────────────────────────────
// Only `method: 'mesh.x'` / `method: "mesh.x"` counts as a call site. A bare
// string literal is NOT enough: Mesh.vue and Federation.vue use the same
// dotted names as resource-cache keys (`key: 'mesh.self-did'`,
// `key: 'federation.nodes'`), which are not RPC methods at all. Matching those
// would report permanent phantom gaps and make this harness unfailable.
const CALL_RE = /method:\s*['"]((?:mesh|federation)\.[a-z0-9-]+)['"]/g
const CASE_RE = /case\s*['"]((?:mesh|federation)\.[a-z0-9-]+)['"]/g

function walk(dir) {
  const out = []
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    const st = statSync(p)
    if (st.isDirectory()) out.push(...walk(p))
    else if (/\.(ts|js|vue)$/.test(name)) out.push(p)
  }
  return out
}

function collect(re, text, into) {
  let m
  while ((m = re.exec(text)) !== null) into.add(m[1])
}

console.log('STATIC: cross-referencing UI call sites against mock handlers')
const called = new Set()
for (const file of walk(SRC)) collect(new RegExp(CALL_RE), readFileSync(file, 'utf8'), called)

const handled = new Set()
collect(new RegExp(CASE_RE), readFileSync(MOCK, 'utf8'), handled)

const missing = [...called].filter((m) => !handled.has(m)).sort()
const unexpected = missing.filter((m) => !KNOWN_GAPS.includes(m))

console.log(`  UI calls ${called.size} mesh/federation methods; mock implements ${handled.size}`)
if (missing.length === 0) {
  pass('every UI-called mesh/federation method has a mock handler')
} else {
  for (const m of unexpected) fail(`no mock handler for ${m}`)
  for (const m of missing.filter((x) => KNOWN_GAPS.includes(x))) {
    console.log(`  … known gap (allowed): ${m}`)
  }
}

// ── LIVE ────────────────────────────────────────────────────────────────────
const freePort = () =>
  new Promise((res, rej) => {
    const srv = createServer()
    srv.on('error', rej)
    srv.listen(0, '127.0.0.1', () => {
      const { port } = srv.address()
      srv.close(() => res(port))
    })
  })

const port = await freePort()
console.log(`LIVE: booting mock-backend.js on :${port}`)

const child = spawn(process.execPath, [MOCK], {
  cwd: UI_ROOT,
  // Never permit this regression harness to contact the live container runtime.
  env: { ...process.env, DEMO: '1', MOCK_BACKEND_HOST: '127.0.0.1', MOCK_BACKEND_PORT: String(port) },
  stdio: ['ignore', 'pipe', 'pipe'],
})
child.stdout.resume()
child.stderr.resume()

let childExited = false
child.on('exit', () => {
  childExited = true
})

const base = `http://127.0.0.1:${port}`
let cookie = ''
const rpc = async (method, params) => {
  const resp = await fetch(`${base}/rpc/v1`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', Cookie: cookie },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  })
  const session = resp.headers.getSetCookie().find(value => value.startsWith('demo_sid='))
  if (session) cookie = session.split(';')[0]
  if (!resp.ok) throw new Error(`${method} → HTTP ${resp.status}`)
  const body = await resp.json()
  if (body.error) throw new Error(`${method} → RPC error ${body.error.code}: ${body.error.message}`)
  return body.result
}

try {
  // Poll until the server answers (bounded).
  const deadline = Date.now() + 60000
  for (;;) {
    if (childExited) throw new Error('mock-backend exited before becoming ready')
    try {
      await rpc('server.echo', { message: 'parity-probe' })
      break
    } catch {
      if (Date.now() > deadline) throw new Error('mock-backend did not become ready within 60s')
      await new Promise((r) => setTimeout(r, 200))
    }
  }
  pass('mock-backend answered on the ephemeral port')

  const radio = await rpc('mesh.rnode-config')
  if (radio.live === null && radio.live_error.includes('demo')) pass('radio settings report unavailable demo hardware')
  else fail('demo radio status implies real hardware')
  const applied = await rpc('mesh.rnode-config-apply', { settings: radio.settings })
  if (applied.applied === false && applied.confirmed === false && applied.message.includes('demo')) pass('demo never claims a radio configuration was applied')
  else fail('demo radio apply claims hardware success')

  // 1. Peer aliasing round-trip (the FED-04 headline: renaming a peer sticks).
  const peers = await rpc('mesh.contacts-list')
  const target = peers.contacts.find((c) => c.pubkey)
  if (!target) throw new Error('mesh.contacts-list returned no contact with a pubkey')
  if (target.alias !== null) fail(`expected a fresh contact to have a null alias, got ${target.alias}`)

  await rpc('mesh.contacts-save', { pubkey: target.pubkey, alias: 'Base Camp' })
  const after = await rpc('mesh.contacts-list')
  const renamed = after.contacts.find((c) => c.pubkey === target.pubkey)
  if (renamed?.alias === 'Base Camp') pass('mesh.contacts-save → contacts-list round-trips the alias')
  else fail(`alias did not persist: expected 'Base Camp', got ${JSON.stringify(renamed?.alias)}`)

  // The daemon omits peers with no pubkey_hex; so must the demo.
  if (after.contacts.every((c) => c.pubkey)) pass('contacts-list omits peers without a pubkey, as the daemon does')
  else fail('contacts-list returned a contact with no pubkey')

  // 2. Scheduling: schedule → list → cancel → list.
  const fireAt = Math.floor(Date.now() / 1000) + 3600
  const scheduled = await rpc('mesh.schedule-message', {
    body: 'mesh parity check',
    fire_at: fireAt,
    contact_id: 1,
  })
  if (typeof scheduled.id !== 'number') fail('mesh.schedule-message did not return a numeric id')

  const listed = await rpc('mesh.list-scheduled')
  if (listed.messages.some((m) => m.id === scheduled.id)) pass('mesh.schedule-message → list-scheduled shows the entry')
  else fail('scheduled message missing from mesh.list-scheduled')

  const cancelled = await rpc('mesh.cancel-scheduled', { id: scheduled.id })
  if (cancelled.cancelled !== true) fail('mesh.cancel-scheduled did not report a cancellation')
  const afterCancel = await rpc('mesh.list-scheduled')
  if (!afterCancel.messages.some((m) => m.id === scheduled.id)) pass('mesh.cancel-scheduled removed the entry')
  else fail('cancelled message still present in mesh.list-scheduled')

  // Cancelling an unknown id must report false, not throw or lie.
  const ghost = await rpc('mesh.cancel-scheduled', { id: 999999 })
  if (ghost.cancelled === false) pass('cancel-scheduled reports false for an unknown id')
  else fail(`cancel-scheduled lied about an unknown id: ${JSON.stringify(ghost)}`)

  // 3. clear-all empties this session's sent messages.
  await rpc('mesh.send-content-inline', {
    contact_id: 1,
    mime: 'text/plain',
    filename: 'parity.txt',
    bytes_b64: Buffer.from('parity').toString('base64'),
  })
  const withMsg = await rpc('mesh.messages', { limit: 500 })
  const cleared = await rpc('mesh.clear-all')
  if (cleared.status !== 'cleared') fail(`mesh.clear-all returned ${JSON.stringify(cleared)}`)
  const afterClear = await rpc('mesh.messages', { limit: 500 })
  if (afterClear.count < withMsg.count) pass('mesh.clear-all dropped this session\'s messages')
  else fail(`mesh.clear-all did not reduce the message count (${withMsg.count} → ${afterClear.count})`)

  // 4. Assistant config round-trip.
  const status = await rpc('mesh.assistant-status')
  for (const k of ['enabled', 'model', 'trusted_only', 'backend', 'allowed_contacts', 'default_model', 'ollama_detected', 'claude_available', 'models', 'denied_askers']) {
    if (!(k in status)) fail(`mesh.assistant-status missing field '${k}'`)
  }
  const configured = await rpc('mesh.assistant-configure', { enabled: true, model: 'llama3.2:3b' })
  if (configured.enabled === true && configured.model === 'llama3.2:3b') pass('mesh.assistant-configure applies and echoes settings')
  else fail(`assistant-configure did not apply: ${JSON.stringify(configured)}`)
  const restatus = await rpc('mesh.assistant-status')
  if (restatus.enabled === true) pass('assistant config persists across calls')
  else fail('assistant config did not persist')

  // 5. federation.cancel-request removes the outbound request.
  const pending = await rpc('federation.list-pending-requests')
  const outbound = pending.requests.find((r) => r.outbound && r.state === 'sent')
  if (!outbound) {
    fail('no outbound/sent request in the demo seed — cancel-request is unexercisable')
  } else {
    const res = await rpc('federation.cancel-request', { id: outbound.id })
    if (res.cancelled !== true || res.notified !== true) fail(`cancel-request returned ${JSON.stringify(res)}`)
    const post = await rpc('federation.list-pending-requests')
    if (!post.requests.some((r) => r.id === outbound.id)) pass('federation.cancel-request removed the request')
    else fail('cancelled request still listed as pending')
  }

  // 6. Chat mutations are observable on the next mesh.messages read — the
  //    whole point of FED-04's "not a bare ok acknowledgement" requirement.
  const sent = await rpc('mesh.send-content-inline', {
    contact_id: 1,
    mime: 'text/plain',
    filename: 'mutate.txt',
    bytes_b64: Buffer.from('mutate me').toString('base64'),
  })
  const targetSeq = sent.message_id

  await rpc('mesh.send-reaction', {
    contact_id: 1,
    target_pubkey: 'demo02abababababababababababababababababababababababababababab',
    target_seq: targetSeq,
    emoji: '🔥',
  })
  let msgs = (await rpc('mesh.messages', { limit: 500 })).messages
  const reaction = msgs.find(
    (m) => m.message_type === 'reaction' && m.typed_payload?.target?.sender_seq === targetSeq,
  )
  if (reaction?.typed_payload?.emoji === '🔥') pass('mesh.send-reaction is visible as a reaction message with a target key')
  else fail('reaction did not appear in mesh.messages with the UI-expected shape')

  await rpc('mesh.send-reply', {
    contact_id: 1,
    target_pubkey: 'demo02abababababababababababababababababababababababababababab',
    target_seq: targetSeq,
    text: 'replying to that',
  })
  msgs = (await rpc('mesh.messages', { limit: 500 })).messages
  const reply = msgs.find((m) => m.message_type === 'reply' && m.typed_payload?.target?.sender_seq === targetSeq)
  if (reply?.plaintext === 'replying to that') pass('mesh.send-reply is visible as a reply carrying its target')
  else fail('reply did not appear with a target key')

  await rpc('mesh.edit-message', { contact_id: 1, target_seq: targetSeq, new_text: 'edited text' })
  msgs = (await rpc('mesh.messages', { limit: 500 })).messages
  const edited = msgs.find((m) => m.sender_seq === targetSeq && m.direction === 'sent')
  if (edited?.plaintext === 'edited text' && edited?.typed_payload?.edited_at)
    pass('mesh.edit-message rewrote the text and set the edited marker')
  else fail(`edit not applied: ${JSON.stringify(edited?.plaintext)}`)

  await rpc('mesh.delete-message', { contact_id: 1, target_seq: targetSeq })
  msgs = (await rpc('mesh.messages', { limit: 500 })).messages
  const deleted = msgs.find((m) => m.sender_seq === targetSeq && m.direction === 'sent')
  // The daemon tombstones in place rather than removing the row.
  if (deleted && deleted.message_type === 'delete' && deleted.typed_payload?.deleted === true)
    pass('mesh.delete-message tombstoned in place, as apply_local_delete does')
  else fail(`delete representation wrong: ${JSON.stringify(deleted)}`)

  const beforeForward = (await rpc('mesh.messages', { limit: 500 })).count
  await rpc('mesh.forward-message', { contact_id: 2, source_message_id: targetSeq })
  msgs = (await rpc('mesh.messages', { limit: 500 })).messages
  const forwarded = msgs.filter((m) => m.peer_contact_id === 2 && m.direction === 'sent')
  if (msgs.length > beforeForward && forwarded.length > 0) pass('mesh.forward-message pushed a copy for the destination peer')
  else fail('forward did not create a message for the destination peer')

  await rpc('mesh.send-channel', { channel: 3, message: 'channel broadcast' })
  msgs = (await rpc('mesh.messages', { limit: 500 })).messages
  if (msgs.some((m) => m.channel === 3 && m.plaintext === 'channel broadcast'))
    pass('mesh.send-channel pushed a channel-addressed message')
  else fail('channel message not visible in mesh.messages')

  // 7. federation.notify-did-change reports per-peer results.
  const notified = await rpc('federation.notify-did-change', {
    old_did: 'did:key:zOld',
    new_did: 'did:key:zNew',
    proof_signature: 'demo-sig',
    proof_message: 'demo-msg',
  })
  if (Array.isArray(notified.results) && notified.notified === notified.results.length) {
    pass('federation.notify-did-change reports one result per node')
  } else {
    fail(`notify-did-change shape wrong: ${JSON.stringify(notified)}`)
  }
} catch (err) {
  fail(err.message)
} finally {
  child.kill('SIGTERM')
}

if (failures > 0) {
  console.error(`\nFAILED: ${failures} parity problem(s)`)
  process.exit(1)
}
console.log('\nOK: demo/real mesh + federation RPC parity holds')
