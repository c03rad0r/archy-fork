import { ref, onActivated, onDeactivated, type Ref } from 'vue'

/**
 * Is this view the one currently on screen?
 *
 * Main tab views are `KeepAlive`'d (see `keepAliveRoutes.ts`), which is what
 * makes revisiting a tab instant — navigating away DEACTIVATES the view rather
 * than unmounting it. That is exactly the behaviour we want for performance, and
 * exactly the trap for anything the view `Teleport`s to `<body>`: teleported
 * content is not inside the view's own DOM subtree, so it keeps rendering over
 * whatever screen the user went to next. Mesh's mobile tab bar and its chat
 * back button did this — they stayed pinned above the bottom bar on every other
 * screen.
 *
 * `BaseModal` solves the transient-dialog half of this by closing itself on any
 * route change. That is the right fix for a dialog and the wrong one for
 * persistent chrome: a tab bar has no "closed" state to fall back to, and
 * forcing one would lose the user's place. Chrome should simply not be rendered
 * while its owner is off screen, and should come back exactly as it was.
 *
 * Usage — gate the `Teleport` itself, so nothing reaches `<body>` at all:
 *
 *   const isViewActive = useViewActive()
 *   <Teleport v-if="isViewActive" to="body"> … </Teleport>
 *
 * Defaults to `true`: outside a `KeepAlive` boundary neither hook ever fires, so
 * a view used both ways (or a component mounted bare in a test) must render
 * normally rather than stay invisible forever. Inside `KeepAlive`, Vue fires
 * `onActivated` immediately after `onMounted`, so the initial `true` is correct
 * there too and there is no first-paint gap.
 */
export function useViewActive(): Ref<boolean> {
  const isViewActive = ref(true)
  onActivated(() => {
    isViewActive.value = true
  })
  onDeactivated(() => {
    isViewActive.value = false
  })
  return isViewActive
}
