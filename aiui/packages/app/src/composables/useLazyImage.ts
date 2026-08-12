import { ref, onMounted, onUnmounted, type Ref } from 'vue'

const PLACEHOLDER = 'data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTYiIGhlaWdodD0iMTYiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PHJlY3Qgd2lkdGg9IjE2IiBoZWlnaHQ9IjE2IiBmaWxsPSIjMWExYTFhIi8+PC9zdmc+'

export function useLazyImage(imageRef: Ref<HTMLImageElement | null>, src: string) {
  const isLoaded = ref(false)
  const currentSrc = ref(PLACEHOLDER)
  let observer: IntersectionObserver | null = null

  function onLoad() {
    isLoaded.value = true
  }

  onMounted(() => {
    const el = imageRef.value
    if (!el) return

    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            currentSrc.value = src
            el.src = src
            el.addEventListener('load', onLoad, { once: true })
            observer?.disconnect()
          }
        }
      },
      { rootMargin: '200px' }
    )

    observer.observe(el)
  })

  onUnmounted(() => {
    observer?.disconnect()
  })

  return { isLoaded, currentSrc, placeholder: PLACEHOLDER }
}
