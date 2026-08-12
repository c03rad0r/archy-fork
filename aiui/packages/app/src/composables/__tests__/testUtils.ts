import { createApp, defineComponent } from 'vue'

/**
 * Helper to test composables that use lifecycle hooks (onMounted, etc.)
 * Creates a temporary Vue app, mounts it, runs the composable, and returns the result.
 */
export function withSetup<T>(composable: () => T): [T, ReturnType<typeof createApp>] {
  let result!: T
  const app = createApp(defineComponent({
    setup() {
      result = composable()
      return () => null
    },
  }))
  const div = document.createElement('div')
  app.mount(div)
  return [result, app]
}
