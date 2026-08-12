import { ref } from 'vue'
import { useNostrIdentity } from './useNostrIdentity'
import { useNostr } from './useNostr'
import { exportAsMarkdown } from '@/utils/conversation-export'
import type { Conversation } from '@aiui/core/types/message'

export function useConversationShare() {
  const { signEvent, pubkey, isLoggedIn } = useNostrIdentity()
  const { publishEvent } = useNostr()

  const isSharing = ref(false)
  const shareError = ref<string | null>(null)
  const sharedNaddr = ref<string | null>(null)

  async function shareAsNostrArticle(
    conversation: Conversation,
    options: { encrypt?: boolean; recipientPubkey?: string } = {}
  ): Promise<string | null> {
    if (!isLoggedIn.value || !pubkey.value) {
      shareError.value = 'Please log in with a Nostr extension (NIP-07) first.'
      return null
    }

    isSharing.value = true
    shareError.value = null

    try {
      const markdown = exportAsMarkdown(conversation)
      const dTag = `aiui-conv-${conversation.id}`
      const title = conversation.title || 'AIUI Conversation'

      let content = markdown
      if (options.encrypt && options.recipientPubkey) {
        // NIP-44 encryption via extension
        const ext = (window as unknown as Record<string, unknown>).nostr as {
          nip44?: { encrypt(pubkey: string, plaintext: string): Promise<string> }
        } | undefined
        if (ext?.nip44?.encrypt) {
          content = await ext.nip44.encrypt(options.recipientPubkey, markdown)
        } else {
          shareError.value = 'NIP-44 encryption not supported by your extension.'
          return null
        }
      }

      const tags: string[][] = [
        ['d', dTag],
        ['title', title],
        ['published_at', String(Math.floor(conversation.createdAt / 1000))],
        ['t', 'aiui'],
        ['t', 'conversation'],
      ]

      if (conversation.model) {
        tags.push(['t', conversation.model])
      }

      if (options.encrypt && options.recipientPubkey) {
        tags.push(['p', options.recipientPubkey])
        tags.push(['encrypted', 'nip44'])
      }

      const unsigned = {
        kind: 30023, // Long-form article
        pubkey: pubkey.value,
        created_at: Math.floor(Date.now() / 1000),
        content,
        tags,
      }

      const signed = await signEvent(unsigned)
      if (!signed) {
        shareError.value = 'Failed to sign event.'
        return null
      }

      await publishEvent(signed)

      // Build naddr
      const naddr = buildNaddr(dTag, pubkey.value, signed.kind)
      sharedNaddr.value = naddr
      return naddr
    } catch (err) {
      shareError.value = err instanceof Error ? err.message : 'Failed to share conversation.'
      return null
    } finally {
      isSharing.value = false
    }
  }

  function buildNaddr(dTag: string, authorPubkey: string, kind: number): string {
    // Simplified naddr encoding — in production use a proper bech32 library
    const parts = [dTag, authorPubkey, String(kind)]
    return `nostr:naddr1${btoa(parts.join(':')).replace(/=/g, '')}`
  }

  return {
    isSharing,
    shareError,
    sharedNaddr,
    shareAsNostrArticle,
  }
}
