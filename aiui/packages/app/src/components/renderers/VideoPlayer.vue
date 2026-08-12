<template>
  <div class="video-player rounded-xl bg-white/5 border border-white/10 overflow-hidden">
    <!-- YouTube embed -->
    <div v-if="youtubeId" class="relative w-full" style="aspect-ratio: 16/9">
      <iframe
        :src="`https://www.youtube-nocookie.com/embed/${youtubeId}`"
        class="absolute inset-0 w-full h-full"
        allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
        allowfullscreen
        title="Video"
      />
    </div>

    <!-- Native video -->
    <div v-else class="relative w-full" style="aspect-ratio: 16/9">
      <video
        ref="videoRef"
        class="absolute inset-0 w-full h-full bg-black"
        controls
        playsinline
        preload="metadata"
        :poster="poster"
      >
        <source v-if="!isHls" :src="url" />
      </video>
    </div>

    <!-- Title bar -->
    <div v-if="title" class="px-3 py-2 border-t border-white/5">
      <p class="text-xs text-white/60 truncate">{{ title }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, shallowRef, watch } from 'vue'

const props = defineProps<{
  url: string
  title?: string
  poster?: string
}>()

const videoRef = ref<HTMLVideoElement | null>(null)

// YouTube detection
const youtubeId = computed(() => {
  const ytRe = /(?:youtube\.com\/(?:watch\?v=|embed\/)|youtu\.be\/)([a-zA-Z0-9_-]{11})/
  const m = props.url.match(ytRe)
  return m ? m[1] : null
})

// HLS detection
const isHls = computed(() =>
  /\.m3u8(\?|$)/i.test(props.url) || /\.hls(\?|$)/i.test(props.url)
)

type HlsInstance = import('hls.js').default
const hls = shallowRef<HlsInstance | null>(null)

const hlsError = ref('')

async function initHls() {
  if (!isHls.value || !videoRef.value || youtubeId.value) return

  try {
    const HlsModule = await import('hls.js')
    const Hls = HlsModule.default

    if (!Hls.isSupported()) {
      // Try native HLS support (Safari)
      if (videoRef.value.canPlayType('application/vnd.apple.mpegurl')) {
        videoRef.value.src = props.url
      }
      return
    }

    const instance = new Hls()
    instance.loadSource(props.url)
    instance.attachMedia(videoRef.value)
    hls.value = instance
  } catch (e) {
    hlsError.value = `Failed to initialize video: ${e instanceof Error ? e.message : 'Unknown error'}`
  }
}

onMounted(() => {
  if (isHls.value) {
    initHls()
  }
})

watch(() => props.url, () => {
  hls.value?.destroy()
  hls.value = null
  if (isHls.value) initHls()
})

onBeforeUnmount(() => {
  hls.value?.destroy()
})
</script>
