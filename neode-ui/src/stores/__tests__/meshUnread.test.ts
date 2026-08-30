import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

import { rpcClient } from '@/api/rpc-client'
import { useMeshStore, type MeshMessage } from '../mesh'

const message = (id: number, contact = 7): MeshMessage => ({
  id,
  direction: 'received',
  peer_contact_id: contact,
  peer_name: 'Alice',
  plaintext: `message ${id}`,
  timestamp: `2026-01-${String(id).padStart(2, '0')}`,
  delivered: true,
  encrypted: true,
  transport: 'meshtastic',
})

function reply(messages: MeshMessage[]) {
  vi.mocked(rpcClient.call).mockResolvedValueOnce({ messages, count: messages.length })
}

describe('mesh unread persistence', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('does not swallow the first message after initializing with empty history', async () => {
    const store = useMeshStore()
    reply([])
    expect(await store.fetchMessages()).toEqual([])
    expect(localStorage.getItem('archipelago.mesh.last-seen.v1')).toBe('{}')

    reply([message(1)])
    expect(await store.fetchMessages()).toEqual([message(1)])
    expect(store.unreadCounts[7]).toBe(1)
  })

  it('keeps read messages read across a page refresh', async () => {
    const firstPage = useMeshStore()
    reply([message(1), message(2)])
    await firstPage.fetchMessages() // migration seeds existing history as read
    firstPage.markChatRead(7)

    setActivePinia(createPinia()) // simulate a full page/store reload
    const refreshedPage = useMeshStore()
    reply([message(1), message(2), message(3)])
    const newlyUnread = await refreshedPage.fetchMessages()

    expect(newlyUnread.map(m => m.id)).toEqual([3])
    expect(refreshedPage.unreadCounts[7]).toBe(1)

    refreshedPage.markChatRead(7)
    setActivePinia(createPinia())
    const readAgain = useMeshStore()
    reply([message(1), message(2), message(3)])
    expect(await readAgain.fetchMessages()).toEqual([])
    expect(readAgain.totalUnread).toBe(0)
  })
})
