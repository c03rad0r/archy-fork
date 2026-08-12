import { defineStore } from 'pinia'
import { ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'

export const useWeb5BadgeStore = defineStore('web5Badge', () => {
  const pendingRequestCount = ref(0)

  async function refresh() {
    try {
      const res = await rpcClient.call<{ requests: Array<{ id: string }> }>({ method: 'network.list-requests' })
      pendingRequestCount.value = res.requests?.length ?? 0
    } catch (e) {
      if (import.meta.env.DEV) console.warn('Badge refresh failed — best-effort', e)
    }
  }

  return { pendingRequestCount, refresh }
})
