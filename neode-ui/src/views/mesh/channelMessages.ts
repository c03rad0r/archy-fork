import type { MeshMessage } from '@/stores/mesh'

const MAX_U32 = 4294967295
const MAX_MESHCORE_CHANNEL_INDEX = 7

export function channelContactId(channelIndex: number): number {
  return MAX_U32 - channelIndex
}

export function isPublicChannelMessage(message: MeshMessage): boolean {
  if (message.peer_contact_id === channelContactId(0)) return true

  // Compatibility for messages persisted by the old V3 decoder, which used
  // SNR as the channel index. MeshCore has only slots 0..7, making 8..255 an
  // unambiguous legacy decoder artifact.
  const derivedIndex = MAX_U32 - message.peer_contact_id
  return message.transport === 'meshcore'
    && derivedIndex > MAX_MESHCORE_CHANNEL_INDEX
    && derivedIndex <= 255
}

export function isChannelMessage(message: MeshMessage, channelIndex: number): boolean {
  return channelIndex === 0
    ? isPublicChannelMessage(message)
    : message.peer_contact_id === channelContactId(channelIndex)
}

