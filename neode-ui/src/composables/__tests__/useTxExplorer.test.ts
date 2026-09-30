// Regression suite for the recurring "tx link opens tx1138.com instead of
// the local Mempool app" bug (reported again on .228, 2026-08-06).
//
// The root cause was never the explorer preference — it was that
// `getAppState` reports `not-installed` for an app whose container list has
// not been fetched yet. A click that landed before the list arrived sent
// the user to a third-party explorer, telling that operator which
// transaction they cared about. These tests pin the fix: the decision waits
// for real data, and the local app wins whenever it exists.
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

const openSession = vi.fn()
vi.mock('@/stores/appLauncher', () => ({
  useAppLauncherStore: () => ({ openSession }),
}))

let containerState: string
let fetched: boolean
const ensureFetched = vi.fn(async () => {
  // Mirrors the real store: state only becomes knowable after the fetch.
  fetched = true
})
vi.mock('@/stores/container', () => ({
  useContainerStore: () => ({
    ensureFetched,
    getAppState: (_id: string) => (fetched ? containerState : 'not-installed'),
  }),
}))

import { useTxExplorer, DEFAULT_TX_EXPLORER } from '../useTxExplorer'

const TX = 'a'.repeat(64)

describe('useTxExplorer.openTx', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    localStorage.clear()
    fetched = false
    containerState = 'running'
    // Reset module-scope prefs/pending between tests.
    const { setExplorer, cancelPending } = useTxExplorer()
    setExplorer(DEFAULT_TX_EXPLORER, false)
    cancelPending()
  })

  it('opens the local Mempool app when it is running', async () => {
    const { openTx, pendingTx } = useTxExplorer()
    await openTx(TX)
    expect(openSession).toHaveBeenCalledWith('mempool', { path: `/tx/${TX}` })
    expect(pendingTx.value).toBeNull()
  })

  it('waits for the container list rather than assuming not-installed (the race)', async () => {
    const { openTx, pendingTx } = useTxExplorer()
    // fetched=false at click time — the old synchronous check read
    // 'not-installed' here and went external.
    await openTx(TX)
    expect(ensureFetched).toHaveBeenCalled()
    expect(openSession).toHaveBeenCalledWith('mempool', { path: `/tx/${TX}` })
    expect(pendingTx.value).toBeNull()
  })

  it('still prefers the local app when it is installed but stopped', async () => {
    containerState = 'stopped'
    const { openTx } = useTxExplorer()
    await openTx(TX)
    expect(openSession).toHaveBeenCalledWith('mempool', { path: `/tx/${TX}` })
  })

  it('prefers the local app mid-restart rather than leaking to a third party', async () => {
    containerState = 'restarting'
    const { openTx } = useTxExplorer()
    await openTx(TX)
    expect(openSession).toHaveBeenCalledWith('mempool', { path: `/tx/${TX}` })
  })

  it('does not open an external explorer while container discovery is pending', async () => {
    const external = vi.spyOn(window, 'open').mockImplementation(() => null)
    let finish!: () => void
    ensureFetched.mockImplementationOnce(() => new Promise<void>(resolve => {
      finish = () => { fetched = true; resolve() }
    }))
    const { openTx, setExplorer } = useTxExplorer()
    setExplorer(DEFAULT_TX_EXPLORER, true)
    const opening = openTx(TX)
    expect(external).not.toHaveBeenCalled()
    expect(openSession).not.toHaveBeenCalled()
    finish()
    await opening
    expect(openSession).toHaveBeenCalledWith('mempool', { path: `/tx/${TX}` })
    expect(external).not.toHaveBeenCalled()
    external.mockRestore()
  })

  it('asks for consent only when Mempool genuinely is not installed', async () => {
    containerState = 'not-installed'
    const { openTx, pendingTx } = useTxExplorer()
    await openTx(TX)
    expect(openSession).not.toHaveBeenCalled()
    expect(pendingTx.value).toBe(TX)
  })
})

describe('explorer default migration', () => {
  beforeEach(() => { localStorage.clear(); vi.resetModules() })
  it('uses mempool.space with consent for a new browser', async () => {
    const { useTxExplorer } = await import('../useTxExplorer')
    expect(useTxExplorer().prefs.value).toEqual({ url: 'https://mempool.space', acknowledged: false })
  })
  it.each(['https://tx1138.com', 'https://tx1138.com/', 'http://tx1138.com'])('migrates %s and resets consent', async url => {
    localStorage.setItem('archipelago.tx-explorer.v1', JSON.stringify({ url, acknowledged: true }))
    const { useTxExplorer } = await import('../useTxExplorer')
    expect(useTxExplorer().prefs.value).toEqual({ url: 'https://mempool.space', acknowledged: false })
    expect(JSON.parse(localStorage.getItem('archipelago.tx-explorer.v1')!)).toEqual(useTxExplorer().prefs.value)
  })
  it('preserves a custom explorer and its consent', async () => {
    const prefs = { url: 'https://my-explorer.example', acknowledged: true }
    localStorage.setItem('archipelago.tx-explorer.v1', JSON.stringify(prefs))
    const { useTxExplorer } = await import('../useTxExplorer')
    expect(useTxExplorer().prefs.value).toEqual(prefs)
  })
})
