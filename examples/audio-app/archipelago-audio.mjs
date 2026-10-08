// Independently authored reference adapter for the public archipelago-v1 protocol.
// No application authentication, signing keys or stream URLs cross this bridge.
export function attachAudioBridge({ parentOrigin, snapshot, actions, unlocked = () => true, host = window }) {
  const parsed = new URL(parentOrigin)
  if (!['http:', 'https:'].includes(parsed.protocol) || parsed.origin !== parentOrigin || parsed.username || parsed.password) throw new Error('Configure an exact trusted dashboard origin')
  if (host.parent === host) return { publish() {}, dispose() {} }
  let session = null, closed = false, queue = Promise.resolve()
  const send = data => host.parent.postMessage({ version: 1, ...data }, parentOrigin)
  function publish() {
    if (closed || !session) return
    const state = unlocked() ? snapshot() : null
    if (!state || typeof state.title !== 'string' || typeof state.artist !== 'string'
      || typeof state.playing !== 'boolean' || typeof state.shuffle !== 'boolean'
      || !Number.isFinite(state.position) || !Number.isFinite(state.duration) || state.position < 0 || state.duration < 0) {
      send({type:'archipelago:media-state',session,available:false}); return
    }
    send({type:'archipelago:media-state',session,available:true,title:state.title.slice(0,256),artist:state.artist.slice(0,256),
      playing:state.playing,shuffle:state.shuffle,position:Math.min(state.position,state.duration),duration:state.duration,
      artwork:typeof state.artwork === 'string' ? state.artwork.slice(0,2048) : ''})
  }
  function message(event) {
    if (closed || event.source !== host.parent || event.origin !== parentOrigin || event.data?.version !== 1) return
    const data = event.data
    if (data.type === 'archipelago:media-connect') {
      if (typeof data.session !== 'string' || !/^[0-9a-f]{32}$/.test(data.session)) return
      session = data.session; publish(); return
    }
    if (data.type !== 'archipelago:media-control' || !session || data.session !== session || !unlocked()) return
    if (!['play','pause','seek','next','previous','shuffle','snapshot'].includes(data.command)) return
    if (data.command === 'seek' && (!Number.isFinite(data.position) || data.position < 0)) return
    const expected = session
    queue = queue.catch(() => {}).then(async () => {
      if (closed || session !== expected || !unlocked()) return
      if (data.command !== 'snapshot') {
        const action = actions[data.command]
        if (typeof action !== 'function') return
        const state = snapshot()
        if (data.command === 'seek') {
          if (!Number.isFinite(state?.duration) || state.duration < 0) return
          await action(Math.min(data.position, state.duration))
        } else await action()
      }
      if (!closed && session === expected) publish()
    }).catch(() => { if (!closed && session === expected) publish() })
  }
  const ready = () => { if (!closed) send({type:'archipelago:media-ready'}) }
  function dispose() {
    if (closed) return
    if (session) send({type:'archipelago:media-state',session,available:false})
    closed = true; session = null
    host.removeEventListener('message',message); host.removeEventListener('pageshow',ready); host.removeEventListener('pagehide',leave)
    try { Promise.resolve(actions.pause?.()).catch(() => {}) } catch { /* App teardown still releases the bridge. */ }
  }
  function leave(event) { if (!event.persisted) dispose() }
  host.addEventListener('message',message); host.addEventListener('pageshow',ready); host.addEventListener('pagehide',leave)
  ready()
  return { publish, dispose }
}
