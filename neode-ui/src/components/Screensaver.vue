<template>
  <Teleport to="body">
    <Transition name="screensaver">
      <div
        v-if="store.isActive"
        class="screensaver-container fixed inset-0 z-[3000] bg-black cursor-pointer"
        @click="store.deactivate()"
        @keydown.escape="store.deactivate()"
      >
        <!-- ASCII variant (every 3rd activation) -->
        <div v-if="store.isAsciiMode" class="screensaver-ascii-content">
          <BitcoinFaceAscii />
        </div>
        <!-- Normal logo with audio viz ring -->
        <div v-else class="screensaver-content">
          <ScreensaverRing />
          <!-- Logo in center -->
          <div class="screensaver-logo-wrapper">
            <ScreensaverLogo />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'
import ScreensaverLogo from '@/components/ScreensaverLogo.vue'
import ScreensaverRing from '@/components/ScreensaverRing.vue'
import BitcoinFaceAscii from '@/views/discover/BitcoinFaceAscii.vue'
import { useScreensaverStore } from '@/stores/screensaver'

const store = useScreensaverStore()

// Dismiss on any key (except when typing)
function onKeyDown(e: KeyboardEvent) {
  if (store.isActive) {
    store.deactivate()
    e.preventDefault()
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeyDown)
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown)
})
</script>

<style scoped>
.screensaver-enter-active,
.screensaver-leave-active {
  transition: opacity 0.5s ease;
}
.screensaver-enter-from,
.screensaver-leave-to {
  opacity: 0;
}

/* Explicit viewport centering */
.screensaver-container {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
}

.screensaver-content {
  position: relative;
  display: grid;
  place-items: center;
}

.screensaver-logo-wrapper {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  z-index: 10;
  filter: drop-shadow(0 0 40px rgba(255, 255, 255, 0.15));
}

/* ASCII variant — centered Bitcoin face animation */
.screensaver-ascii-content {
  display: flex;
  align-items: center;
  justify-content: center;
  transform: scale(2);
}

@media (min-width: 640px) {
  .screensaver-ascii-content {
    transform: scale(2.5);
  }
}

@media (min-width: 768px) {
  .screensaver-ascii-content {
    transform: scale(3);
  }
}
</style>
