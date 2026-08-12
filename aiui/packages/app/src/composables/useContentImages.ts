import { ref, reactive, watch, isRef, type Ref } from 'vue'

interface UseContentImagesOptions<T> {
  items: Ref<T[]> | (() => T[])
  id: (item: T) => string
  existingUrl: (item: T) => string | undefined | null
  fetch: (item: T) => Promise<string | null>
  fallback: (item: T) => string
}

export function useContentImages<T>(options: UseContentImagesOptions<T>) {
  const failed = ref<Set<string>>(new Set())
  const fetched = reactive<Map<string, string>>(new Map())

  function markFailed(id: string) {
    failed.value = new Set([...failed.value, id])
  }

  function coverSrc(item: T): string | null {
    const itemId = options.id(item)
    if (failed.value.has(itemId)) return null
    return options.existingUrl(item) || fetched.get(itemId) || null
  }

  function fallbackSrc(item: T): string {
    return options.fallback(item)
  }

  function onError(item: T) {
    markFailed(options.id(item))
  }

  function isLoading(item: T): boolean {
    const itemId = options.id(item)
    return !coverSrc(item) && !failed.value.has(itemId)
  }

  function fetchCovers(list: T[]) {
    for (const item of list) {
      const itemId = options.id(item)
      if (options.existingUrl(item) || fetched.has(itemId) || failed.value.has(itemId)) continue
      options.fetch(item).then((url) => {
        if (url) {
          fetched.set(itemId, url)
        } else {
          markFailed(itemId)
        }
      }).catch(() => {
        markFailed(itemId)
      })
    }
  }

  const src = options.items
  const itemsGetter: () => T[] = isRef(src) ? () => src.value : src
  watch(itemsGetter, (list) => fetchCovers(list), { immediate: true })

  return { coverSrc, fallbackSrc, onError, isLoading }
}
