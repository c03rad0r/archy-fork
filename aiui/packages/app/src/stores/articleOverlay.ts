import { defineStore } from 'pinia'
import { ref } from 'vue'

const SAFE_URL_SCHEME = /^https?:\/\//i

export const useArticleOverlayStore = defineStore('articleOverlay', () => {
  const isOpen = ref(false)
  const url = ref<string | null>(null)
  const title = ref('')
  const content = ref<string | null>(null)
  const imgSrc = ref<string | null>(null)

  function open(articleUrl: string, articleTitle = '', articleContent?: string, articleImgSrc?: string) {
    const trimmed = String(articleUrl ?? '').trim()
    if (!SAFE_URL_SCHEME.test(trimmed)) return
    try {
      new URL(trimmed)
    } catch {
      return
    }
    url.value = trimmed
    title.value = String(articleTitle ?? 'Article').slice(0, 200)
    content.value = typeof articleContent === 'string' && articleContent.trim().length > 0 ? articleContent.trim() : null
    imgSrc.value = typeof articleImgSrc === 'string' && articleImgSrc.trim().length > 0 ? articleImgSrc.trim() : null
    isOpen.value = true
  }

  function close() {
    isOpen.value = false
    url.value = null
    title.value = ''
    content.value = null
    imgSrc.value = null
  }

  return { isOpen, url, title, content, imgSrc, open, close }
})
