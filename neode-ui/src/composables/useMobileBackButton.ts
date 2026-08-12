import { ref, computed, onMounted, onBeforeUnmount } from 'vue'

/**
 * Composable for mobile back button positioning
 * Ensures back buttons are always 8px above the mobile tab bar
 * Uses ResizeObserver to reactively update when tab bar height changes
 */
export function useMobileBackButton() {
  const tabBarHeight = ref<number>(72) // Default fallback height
  
  // Computed property for bottom position - always 16px above tab bar
  const bottomPosition = computed(() => {
    return `${tabBarHeight.value + 8}px`
  })
  
  // Computed property for Tailwind class (for use in class bindings)
  const bottomClass = computed(() => {
    // Use Tailwind arbitrary value with the computed height
    return `bottom-[${tabBarHeight.value + 8}px]`
  })
  
  let resizeObserver: ResizeObserver | null = null
  let mutationObserver: MutationObserver | null = null
  let intervalId: ReturnType<typeof setInterval> | null = null
  
  function updateTabBarHeight() {
    if (typeof window === 'undefined') return

    // Try to find the mobile tab bar element
    const tabBar = document.querySelector('[data-mobile-tab-bar]') as HTMLElement
    if (tabBar && tabBar.offsetHeight > 0) {
      tabBarHeight.value = tabBar.offsetHeight
      return
    }

    // Fallback: read from CSS variable if available
    const cssVar = getComputedStyle(document.documentElement)
      .getPropertyValue('--mobile-tab-bar-height')
      .trim()

    if (cssVar) {
      const height = parseFloat(cssVar)
      if (!isNaN(height) && height > 0) {
        tabBarHeight.value = height
        return
      }
    }

    // Final fallback: keep current value (don't reset to 0)
  }
  
  onMounted(() => {
    // Initial update
    updateTabBarHeight()
    
    // Watch for CSS variable changes
    mutationObserver = new MutationObserver(() => {
      updateTabBarHeight()
    })
    
    mutationObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['style'],
    })
    
    // Watch for tab bar size changes
    const tabBar = document.querySelector('[data-mobile-tab-bar]') as HTMLElement
    if (tabBar && 'ResizeObserver' in window) {
      resizeObserver = new ResizeObserver(() => {
        updateTabBarHeight()
      })
      resizeObserver.observe(tabBar)
    }
    
    // Also listen to window resize as fallback
    window.addEventListener('resize', updateTabBarHeight)
    
    // Periodic check to ensure we're always in sync (safety net)
    intervalId = setInterval(() => {
      updateTabBarHeight()
    }, 1000)
  })
  
  onBeforeUnmount(() => {
    if (mutationObserver) {
      mutationObserver.disconnect()
    }
    if (resizeObserver) {
      resizeObserver.disconnect()
    }
    if (intervalId) {
      clearInterval(intervalId)
    }
    window.removeEventListener('resize', updateTabBarHeight)
  })
  
  return {
    bottomPosition,
    bottomClass,
    tabBarHeight,
  }
}

