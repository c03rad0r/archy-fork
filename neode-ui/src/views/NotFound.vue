<script setup lang="ts">
import { RouterLink } from 'vue-router'
</script>

<template>
  <div class="min-h-screen flex items-center justify-center px-4">
    <div class="glass-card px-8 py-10 text-center max-w-md">
      <h1 class="text-6xl font-bold text-white/30 mb-4">404</h1>
      <p class="text-lg text-white/70 mb-6">Page not found</p>
      <RouterLink to="/dashboard" class="glass-button inline-block px-6 py-3">
        Back to Dashboard
      </RouterLink>
    </div>
  </div>
</template>
