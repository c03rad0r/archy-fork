// Exercise the actual browser upload protocol against an isolated demo server.
import assert from 'node:assert/strict'
import { after, before, test } from 'node:test'
import { spawn } from 'node:child_process'
import { createServer } from 'node:net'
import { readFile } from 'node:fs/promises'
import { File } from 'node:buffer'
import { createHash, randomBytes } from 'node:crypto'
import ts from 'typescript'
import http from 'node:http'

const root = new URL('../', import.meta.url)
const source = await readFile(new URL('src/api/resumable-upload.ts', root), 'utf8')
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText
const { resumableUpload } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`)
globalThis.window = new EventTarget()
globalThis.document = new EventTarget()
document.visibilityState = 'visible'
let child, origin, output = ''
before(async () => {
  const server = createServer()
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  const port = server.address().port
  await new Promise(resolve => server.close(resolve))
  origin = `http://127.0.0.1:${port}`
  child = spawn(process.execPath, ['mock-backend.js'], { cwd: root, env: {
    ...process.env, DEMO: '1', MOCK_BACKEND_HOST: '127.0.0.1',
    MOCK_BACKEND_PORT: String(port), DEMO_FILE_QUOTA_BYTES: String(8 * 1024 * 1024),
  }, stdio: ['ignore', 'pipe', 'pipe'] })
  child.stdout.on('data', data => { output = (output + data).slice(-10000) })
  child.stderr.on('data', data => { output = (output + data).slice(-10000) })
  for (let n = 0; n < 600; n++) {
    try { if ((await fetch(`${origin}/health`)).ok) return } catch {}
    if (child.exitCode !== null) break
    await new Promise(resolve => setTimeout(resolve, 100))
  }
  throw new Error(`Demo failed to start: ${output}`)
})
after(async () => {
  if (child && child.exitCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve))
    child.kill('SIGTERM')
    await exited
  }
})
async function visitor() {
  const response = await fetch(`${origin}/health`)
  const cookie = response.headers.getSetCookie().find(v => v.startsWith('demo_sid='))?.split(';')[0]
  assert.ok(cookie, 'a new visitor must receive an isolated session')
  return (url, init = {}) => fetch(url.startsWith('http') ? url : origin + url, {
    ...init, headers: { ...init.headers, Cookie: cookie },
  })
}
const prefix = '/app/filebrowser/api'
const path = p => p.split('/').map(encodeURIComponent).join('/')
const begin = (client, name, size) => client(`${prefix}/tus${path(name)}`, {
  method: 'POST', headers: { 'Upload-Length': String(size), 'Tus-Resumable': '1.0.0' },
})
const patch = (client, name, offset, body) => client(`${prefix}/tus${path(name)}`, {
  method: 'PATCH', headers: { 'Upload-Offset': String(offset), 'Tus-Resumable': '1.0.0',
    'Content-Type': 'application/offset+octet-stream' }, body,
})
test('real client resumes lost chunk/rename replies; exact bytes, encoded name and visitor isolation', async () => {
  const alice = await visitor(), bob = await visitor()
  const data = randomBytes(5 * 1024 * 1024 + 123)
  const name = 'literal %2F + ? # é.bin'
  let droppedChunk = false, droppedRename = false, last = 0
  const transport = async (url, init) => {
    const response = await alice(url, init)
    if (init?.method === 'PATCH' && url.includes('/tus/') && !droppedChunk) {
      assert.equal(response.status, 204)
      droppedChunk = true
      throw new TypeError('simulated lost chunk reply')
    }
    if (init?.method === 'PATCH' && url.includes('action=rename') && !droppedRename) {
      assert.equal(response.status, 200)
      droppedRename = true
      throw new TypeError('simulated lost rename reply')
    }
    return response
  }
  await resumableUpload(`${origin}/app/filebrowser`, '/Documents', new File([data], name), transport,
    { signal: new AbortController().signal, onProgress: value => { last = value } })
  assert.ok(droppedChunk && droppedRename)
  assert.equal(last, data.length)
  const url = `${prefix}/resources${path(`/Documents/${name}`)}?checksum=sha256`
  const saved = await (await alice(url)).json()
  assert.equal(saved.size, data.length)
  assert.equal(saved.checksums.sha256, createHash('sha256').update(data).digest('hex'))
  assert.deepEqual(Buffer.from(await (await alice(`${prefix}/raw${path(`/Documents/${name}`)}`)).arrayBuffer()), data)
  assert.equal((await bob(url)).status, 404)
  assert.equal((await bob(`${prefix}/raw${path(`/Documents/${name}`)}`)).status, 404)
  const listed = await (await alice(`${prefix}/resources/Documents`)).json()
  assert.ok(listed.items.some(e => e.name === name))
  assert.ok(!listed.items.some(e => e.name.startsWith('.archy-upload-')))
})
test('zero bytes, replacement and cancellation leave only completed destinations', async () => {
  const client = await visitor()
  for (const bytes of [Buffer.from('old'), Buffer.alloc(0), Buffer.from('new')]) {
    await resumableUpload(`${origin}/app/filebrowser`, '/Documents', new File([bytes], 'replace.bin'), client,
      { signal: new AbortController().signal, onProgress() {} })
    assert.deepEqual(Buffer.from(await (await client(`${prefix}/raw/Documents/replace.bin`)).arrayBuffer()), bytes)
  }
  const controller = new AbortController()
  await assert.rejects(resumableUpload(`${origin}/app/filebrowser`, '/Documents',
    new File([randomBytes(3 * 1024 * 1024)], 'cancel.bin'), client, {
      signal: controller.signal, onProgress(n) { if (n > 0) controller.abort() },
    }), { name: 'AbortError' })
  const listed = await (await client(`${prefix}/resources/Documents`)).json()
  assert.ok(!listed.items.some(e => e.name.startsWith('.archy-upload-') || e.name === 'cancel.bin'))
  assert.equal((await begin(client, '/Documents/quota.part', 7 * 1024 * 1024)).status, 201)
})
test('quota includes simultaneous reservations; stale offsets and oversized chunks preserve committed bytes', async () => {
  const client = await visitor()
  // Deleting a seeded file must not make the byte counter negative.
  assert.equal((await client(`${prefix}/resources/Documents/bitcoin-whitepaper-notes.md`, { method: 'DELETE' })).status, 200)
  assert.equal((await begin(client, '/Documents/a.part', 6 * 1024 * 1024)).status, 201)
  assert.equal((await begin(client, '/Documents/b.part', 3 * 1024 * 1024)).status, 507)
  assert.equal((await patch(client, '/Documents/a.part', 1, Buffer.from('bad'))).status, 409)
  assert.equal((await patch(client, '/Documents/a.part', 0, Buffer.alloc(2 * 1024 * 1024 + 1))).status, 413)
  assert.equal((await client(`${prefix}/tus/Documents/a.part`, { method: 'HEAD' })).headers.get('Upload-Offset'), '0')
  assert.equal((await patch(client, '/Documents/a.part', 0, Buffer.from('abc'))).status, 204)
  assert.equal((await patch(client, '/Documents/a.part', 0, Buffer.from('abc'))).status, 409)
  assert.equal((await client(`${prefix}/tus/Documents/a.part`, { method: 'HEAD' })).headers.get('Upload-Offset'), '3')
  assert.equal((await client(`${prefix}/tus/Documents/a.part`, { method: 'DELETE' })).status, 204)
  assert.equal((await begin(client, '/Documents/b.part', 8 * 1024 * 1024)).status, 201)
})
test('interrupted chunk resumes from committed offset; deleting a folder releases pending reservations', async () => {
  const response = await fetch(`${origin}/health`)
  const cookie = response.headers.getSetCookie().find(v => v.startsWith('demo_sid='))?.split(';')[0]
  const client = (url, init = {}) => fetch(origin + url, { ...init, headers: { ...init.headers, Cookie: cookie } })
  await client(`${prefix}/resources/Temporary/`, { method: 'POST' })
  assert.equal((await begin(client, '/Temporary/a.part', 6)).status, 201)
  const request = http.request(`${origin}${prefix}/tus/Temporary/a.part`, { method: 'PATCH', headers: {
    Cookie: cookie, 'Upload-Offset': '0', 'Tus-Resumable': '1.0.0',
    'Content-Type': 'application/offset+octet-stream', 'Content-Length': '6',
  } })
  request.on('error', () => {})
  request.write('abc')
  // HEAD can run while the chunk is incomplete, but must not report those bytes
  // committed. Abort the real TCP request, then resume the exact same offset.
  await new Promise(resolve => setTimeout(resolve, 100))
  assert.equal((await client(`${prefix}/tus/Temporary/a.part`, { method: 'HEAD' })).headers.get('Upload-Offset'), '0')
  const closed = new Promise(resolve => request.once('close', resolve))
  request.destroy()
  await closed
  let result
  for (let n = 0; n < 20; n++) {
    result = await patch(client, '/Temporary/a.part', 0, Buffer.from('abcdef'))
    if (result.status !== 409) break
    await new Promise(resolve => setTimeout(resolve, 25))
  }
  assert.equal(result.status, 204)
  assert.equal(await (await client(`${prefix}/raw/Temporary/a.part`)).text(), 'abcdef')
  assert.equal((await begin(client, '/Temporary/b.part', 7 * 1024 * 1024)).status, 201)
  await client(`${prefix}/resources/Temporary`, { method: 'DELETE' })
  assert.equal((await client(`${prefix}/tus/Temporary/b.part`, { method: 'HEAD' })).status, 404)
  assert.equal((await begin(client, '/Documents/c.part', 8 * 1024 * 1024)).status, 201)
})
