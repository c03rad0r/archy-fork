import { watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useCloudStore } from '@/stores/cloud'
import { useToast } from './useToast'

/** Keep progress on its originating screen; announce completion elsewhere once. */
export function useUploadNotifications() {
  const store = useCloudStore()
  const route = useRoute()
  const router = useRouter()
  const toast = useToast()
  watch(() => store.upload?.active, (active, wasActive) => {
    const task = store.upload
    if (wasActive !== true || active !== false || !task || route.fullPath === task.originRoute) return
    const message = task.error
      ? `Upload failed · ${task.completed}/${task.count} saved: ${task.error}`
      : task.cancelled
        ? `Upload stopped · ${task.completed}/${task.count} saved`
        : `Upload complete · ${task.count === 1 ? task.filename : `${task.count} files`}`
    toast.action(message, {
      label: 'View upload',
      onClick: () => { void router.push(task.originRoute || '/dashboard/cloud') },
    }, { variant: task.error ? 'error' : task.cancelled ? 'info' : 'success', duration: 8000 })
  }, { flush: 'sync' })
}
