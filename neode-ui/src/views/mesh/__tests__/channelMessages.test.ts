import { describe, expect, it } from 'vitest'
import type { MeshMessage } from '@/stores/mesh'
import { channelContactId, isChannelMessage, isPublicChannelMessage } from '../channelMessages'

function message(peerContactId: number, transport = 'meshcore'): MeshMessage {
  return {
    id: 1,
    direction: 'received',
    peer_contact_id: peerContactId,
    peer_name: null,
    plaintext: 'hello',
    timestamp: '2026-10-10T00:00:00Z',
    delivered: true,
    encrypted: false,
    transport,
    message_type: 'text',
    typed_payload: null,
    sender_pubkey: null,
    sender_seq: null,
  }
}

describe('Mesh public-channel compatibility', () => {
  it('shows a correctly decoded slot-zero message in Public', () => {
    expect(isPublicChannelMessage(message(channelContactId(0)))).toBe(true)
  })

  it('recovers persisted V3 messages whose SNR was mistaken for a channel index', () => {
    expect(isPublicChannelMessage(message(channelContactId(47)))).toBe(true)
    expect(isPublicChannelMessage(message(channelContactId(208)))).toBe(true)
  })

  it('does not merge valid non-public slots or another transport into Public', () => {
    expect(isPublicChannelMessage(message(channelContactId(1)))).toBe(false)
    expect(isPublicChannelMessage(message(channelContactId(47), 'meshtastic'))).toBe(false)
    expect(isChannelMessage(message(channelContactId(1)), 1)).toBe(true)
  })
})

