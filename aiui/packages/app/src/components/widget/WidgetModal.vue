<template>
  <Teleport to="body">
    <Transition name="widget">
      <div
        v-if="isOpen"
        class="fixed z-40 animate-scale-in"
        :class="[positionClasses, currentTheme]"
      >
        <div
          class="w-[380px] h-[600px] md:w-[420px] md:h-[640px] path-glass-card overflow-hidden"
          :style="isDark
            ? 'box-shadow: 0 20px 60px rgba(0, 0, 0, 0.6), 0 0 40px rgba(247, 147, 26, 0.06)'
            : 'box-shadow: 0 20px 60px rgba(0, 0, 0, 0.12), 0 0 40px rgba(247, 147, 26, 0.04)'"
        >
          <ChatWindow
            variant="modal"
            :side="side"
            show-close
            @switch-side="$emit('switchSide')"
            @close="$emit('close')"
          />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import ChatWindow from '@/components/chat/ChatWindow.vue'

const props = withDefaults(
  defineProps<{
    isOpen: boolean
    side?: 'left' | 'right'
  }>(),
  { side: 'right' }
)

defineEmits<{
  close: []
  switchSide: []
}>()

const { isDark, currentTheme } = useTheme()

const positionClasses = computed(() =>
  props.side === 'left'
    ? 'bottom-24 left-6'
    : 'bottom-24 right-6'
)
</script>

<style scoped>
.widget-enter-active {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
.widget-leave-active {
  transition: all 0.2s ease-in;
}
.widget-enter-from {
  opacity: 0;
  transform: scale(0.9) translateY(20px);
}
.widget-leave-to {
  opacity: 0;
  transform: scale(0.9) translateY(20px);
}
</style>
