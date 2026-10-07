<template>
  <TransitionGroup name="panel-slide">
    <div v-for="id in sessionIds" :key="id" v-show="launcher.panelAppId === id" class="app-panel-container">
      <AppSession :app-id-prop="id" :path-prop="paths.get(id)" :suspended="launcher.panelAppId !== id" @close="launcher.closePanel()" />
    </div>
  </TransitionGroup>
</template>
<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { useAppLauncherStore } from '@/stores/appLauncher'
import AppSession from '@/views/AppSession.vue'
const launcher = useAppLauncherStore()
const paths = reactive(new Map<string, string | undefined>())
// A single keyed tree owns each frame throughout visible -> retained promotion.
const sessionIds = computed(() => [...new Set([launcher.mediaAppId, launcher.panelAppId].filter((id): id is string => !!id))])
watch([() => launcher.panelAppId, () => launcher.panelPath], ([id, path]) => {
  if (!id) return
  // Reopening playback without an explicit deep link keeps the original URL.
  if (id === launcher.mediaAppId && paths.has(id) && path == null) return
  paths.set(id, path ?? undefined)
}, { immediate: true })
watch(sessionIds, ids => {
  for (const id of paths.keys()) if (!ids.includes(id)) paths.delete(id)
}, { flush: 'sync' })
</script>
<style scoped>
@media (prefers-reduced-motion: reduce) {
  .panel-slide-enter-active, .panel-slide-leave-active { transition: none !important; }
}
</style>
