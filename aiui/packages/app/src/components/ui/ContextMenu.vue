<template>
  <Teleport to="body">
    <div
      v-if="isOpen"
      class="fixed inset-0 z-[9998]"
      aria-hidden="true"
      @click="close"
      @contextmenu.prevent="close"
    />
    <Transition name="context-menu">
      <div
        v-if="isOpen"
        class="fixed z-[9999] glass-card p-1.5 rounded-xl shadow-2xl animate-scale-in min-w-[140px]"
        :style="positionStyle"
        role="menu"
        @click.stop
      >
        <slot />
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'

const isOpen = ref(false)
const x = ref(0)
const y = ref(0)

const positionStyle = computed(() => {
  // Keep menu within viewport
  const menuWidth = 160
  const menuHeight = 200
  const adjustedX = Math.min(x.value, window.innerWidth - menuWidth - 8)
  const adjustedY = Math.min(y.value, window.innerHeight - menuHeight - 8)
  return {
    left: `${Math.max(8, adjustedX)}px`,
    top: `${Math.max(8, adjustedY)}px`,
  }
})

function open(clientX: number, clientY: number) {
  x.value = clientX
  y.value = clientY
  isOpen.value = true
}

function close() {
  isOpen.value = false
}

function handleEscape(e: KeyboardEvent) {
  if (e.key === 'Escape' && isOpen.value) {
    close()
  }
}

onMounted(() => {
  document.addEventListener('keydown', handleEscape)
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleEscape)
})

defineExpose({ open, close, isOpen })
</script>

<style scoped>
.context-menu-enter-active {
  transition: all 0.15s cubic-bezier(0.22, 1, 0.36, 1);
}
.context-menu-leave-active {
  transition: all 0.1s ease-in;
}
.context-menu-enter-from,
.context-menu-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>
