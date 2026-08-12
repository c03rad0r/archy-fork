<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'

// Kiosk display resolution/scaling presets. Only shown on nodes that actually
// have a kiosk display (has_kiosk from the backend).
const hasKiosk = ref(false)
const preset = ref('auto')
const applying = ref(false)
const error = ref('')

const presets = [
  {
    id: 'auto',
    label: 'Auto (recommended)',
    description: 'Pick a comfortable size for the detected screen — 4K TVs get a Full-HD-sized layout at double sharpness.',
  },
  {
    id: 'large',
    label: 'Large UI',
    description: 'Biggest text and buttons — easiest to read from across the room, fits less on screen.',
  },
  {
    id: 'balanced',
    label: 'Balanced',
    description: 'Desktop-sized layout on any screen that can carry it — more on screen, still sharp.',
  },
  {
    id: 'native',
    label: 'Native (no scaling)',
    description: 'Use the screen’s full native resolution 1:1 — the most content, the smallest UI.',
  },
]

onMounted(async () => {
  try {
    const res = await rpcClient.call<{ has_kiosk: boolean; preset: string }>({ method: 'system.kiosk-display.get' })
    hasKiosk.value = res.has_kiosk
    preset.value = res.preset
  } catch { /* backend without the RPC — leave the section hidden */ }
})

async function apply(id: string) {
  if (applying.value || id === preset.value) return
  applying.value = true
  error.value = ''
  const prev = preset.value
  preset.value = id
  try {
    await rpcClient.call({ method: 'system.kiosk-display.set', params: { preset: id }, timeout: 20000 })
  } catch (e: unknown) {
    preset.value = prev
    error.value = e instanceof Error ? e.message : 'Failed to apply display setting'
  } finally {
    applying.value = false
  }
}
</script>

<template>
  <!-- Kiosk Display Section — only on nodes with an attached kiosk screen -->
  <div v-if="hasKiosk" class="glass-card px-6 py-6 mb-6">
    <h2 class="text-xl font-semibold text-white/96 mb-2">Display</h2>
    <p class="text-sm text-white/60 mb-6">
      How big the interface renders on the screen attached to this node. Changing this restarts the on-screen display.
    </p>
    <div data-controller-container tabindex="0" class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <button
        v-for="p in presets"
        :key="p.id"
        :disabled="applying"
        @click="apply(p.id)"
        class="path-option-card text-left p-5 disabled:opacity-60"
        :class="{ 'path-option-card--selected': preset === p.id }"
      >
        <div class="font-medium text-white/90 mb-1">{{ p.label }}</div>
        <p class="text-sm text-white/60">{{ p.description }}</p>
      </button>
    </div>
    <div v-if="error" class="mt-4 alert-error text-sm">{{ error }}</div>
  </div>
</template>
