import { ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import type { FileBrowserItem } from '@/api/filebrowser-client'
import { useAudioPlayer } from './useAudioPlayer'

/**
 * Shape of a purchased item as listed by `content.owned-list` — the minimum
 * fields this composable needs to fetch and route it. Cloud.vue's `PaidItem`
 * interface satisfies this structurally.
 */
export interface OwnedItemLike {
  onion: string
  content_id: string
  filename: string
  mime_type: string
  size_bytes: number
}

/** Same key formula the Paid Files row uses for `:key` — one key, one row. */
export function paidItemKey(it: OwnedItemLike): string {
  return it.onion + it.content_id
}

/**
 * Validate ownership, stream to viewer and manage loading/error state for a purchased
 * item (UIFIX-04 / UIFIX-06).
 *
 * - image/video → routed into the caller's MediaLightbox via `lightboxItems`
 *   / `lightboxIndex`, fed a synthetic FileBrowserItem whose blob URL is
 *   served back through `resolveBlobUrl`. The lightbox owns revoking that
 *   URL on unmount — this composable must never schedule a competing revoke
 *   for a URL it has handed to the lightbox.
 * - audio → today's global bottom-bar player, unchanged.
 * - anything else (no in-app viewer) → today's browser-tab fallback,
 *   unchanged, including its own revoke timer.
 */
export function usePaidItemViewer() {
  const audioPlayer = useAudioPlayer()

  const opening = ref<string | null>(null)
  const error = ref<string | null>(null)
  const lightboxItems = ref<FileBrowserItem[]>([])
  const lightboxIndex = ref<number | null>(null)

  // Synthetic-path -> authenticated cache URL (or legacy blob URL). Populated only for items
  // routed to the lightbox; resolveBlobUrl reads from here and never fetches.
  const urlByPath = new Map<string, string>()
  // One in-flight fetch per item key — a second open() for the same key
  // while the first is pending returns the same promise instead of issuing
  // a second RPC (UIFIX-04 concurrency edge / T-01-65).
  const inFlight = new Map<string, Promise<void>>()

  async function resolveBlobUrl(path: string): Promise<string> {
    const url = urlByPath.get(path)
    if (!url) throw new Error('Not resolved')
    return url
  }

  function runOpen(it: OwnedItemLike, key: string): Promise<void> {
    return (async () => {
      opening.value = key
      error.value = null
      try {
        const res = await rpcClient.call<{ data_base64?: string; data?: string; owned?: boolean; mime_type?: string; error?: string }>({
          method: 'content.owned-get',
          params: { onion: it.onion, content_id: it.content_id, cache_only: true },
          timeout: 60000,
        })
        const b64 = res.data_base64 ?? res.data
        if (b64 === undefined && res.owned !== true) {
          error.value = res.error || "Couldn't open this file — its cached copy is unavailable."
          return
        }
        const mime = res.mime_type || it.mime_type
        const url = b64 === undefined
          ? `/api/peer-content/${encodeURIComponent(it.onion)}/${encodeURIComponent(it.content_id)}`
          : URL.createObjectURL(new Blob([Uint8Array.from(atob(b64), c => c.charCodeAt(0))], { type: mime }))

        // Music ALWAYS plays in the global bottom-bar player — never a
        // lightbox (blob URL stays alive for the bar; it owns playback now).
        if (mime.startsWith('audio/')) {
          audioPlayer.play(url, it.filename.split('/').pop() || it.filename)
          return
        }

        if (mime.startsWith('image/') || mime.startsWith('video/')) {
          const basename = it.filename.split('/').pop() || it.filename
          const path = `paid://${key}`
          urlByPath.set(path, url)
          const synthetic: FileBrowserItem = {
            name: basename,
            path,
            size: it.size_bytes,
            modified: '',
            isDir: false,
            type: mime,
            extension: basename.includes('.') ? basename.split('.').pop()!.toLowerCase() : '',
          }
          lightboxItems.value = [synthetic]
          lightboxIndex.value = 0
          return
        }

        // No in-app viewer (documents, etc.) — keep today's behaviour.
        window.open(url, '_blank', 'noopener')
        if (url.startsWith("blob:")) setTimeout(() => URL.revokeObjectURL(url), 60000)
      } catch {
        error.value = "Couldn't open this file — it may be unavailable right now."
      } finally {
        opening.value = null
        inFlight.delete(key)
      }
    })()
  }

  function open(it: OwnedItemLike): Promise<void> {
    const key = paidItemKey(it)
    const existing = inFlight.get(key)
    if (existing) return existing
    const task = runOpen(it, key)
    inFlight.set(key, task)
    return task
  }

  function closeLightbox() {
    lightboxIndex.value = null
  }

  return {
    opening,
    error,
    lightboxItems,
    lightboxIndex,
    resolveBlobUrl,
    open,
    closeLightbox,
  }
}
