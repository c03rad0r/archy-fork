import { ref } from 'vue'

export function useWebShare() {
  const isSupported = 'share' in navigator
  const showFallback = ref(false)
  const fallbackText = ref('')
  const fallbackUrl = ref('')

  async function share(data: { title?: string; text?: string; url?: string }) {
    if (isSupported) {
      try {
        await navigator.share(data)
        return true
      } catch {
        // User cancelled or not supported
      }
    }

    // Fallback: show glass share sheet
    fallbackText.value = data.text ?? data.title ?? ''
    fallbackUrl.value = data.url ?? ''
    showFallback.value = true
    return false
  }

  async function copyToClipboard(text: string) {
    try {
      await navigator.clipboard.writeText(text)
      return true
    } catch {
      return false
    }
  }

  function closeFallback() {
    showFallback.value = false
  }

  return {
    isSupported,
    showFallback,
    fallbackText,
    fallbackUrl,
    share,
    copyToClipboard,
    closeFallback,
  }
}
