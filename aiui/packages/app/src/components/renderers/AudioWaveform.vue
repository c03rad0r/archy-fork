<template>
  <div class="audio-waveform rounded-xl bg-white/5 border border-white/10 overflow-hidden">
    <div class="px-4 py-3 flex items-center gap-3">
      <!-- Play/Pause -->
      <button
        class="w-10 h-10 flex items-center justify-center rounded-full bg-accent/15 text-accent hover:bg-accent/25 transition-colors shrink-0"
        :disabled="loading"
        @click="togglePlay"
      >
        <svg v-if="!isPlaying" class="w-4 h-4 ml-0.5" fill="currentColor" viewBox="0 0 20 20">
          <path d="M6.3 2.841A1.5 1.5 0 004 4.11V15.89a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z" />
        </svg>
        <svg v-else class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
          <path d="M5.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75A.75.75 0 007.25 3h-1.5zM12.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75a.75.75 0 00-.75-.75h-1.5z" />
        </svg>
      </button>

      <!-- Waveform container -->
      <div class="flex-1 min-w-0">
        <div ref="waveformRef" class="waveform-container" />
        <div class="flex justify-between mt-1">
          <span class="text-xs text-white/30 tabular-nums">{{ formatTime(currentTime) }}</span>
          <span class="text-xs text-white/30 tabular-nums">{{ formatTime(duration) }}</span>
        </div>
      </div>
    </div>

    <div v-if="title" class="px-4 pb-3 -mt-1">
      <p class="text-xs text-white/60 truncate">{{ title }}</p>
    </div>

    <div v-if="loading" class="px-4 pb-3">
      <p class="text-xs text-white/30">Loading audio...</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, shallowRef } from 'vue'

const props = defineProps<{
  url: string
  title?: string
}>()

const waveformRef = ref<HTMLElement | null>(null)
const isPlaying = ref(false)
const currentTime = ref(0)
const duration = ref(0)
const loading = ref(true)

type WaveSurferInstance = import('wavesurfer.js').default
const ws = shallowRef<WaveSurferInstance | null>(null)

async function initWaveSurfer() {
  if (!waveformRef.value) return

  const WaveSurfer = (await import('wavesurfer.js')).default

  const instance = WaveSurfer.create({
    container: waveformRef.value,
    waveColor: '#F7931A44',
    progressColor: '#F7931A',
    cursorColor: '#F7931Acc',
    barWidth: 2,
    barGap: 1,
    barRadius: 2,
    height: 48,
    normalize: true,
    url: props.url,
    backend: 'WebAudio',
  })

  instance.on('ready', () => {
    loading.value = false
    duration.value = instance.getDuration()
  })

  instance.on('timeupdate', (time: number) => {
    currentTime.value = time
  })

  instance.on('play', () => { isPlaying.value = true })
  instance.on('pause', () => { isPlaying.value = false })
  instance.on('finish', () => { isPlaying.value = false })

  instance.on('error', () => {
    loading.value = false
  })

  ws.value = instance
}

function togglePlay() {
  ws.value?.playPause()
}

function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60)
  const s = Math.floor(seconds % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

onMounted(() => {
  initWaveSurfer()
})

onBeforeUnmount(() => {
  ws.value?.destroy()
})
</script>
