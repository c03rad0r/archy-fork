<template>
  <div class="app-loading-screen absolute inset-0 z-10 flex flex-col items-center justify-center">
    <div class="app-loading-icon">
      <img :src="icon" :alt="title" @error="handleImageError" />
    </div>
    <p class="app-loading-title">{{ title }}</p>
    <div class="app-loading-bar">
      <div class="app-loading-fill" :style="{ width: `${clampedProgress}%` }"></div>
    </div>
    <p class="app-loading-hint">{{ hint }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { handleImageError } from '@/views/apps/appsConfig'

const props = withDefaults(defineProps<{
  icon: string
  title: string
  progress: number
  hint?: string
}>(), {
  hint: 'Loading…',
})

const clampedProgress = computed(() => Math.min(100, Math.max(0, props.progress)))
</script>

<style scoped>
.app-loading-screen {
  gap: 18px;
  background: #0b0d12;
}
.app-loading-icon {
  width: 84px;
  height: 84px;
  border-radius: 20px;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  animation: app-loading-pulse 1.8s ease-in-out infinite;
}
.app-loading-icon img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.app-loading-title {
  margin: 0;
  font-size: 1rem;
  font-weight: 600;
  color: rgba(255, 255, 255, 0.9);
}
.app-loading-bar {
  width: min(240px, 60vw);
  height: 4px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.1);
  overflow: hidden;
}
.app-loading-fill {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, #fb923c, #f59e0b);
  transition: width 0.3s ease;
}
.app-loading-hint {
  margin: 0;
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.4);
}
@keyframes app-loading-pulse {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(1.05); opacity: 0.85; }
}
</style>
