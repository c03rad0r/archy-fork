<template>
  <button
    type="button"
    data-controller-ignore
    class="w-full flex items-center gap-2 text-white/80 hover:text-white transition-colors"
    title="Open CLI (F)"
    @click="openCLI"
  >
    <div class="relative shrink-0">
      <div class="w-2 h-2 rounded-full bg-green-400"></div>
      <div class="absolute inset-0 w-2 h-2 rounded-full bg-green-400 animate-ping opacity-50"></div>
    </div>
    <span class="text-xs font-medium">Online</span>
  </button>
</template>

<script setup lang="ts">
import { useCLIStore } from '@/stores/cli'

const cliStore = useCLIStore()

function openCLI() {
  cliStore.open()
}
</script>
