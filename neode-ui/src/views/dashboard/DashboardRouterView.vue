<template>
  <RouterView v-slot="{ Component, route }">
    <Transition :name="getTransitionName(route)">
      <KeepAlive :max="KEEP_ALIVE_MAX" :include="keepAliveIncludes">
        <component
          :is="wrapperFor(route.path)"
          :key="route.path"
          :mobile-tab-padding-top="mobileTabPaddingTop"
          :needs-mobile-back-button-space="needsMobileBackButtonSpace"
        >
          <component
            :is="Component"
            :class="isFullBleedPath(route.path) ? undefined : 'view-container flex-none'"
          />
        </component>
      </KeepAlive>
    </Transition>
  </RouterView>
</template>

<script setup lang="ts">
// The KeepAlive host extracted from Dashboard.vue's nested RouterView (D-01),
// restructured after the Task 3 checkpoint failure (broken margins + missing
// slide transitions) to restore the pre-02-02 rendered DOM exactly.
//
// Structural invariants — the tests in __tests__/keepAliveTabs.test.ts pin
// these, and dashboard-styles.css is the contract they serve:
//
// 1. The <Transition> child is a keyed `div.view-wrapper` that sits directly
//    in the RouterView slot (so it is the direct child of
//    `.perspective-container` in Dashboard.vue). Every transition in
//    dashboard-styles.css is a compound selector
//    (`.slide-up-enter-active.view-wrapper`, `.depth-forward-enter-from.view-wrapper`,
//    …), so the transition classes and `view-wrapper` MUST share one element,
//    with no intermediate wrapper flattening the 3D perspective or clipping
//    slide movement.
// 2. The per-route wrapper shapes (full-bleed chat/mesh vs. the default
//    padded/scrollable shape) live INSIDE `div.view-wrapper` — `view-wrapper`
//    is `absolute inset-0` and must never be applied to the view's own root
//    inside the padded wrapper (that pins the view over the page padding:
//    the broken-margins regression).
// 3. KeepAlive caching is reconciled with (1) and (2) by making the keyed
//    `div.view-wrapper` the root of a statically-defined per-route wrapper
//    component (dashboardViewWrappers.ts). KeepAlive sits between
//    <Transition> and the keyed wrapper vnode — the canonical composition —
//    and `:include` (matching the wrappers' static names, derived from
//    KEEP_ALIVE_PATHS) decides which wrappers are instance-cached. Caching
//    the wrapper caches its whole subtree, including the routed view.
// 4. KeepAlive itself is never keyed, toggled with v-if, or nested under a
//    per-route element — any of those tears down its instance cache.
//
// Scroll behavior: each route's scroll container lives inside its keyed
// wrapper, so non-kept routes reset to top on entry (the pre-02-02 behavior)
// and the kept-alive tab's scroll container is part of its cached subtree —
// no manual scroll-retention bookkeeping needed.
import { RouterView } from 'vue-router'
import { useRouteTransitions } from './useRouteTransitions'
import { KEEP_ALIVE_MAX } from './keepAliveRoutes'
import { isFullBleedPath, keepAliveIncludeNames, wrapperFor } from './dashboardViewWrappers'

defineProps<{
  mobileTabPaddingTop: number | null
  needsMobileBackButtonSpace: boolean
}>()

const { getTransitionName } = useRouteTransitions()
const keepAliveIncludes = keepAliveIncludeNames()
</script>
