<template>
  <!-- Desktop: subtle "frosted pill" link, sits at the top of the content flow
       (the style from the Networking Profits page, now shared globally). -->
  <button
    type="button"
    @click="$emit('click')"
    :class="['hidden md:inline-flex items-center gap-2 px-3 py-1.5 rounded-md bg-white/5 hover:bg-white/10 text-white/70 hover:text-white text-sm transition-colors', desktopMargin]"
  >
    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
    </svg>
    {{ label }}
  </button>

  <!-- Mobile: floating transparent button pinned 8px above the tab bar.
       Gated on the owning view being active: this is Teleported to <body>, so
       it lives outside the view's own subtree and KeepAlive deactivating the
       owner does NOT remove it — the button stayed pinned above the tab bar on
       every other screen. activated/deactivated propagate from the KeepAlive
       boundary down to this child, so the shared component can guard itself and
       every caller is fixed at once. -->
  <Teleport v-if="isViewActive" to="body">
    <button
      type="button"
      @click="$emit('click')"
      class="md:hidden mobile-back-btn back-button-glass px-6 py-3 rounded-xl font-medium flex items-center justify-center gap-2"
    >
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
      </svg>
      <span>{{ label }}</span>
    </button>
  </Teleport>
</template>

<script setup lang="ts">
import { useViewActive } from '@/composables/useViewActive'
/**
 * Standard back button. Renders a transparent text link at the top on desktop
 * and a floating transparent "glass" pill pinned above the tab bar on mobile —
 * the pattern set by the Cloud detail pages (PeerFiles/CloudFolder).
 *
 * Presentational only: it emits `click`; the parent keeps its own navigation
 * logic (router.push / router.back / conditional goBack).
 */
const isViewActive = useViewActive()

withDefaults(
  defineProps<{
    label?: string
    /** Desktop bottom-margin utility (views vary between mb-4 and mb-6). */
    desktopMargin?: string
  }>(),
  { label: 'Back', desktopMargin: 'mb-4' }
)

defineEmits<{ (e: 'click'): void }>()
</script>
