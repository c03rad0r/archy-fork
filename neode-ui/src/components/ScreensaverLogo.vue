<template>
  <div class="logo-gradient-border screensaver-logo-cycle relative w-48 h-48 sm:w-64 sm:h-64 md:w-80 md:h-80 flex items-center justify-center overflow-hidden">
    <!-- Squares logo -->
    <div class="screensaver-logo-squares absolute inset-[3px] flex items-center justify-center">
      <AnimatedLogo size="xl" no-border fit />
    </div>
  </div>
</template>

<script setup lang="ts">
import AnimatedLogo from '@/components/AnimatedLogo.vue'
</script>

<style scoped>
.screensaver-logo-squares {
  opacity: 1;
  position: relative;
  z-index: 1;
}
</style>
