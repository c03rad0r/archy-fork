import { ref, onMounted, onUnmounted } from 'vue'

const STORAGE_KEY = 'aiui-sync-queue'

interface QueuedOperation {
  id: string
  type: string
  data: unknown
  createdAt: number
}

const queue = ref<QueuedOperation[]>([])
const hasQueuedItems = ref(false)

function loadQueue() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) queue.value = JSON.parse(stored)
    hasQueuedItems.value = queue.value.length > 0
  } catch { /* ignore */ }
}

function saveQueue() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(queue.value))
  hasQueuedItems.value = queue.value.length > 0
}

loadQueue()

export function useSyncQueue() {
  function enqueue(type: string, data: unknown) {
    queue.value.push({
      id: crypto.randomUUID(),
      type,
      data,
      createdAt: Date.now(),
    })
    saveQueue()
  }

  async function processQueue(handler: (op: QueuedOperation) => Promise<boolean>) {
    const remaining: QueuedOperation[] = []
    for (const op of queue.value) {
      try {
        const success = await handler(op)
        if (!success) remaining.push(op)
      } catch {
        remaining.push(op)
      }
    }
    queue.value = remaining
    saveQueue()
  }

  function clearQueue() {
    queue.value = []
    saveQueue()
  }

  // Auto-retry on visibility change
  let retryHandler: (() => void) | null = null

  function startAutoRetry(handler: (op: QueuedOperation) => Promise<boolean>) {
    retryHandler = () => {
      if (document.visibilityState === 'visible' && queue.value.length > 0) {
        processQueue(handler)
      }
    }
    document.addEventListener('visibilitychange', retryHandler)
  }

  onMounted(() => {
    // Already loaded
  })

  onUnmounted(() => {
    if (retryHandler) {
      document.removeEventListener('visibilitychange', retryHandler)
    }
  })

  return {
    queue,
    hasQueuedItems,
    enqueue,
    processQueue,
    clearQueue,
    startAutoRetry,
  }
}
