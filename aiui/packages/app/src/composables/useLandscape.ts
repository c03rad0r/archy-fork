import { ref, computed, onMounted, onUnmounted } from 'vue'

export function useLandscape() {
  const windowWidth = ref(window.innerWidth)
  const windowHeight = ref(window.innerHeight)

  const isLandscape = computed(() => windowWidth.value > windowHeight.value)
  const isMobile = computed(() => Math.min(windowWidth.value, windowHeight.value) < 768)
  const isMobileLandscape = computed(() => isMobile.value && isLandscape.value)

  function onResize() {
    windowWidth.value = window.innerWidth
    windowHeight.value = window.innerHeight
  }

  onMounted(() => {
    window.addEventListener('resize', onResize)
    window.addEventListener('orientationchange', onResize)
  })

  onUnmounted(() => {
    window.removeEventListener('resize', onResize)
    window.removeEventListener('orientationchange', onResize)
  })

  return {
    isLandscape,
    isMobile,
    isMobileLandscape,
    windowWidth,
    windowHeight,
  }
}
