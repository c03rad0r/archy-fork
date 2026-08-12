<template>
  <Teleport to="body">
    <Transition name="sheet">
      <div v-if="isOpen" class="fixed inset-0 z-50" @click.self="close">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50" @click="close" />

        <!-- Sheet -->
        <div
          ref="sheetRef"
          class="absolute bottom-0 left-0 right-0 rounded-t-2xl bg-[#0a0a0a] border-t border-white/10 overflow-hidden"
          :style="{ maxHeight: `${snapHeight}dvh`, height: `${currentHeight}dvh` }"
          @touchstart="onTouchStart"
          @touchmove="onTouchMove"
          @touchend="onTouchEnd"
        >
          <!-- Drag handle -->
          <div class="flex justify-center py-3 cursor-grab" @mousedown="onMouseDown">
            <div class="w-10 h-1 rounded-full bg-white/20" />
          </div>

          <!-- Content -->
          <div class="overflow-y-auto custom-scrollbar" :style="{ maxHeight: `calc(${currentHeight}dvh - 40px)` }">
            <slot />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'

const props = withDefaults(defineProps<{
  isOpen: boolean
  snapPoints?: number[]
}>(), {
  snapPoints: () => [40, 80, 100],
})

const emit = defineEmits<{
  close: []
  'update:isOpen': [value: boolean]
}>()

const sheetRef = ref<HTMLElement | null>(null)
const currentSnap = ref(0) // index into snapPoints
const currentHeight = ref(props.snapPoints[0])
const snapHeight = ref(100)

let startY = 0
let startHeight = 0

watch(() => props.isOpen, (open) => {
  if (open) {
    currentSnap.value = 0
    currentHeight.value = props.snapPoints[0]
  }
})

function close() {
  emit('close')
  emit('update:isOpen', false)
}

function onTouchStart(e: TouchEvent) {
  startY = e.touches[0].clientY
  startHeight = currentHeight.value
}

function onTouchMove(e: TouchEvent) {
  const dy = startY - e.touches[0].clientY
  const vh = window.innerHeight / 100
  const newHeight = startHeight + dy / vh
  currentHeight.value = Math.max(10, Math.min(100, newHeight))
}

function onTouchEnd() {
  // Snap to nearest point
  const nearest = props.snapPoints.reduce((prev, curr) =>
    Math.abs(curr - currentHeight.value) < Math.abs(prev - currentHeight.value) ? curr : prev
  )

  if (currentHeight.value < 15) {
    close()
    return
  }

  currentHeight.value = nearest
  currentSnap.value = props.snapPoints.indexOf(nearest)
}

function onMouseDown(e: MouseEvent) {
  startY = e.clientY
  startHeight = currentHeight.value

  const onMove = (ev: MouseEvent) => {
    const dy = startY - ev.clientY
    const vh = window.innerHeight / 100
    currentHeight.value = Math.max(10, Math.min(100, startHeight + dy / vh))
  }

  const onUp = () => {
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
    onTouchEnd()
  }

  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}
</script>

<style scoped>
.sheet-enter-active {
  transition: all 0.3s cubic-bezier(0.22, 1, 0.36, 1);
}
.sheet-leave-active {
  transition: all 0.2s ease-in;
}
.sheet-enter-from,
.sheet-leave-to {
  transform: translateY(100%);
  opacity: 0;
}
</style>
