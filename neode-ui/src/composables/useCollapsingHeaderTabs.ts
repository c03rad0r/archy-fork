import { nextTick, onBeforeUnmount, onMounted, ref, type Ref } from 'vue'

export function useCollapsingHeaderTabs(
  headerRef: Ref<HTMLElement | null>,
  primaryRef: Ref<HTMLElement | null>,
  tabsProbeRef: Ref<HTMLElement | null>,
  minSearchWidth = 176,
  minTabsWidth = 260
) {
  const collapsed = ref(false)
  let resizeObserver: ResizeObserver | null = null

  function measure() {
    const header = headerRef.value
    const probe = tabsProbeRef.value
    if (!header || !probe) return

    const primaryWidth = primaryRef.value?.getBoundingClientRect().width ?? 0
    const tabsWidth = probe.getBoundingClientRect().width
    const gapWidth = 48
    const fullTabsFit = primaryWidth + tabsWidth + minSearchWidth + gapWidth <= header.clientWidth
    const usableTabsFit = primaryWidth + minTabsWidth + minSearchWidth + gapWidth <= header.clientWidth
    collapsed.value = !fullTabsFit && !usableTabsFit
  }

  function scheduleMeasure() {
    nextTick(() => requestAnimationFrame(measure))
  }

  onMounted(() => {
    scheduleMeasure()
    resizeObserver = new ResizeObserver(scheduleMeasure)
    if (headerRef.value) resizeObserver.observe(headerRef.value)
    if (primaryRef.value) resizeObserver.observe(primaryRef.value)
    if (tabsProbeRef.value) resizeObserver.observe(tabsProbeRef.value)
    window.addEventListener('resize', scheduleMeasure)
  })

  onBeforeUnmount(() => {
    resizeObserver?.disconnect()
    window.removeEventListener('resize', scheduleMeasure)
  })

  return { collapsed, measure: scheduleMeasure }
}
