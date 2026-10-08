import { defineStore } from 'pinia'
import { ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'

export const useWeb5BadgeStore = defineStore('web5Badge', () => {
  const pendingRequestCount = ref(0)

  async function refresh() {
    const results = await Promise.allSettled([
      rpcClient.call<{ requests: Array<{ id: string }> }>({ method: 'network.list-requests' }),
      rpcClient.federationListPendingRequests(),
    ])
    // Do not clear an existing badge when a request source is unavailable.
    if (results.every(result => result.status === 'fulfilled')) {
      const legacy = results[0]
      const nostr = results[1]
      if (legacy?.status === 'fulfilled' && nostr?.status === 'fulfilled') {
        pendingRequestCount.value = (legacy.value.requests?.length ?? 0) +
          (nostr.value.requests?.filter(request => request.state === 'pending' && !request.outbound).length ?? 0)
      }
    }
  }

  return { pendingRequestCount, refresh }
})
