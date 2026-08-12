import { ref, onMounted, onBeforeUnmount } from 'vue'

const price = ref<number | null>(null)
const currency = ref('USD')
const lastUpdated = ref<number | null>(null)
const isLoading = ref(false)

let refreshTimer: ReturnType<typeof setInterval> | null = null
let initialized = false

const CACHE_TTL = 30_000 // 30 seconds

async function fetchPrice() {
  // Skip if we fetched recently (within TTL)
  if (lastUpdated.value && Date.now() - lastUpdated.value < CACHE_TTL) return

  isLoading.value = true
  try {
    const res = await fetch('https://mempool.space/api/v1/prices')
    if (!res.ok) throw new Error('Failed to fetch price')
    const data = await res.json()
    price.value = data.USD ?? null
    lastUpdated.value = Date.now()
  } catch {
    // Keep existing price on error
  } finally {
    isLoading.value = false
  }
}

export function useBitcoinPrice() {
  function formatPrice(usd: number | null): string {
    if (usd === null) return '...'
    return '$' + usd.toLocaleString('en-US', { minimumFractionDigits: 0, maximumFractionDigits: 0 })
  }

  function satsToUsd(sats: number): string {
    if (!price.value) return '...'
    const usd = (sats / 100000000) * price.value
    if (usd < 0.01) return '<$0.01'
    return '$' + usd.toFixed(2)
  }

  function usdToSats(usd: number): number | null {
    if (!price.value) return null
    return Math.round((usd / price.value) * 100000000)
  }

  onMounted(() => {
    if (!initialized) {
      initialized = true
      fetchPrice()
      refreshTimer = setInterval(fetchPrice, 60000) // 60s refresh
    }
  })

  onBeforeUnmount(() => {
    if (refreshTimer) {
      clearInterval(refreshTimer)
      refreshTimer = null
      initialized = false
    }
  })

  return {
    price,
    currency,
    lastUpdated,
    isLoading,
    formatPrice,
    satsToUsd,
    usdToSats,
    fetchPrice,
  }
}
