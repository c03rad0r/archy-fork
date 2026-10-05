// In-memory File Browser TUS subset used by the public demo. Each visitor's
// reservations and bytes belong to their existing isolated, expiring session.
export function installDemoUploads(app, { currentStore, quota, normalize, parent, base, type }) {
  const route = '/app/filebrowser/api/tus/*'
  const number = value => typeof value === 'string' && /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) ? Number(value) : null
  const locate = req => normalize(req.params[0] || '')
  const headers = (res, upload) => res.set({
    'Tus-Resumable': '1.0.0', 'Upload-Offset': String(upload.offset),
    'Upload-Length': String(upload.length), 'Cache-Control': 'no-store',
  })
  const complete = (files, name, upload) => {
    const data = Buffer.concat(upload.chunks, upload.length)
    files.contents[name] = data
    files.tree[parent(name)].push({ name: base(name), path: name, size: data.length,
      modified: new Date().toISOString(), isDir: false, type: type(base(name)) })
    files.bytes += data.length
    files.reserved -= upload.length
    files.uploaded.add(name)
    files.uploads.delete(name)
  }
  app.head(route, (req, res) => {
    const upload = currentStore().files.uploads.get(locate(req))
    if (!upload) return res.sendStatus(404)
    headers(res, upload).status(200).end()
  })
  app.post(route, (req, res) => {
    const name = locate(req)
    const files = currentStore().files
    const length = number(req.get('Upload-Length'))
    if (req.get('Tus-Resumable') !== '1.0.0' || length === null || name === '/' ||
        name.split('/').some(p => p === '.' || p === '..' || p.includes('\0'))) return res.sendStatus(400)
    if (!files.tree[parent(name)]) return res.sendStatus(404)
    if (files.uploads.has(name) || files.tree[parent(name)].some(e => e.path === name)) return res.sendStatus(409)
    // Reserve the whole declared size, so concurrent uploads cannot evade quota.
    // Bound empty/abandoned session entries independently of their byte length.
    if (files.bytes + files.reserved + length > quota || files.uploads.size >= 64) return res.sendStatus(507)
    const upload = { length, offset: 0, chunks: [], writing: false }
    files.uploads.set(name, upload)
    files.reserved += length
    if (!length) complete(files, name, upload)
    headers(res, upload).location(req.path).status(201).end()
  })
  app.patch(route, async (req, res) => {
    const name = locate(req)
    const files = currentStore().files
    const upload = files.uploads.get(name)
    if (!upload) return res.sendStatus(404)
    if (req.get('Tus-Resumable') !== '1.0.0') return res.sendStatus(412)
    if (req.get('Content-Type') !== 'application/offset+octet-stream') return res.sendStatus(415)
    if (upload.writing || number(req.get('Upload-Offset')) !== upload.offset) return headers(res, upload).status(409).end()
    upload.writing = true
    const chunks = []
    let size = 0
    try {
      for await (const chunk of req) {
        size += chunk.length
        if (size > 2 * 1024 * 1024 || size > upload.length - upload.offset) {
          res.status(413).end()
          return
        }
        chunks.push(chunk)
      }
      // A simultaneous cancellation must never resurrect its staging file.
      if (files.uploads.get(name) !== upload) return res.sendStatus(404)
      upload.chunks.push(...chunks)
      upload.offset += size
      if (upload.offset === upload.length) complete(files, name, upload)
      headers(res, upload).status(204).end()
    } catch {
      // An interrupted chunk does not advance the committed offset. HEAD tells
      // the reconnecting client exactly where to resume.
      if (!res.headersSent && !req.destroyed) res.sendStatus(400)
    } finally {
      upload.writing = false
    }
  })
  app.delete(route, (req, res) => {
    const files = currentStore().files
    const name = locate(req)
    const upload = files.uploads.get(name)
    if (!upload) return res.sendStatus(404)
    files.reserved -= upload.length
    files.uploads.delete(name)
    res.status(204).end()
  })
}
