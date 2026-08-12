<template>
  <div class="flex items-center gap-2">
    <!-- Status Indicator -->
    <div class="relative">
      <div 
        class="w-3 h-3 rounded-full transition-colors"
        :class="statusClass"
      ></div>
      <div 
        v-if="isRunning"
        class="absolute inset-0 w-3 h-3 rounded-full animate-ping opacity-75"
        :class="statusClass"
      ></div>
    </div>
    
    <!-- Status Text -->
    <span class="text-sm font-medium" :class="textClass">
      {{ statusText }}
    </span>
    
    <!-- Health Badge (if running) -->
    <span 
      v-if="isRunning && health"
      class="px-2 py-0.5 rounded text-xs font-medium"
      :class="healthClass"
    >
      {{ healthText }}
    </span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  state:
    | 'created'
    | 'running'
    | 'stopped'
    | 'exited'
    | 'paused'
    | 'unknown'
    | 'stopping'
    | 'starting'
    | 'restarting'
    | 'installing'
    | 'updating'
    | 'removing'
    | 'installed'
  health?: 'healthy' | 'unhealthy' | 'unknown' | 'starting'
}

const props = withDefaults(defineProps<Props>(), {
  health: 'unknown',
})

const isRunning = computed(() => props.state === 'running')

const statusClass = computed(() => {
  switch (props.state) {
    case 'running':
      return 'bg-green-400'
    case 'stopped':
    case 'exited':
    case 'installed':
      return 'bg-gray-400'
    case 'paused':
    case 'starting':
    case 'stopping':
    case 'restarting':
    case 'installing':
    case 'updating':
    case 'removing':
      return 'bg-yellow-400'
    default:
      return 'bg-red-400'
  }
})

const textClass = computed(() => {
  switch (props.state) {
    case 'running':
      return 'text-green-400'
    case 'stopped':
    case 'exited':
    case 'installed':
      return 'text-gray-400'
    case 'paused':
    case 'starting':
    case 'stopping':
    case 'restarting':
    case 'installing':
    case 'updating':
    case 'removing':
      return 'text-yellow-400'
    default:
      return 'text-red-400'
  }
})

const statusText = computed(() => {
  switch (props.state) {
    case 'running':
      return 'Running'
    case 'stopped':
      return 'Stopped'
    case 'exited':
      return 'Exited'
    case 'paused':
      return 'Paused'
    case 'created':
      return 'Created'
    case 'installed':
      return 'Installed'
    case 'starting':
      return 'Starting…'
    case 'stopping':
      return 'Stopping…'
    case 'restarting':
      return 'Restarting…'
    case 'installing':
      return 'Installing…'
    case 'updating':
      return 'Updating…'
    case 'removing':
      return 'Removing…'
    default:
      return 'Unknown'
  }
})

const healthClass = computed(() => {
  switch (props.health) {
    case 'healthy':
      return 'bg-green-500/20 text-green-400 border border-green-500/30'
    case 'unhealthy':
      return 'bg-red-500/20 text-red-400 border border-red-500/30'
    case 'starting':
      return 'bg-yellow-500/20 text-yellow-400 border border-yellow-500/30'
    default:
      return 'bg-gray-500/20 text-gray-400 border border-gray-500/30'
  }
})

const healthText = computed(() => {
  switch (props.health) {
    case 'healthy':
      return 'Healthy'
    case 'unhealthy':
      return 'Unhealthy'
    case 'starting':
      return 'Starting'
    default:
      return 'Unknown'
  }
})
</script>
