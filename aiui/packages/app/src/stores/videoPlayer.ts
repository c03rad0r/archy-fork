import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useVideoPlayerStore = defineStore('videoPlayer', () => {
  const isOpen = ref(false)
  const videoUrl = ref('')
  const title = ref('')
  const posterUrl = ref<string | null>(null)

  function open(url: string, filmTitle: string, filmPoster?: string | null) {
    videoUrl.value = url
    title.value = filmTitle
    posterUrl.value = filmPoster ?? null
    isOpen.value = true
  }

  function close() {
    isOpen.value = false
    videoUrl.value = ''
    title.value = ''
    posterUrl.value = null
  }

  return { isOpen, videoUrl, title, posterUrl, open, close }
})
