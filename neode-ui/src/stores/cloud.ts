import { ref, computed } from 'vue'
import { defineStore } from 'pinia'
import { fileBrowserClient, type FileBrowserItem } from '@/api/filebrowser-client'

export const useCloudStore = defineStore('cloud', () => {
  const currentPath = ref('/')
  const items = ref<FileBrowserItem[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const authenticated = ref(false)
  // Per-path listing cache: re-entering a folder paints the last listing
  // immediately (no spinner) while the fresh listing loads behind it.
  const pathCache = new Map<string, FileBrowserItem[]>()
  // Last-wins guard for overlapping navigations (fast folder hopping).
  let navSeq = 0

  const breadcrumbs = computed(() => {
    const parts = currentPath.value.split('/').filter(Boolean)
    const crumbs = [{ name: 'Home', path: '/' }]
    let path = ''
    for (const part of parts) {
      path += `/${part}`
      crumbs.push({ name: part, path })
    }
    return crumbs
  })

  const sortedItems = computed(() => {
    const dirs = items.value.filter((i) => i.isDir)
    const files = items.value.filter((i) => !i.isDir)
    dirs.sort((a, b) => a.name.localeCompare(b.name))
    files.sort((a, b) => a.name.localeCompare(b.name))
    return [...dirs, ...files]
  })

  async function init(): Promise<boolean> {
    if (authenticated.value) return true
    const ok = await fileBrowserClient.login()
    authenticated.value = ok
    return ok
  }

  async function navigate(path: string): Promise<void> {
    const seq = ++navSeq
    const apply = (p: string, result: FileBrowserItem[]) => {
      pathCache.set(p, result)
      if (seq !== navSeq) return // a newer navigation superseded this one
      items.value = result
      currentPath.value = p
    }
    // Stale-while-revalidate: show the cached listing for this path
    // immediately (no spinner), then refresh it underneath.
    const cached = pathCache.get(path)
    if (cached) {
      items.value = cached
      currentPath.value = path
    } else {
      loading.value = true
    }
    error.value = null
    try {
      if (!authenticated.value) {
        const ok = await init()
        if (!ok) {
          error.value = 'Failed to authenticate with File Browser'
          return
        }
      }
      try {
        apply(path, await fileBrowserClient.listDirectory(path))
      } catch {
        // Directory may not exist — try to create it, then retry
        if (path !== '/') {
          try {
            const parentPath = path.substring(0, path.lastIndexOf('/')) || '/'
            const dirName = path.substring(path.lastIndexOf('/') + 1)
            await fileBrowserClient.createFolder(parentPath, dirName)
            apply(path, await fileBrowserClient.listDirectory(path))
          } catch {
            // Fall back to root
            apply('/', await fileBrowserClient.listDirectory('/'))
          }
        } else {
          throw new Error('Failed to list root directory')
        }
      }
    } catch (e) {
      // Keep showing the cached listing on a failed revalidate.
      if (!cached) error.value = e instanceof Error ? e.message : 'Failed to load files'
    } finally {
      if (seq === navSeq) loading.value = false
    }
  }

  async function refresh(): Promise<void> {
    await navigate(currentPath.value)
  }

  async function uploadFile(file: File): Promise<void> {
    await fileBrowserClient.upload(currentPath.value, file)
    await refresh()
  }

  async function deleteItem(path: string): Promise<void> {
    await fileBrowserClient.deleteItem(path)
    await refresh()
  }

  function downloadUrl(path: string): string {
    return fileBrowserClient.downloadUrl(path)
  }

  async function fetchBlobUrl(path: string): Promise<string> {
    return fileBrowserClient.fetchBlobUrl(path)
  }

  async function streamUrl(path: string): Promise<string> {
    return fileBrowserClient.streamUrl(path)
  }

  async function downloadFile(path: string): Promise<void> {
    return fileBrowserClient.downloadFile(path)
  }

  function reset(): void {
    currentPath.value = '/'
    items.value = []
    loading.value = false
    error.value = null
    pathCache.clear()
  }

  return {
    currentPath,
    items,
    loading,
    error,
    authenticated,
    breadcrumbs,
    sortedItems,
    init,
    navigate,
    refresh,
    uploadFile,
    deleteItem,
    downloadUrl,
    fetchBlobUrl,
    streamUrl,
    downloadFile,
    reset,
  }
})
