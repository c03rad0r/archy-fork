import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { useMeshStore } from '@/stores/mesh'

export interface ReceivedMessage {
  from_pubkey: string
  message: string
  timestamp: string
}

const MESSAGE_POLL_INTERVAL = 30000 // 30s

// Shared state (singleton) so toast works across route changes
const receivedMessages = ref<ReceivedMessage[]>([])
const lastMessageCount = ref(0)
const loadingMessages = ref(false)
type MessageToast = {
  show: boolean
  text: string
  fromPubkey: string
  contactId: number | null
}
const emptyToast = (): MessageToast => ({ show: false, text: '', fromPubkey: '', contactId: null })
const toastMessage = ref<MessageToast>(emptyToast())
let pollTimer: ReturnType<typeof setInterval> | null = null

export function useMessageToast() {
  const router = useRouter()
  const mesh = useMeshStore()

  const unreadCount = computed(() =>
    Math.max(0, receivedMessages.value.length - lastMessageCount.value)
  )

  async function loadReceivedMessages() {
    loadingMessages.value = true
    try {
      const res = await rpcClient.getReceivedMessages()
      const msgs = (res.messages || []) as ReceivedMessage[]
      receivedMessages.value = msgs
      // New messages since last check? (don't show toast on initial load)
      if (msgs.length > lastMessageCount.value && lastMessageCount.value > 0) {
        const newCount = msgs.length - lastMessageCount.value
        const latest = msgs[msgs.length - 1]
        toastMessage.value = {
          show: true,
          text: (newCount === 1 ? latest?.message : null) ?? `${newCount} new messages`,
          // Only deep-link to a specific chat when it's a single new message
          // from one sender; otherwise open the mesh list.
          fromPubkey: newCount === 1 ? (latest?.from_pubkey ?? '') : '',
          contactId: null,
        }
        lastMessageCount.value = msgs.length
      } else {
        lastMessageCount.value = msgs.length
      }
    } catch (e) {
      // Stop polling on auth failure — session expired, no point retrying
      if (e instanceof Error && /401|Unauthorized/i.test(e.message)) {
        stopPolling()
        return
      }
      if (import.meta.env.DEV) console.error('Failed to load messages:', e)
    } finally {
      loadingMessages.value = false
    }

    // Federation messages and radio-mesh messages use separate backend
    // queues. Poll the mesh store too so Meshtastic/MeshCore/Reticulum
    // arrivals produce the same app-wide toast. fetchMessages returns only
    // the newly-unread batch computed from its durable per-contact watermark.
    const newMeshMessages = await mesh.fetchMessages()
    if (newMeshMessages.length > 0) {
      const latest = newMeshMessages[newMeshMessages.length - 1]!
      const oneConversation = newMeshMessages.every(
        msg => msg.peer_contact_id === latest.peer_contact_id
      )
      toastMessage.value = {
        show: true,
        text: newMeshMessages.length === 1
          ? latest.plaintext
          : `${newMeshMessages.length} new messages`,
        fromPubkey: '',
        contactId: oneConversation ? latest.peer_contact_id : null,
      }
    }
  }

  function isAuthenticated(): boolean {
    return localStorage.getItem('neode-auth') === 'true'
  }

  function startPolling() {
    if (pollTimer) return
    if (!isAuthenticated()) return
    loadReceivedMessages()
    pollTimer = setInterval(() => {
      if (!isAuthenticated()) {
        stopPolling()
        return
      }
      loadReceivedMessages()
    }, MESSAGE_POLL_INTERVAL)
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  function markAsRead() {
    lastMessageCount.value = receivedMessages.value.length
  }

  function dismissToastAndOpenMessages() {
    const { fromPubkey: peer, contactId } = toastMessage.value
    toastMessage.value = emptyToast()
    markAsRead()
    // Open the exact radio conversation by contact id, or the federation
    // conversation by pubkey. Multiple conversations fall back to the list.
    if (contactId !== null) {
      router.push({ path: '/dashboard/mesh', query: { contact: String(contactId) } })
    } else {
      router.push(peer ? { path: '/dashboard/mesh', query: { peer } } : '/dashboard/mesh')
    }
  }

  // Dismiss the toast without navigating (the close icon).
  function closeToast() {
    toastMessage.value = emptyToast()
  }

  return {
    receivedMessages,
    lastMessageCount,
    loadingMessages,
    toastMessage,
    unreadCount,
    loadReceivedMessages,
    startPolling,
    stopPolling,
    markAsRead,
    dismissToastAndOpenMessages,
    closeToast,
  }
}
