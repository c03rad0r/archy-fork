<template>
  <div :class="currentTheme">
    <WidgetFab
      :is-open="isOpen"
      :position="fabPosition"
      @toggle="isOpen = !isOpen"
    />
    <WidgetModal
      :is-open="isOpen"
      :side="side"
      @close="isOpen = false"
      @switch-side="toggleSide"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import WidgetFab from './WidgetFab.vue'
import WidgetModal from './WidgetModal.vue'

const { currentTheme } = useTheme()

const isOpen = ref(false)
const side = ref<'left' | 'right'>('right')

const fabPosition = computed(() =>
  side.value === 'left' ? 'bottom-left' : 'bottom-right'
)

function toggleSide() {
  side.value = side.value === 'right' ? 'left' : 'right'
}
</script>
