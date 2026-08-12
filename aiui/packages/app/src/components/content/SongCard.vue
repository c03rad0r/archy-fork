<template>
  <button
    class="flex gap-3 p-2 rounded-xl transition-all duration-200 text-left w-full group overflow-hidden"
    :class="isDark
      ? 'hover:bg-white/5 active:bg-white/10'
      : 'hover:bg-black/[0.03] active:bg-black/5'"
    @click="$emit('select', song)"
  >
    <div class="cover-card-sm shrink-0 w-12 h-12 rounded-lg overflow-hidden">
      <img
        v-if="coverSrc"
        :src="coverSrc"
        :alt="song.title"
        class="w-full h-full object-cover rounded-[6px] transition-transform duration-300 group-hover:scale-105"
        loading="lazy"
        @error="coverFailed = true"
      />
      <div
        v-else
        class="w-full h-full rounded-[6px] bg-cover bg-center"
        :style="{ backgroundImage: `url(${fallbackCover})` }"
      />
    </div>

    <div class="min-w-0 flex-1 py-0.5">
      <p class="text-sm font-semibold truncate"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">{{ song.title }}</p>
      <p class="text-xs mt-0.5"
         :class="isDark ? 'text-white/40' : 'text-gray-500'">
        {{ song.artist }}<template v-if="song.year"> · {{ song.year }}</template>
      </p>
      <div class="flex items-center gap-1.5 mt-1.5">
        <span v-if="isExternal"
              class="text-xs px-1.5 py-0.5 rounded font-medium"
              :class="isDark ? 'bg-info/15 text-info/70' : 'bg-info/10 text-blue-600'">
          not in library
        </span>
        <span
          v-for="src in (song.sources ?? []).slice(0, 3)"
          :key="src.type"
          class="text-xs px-1.5 py-0.5 rounded font-medium"
          :class="isDark ? 'bg-white/8 text-white/50' : 'bg-black/5 text-gray-500'"
        >
          {{ src.type }}
        </span>
        <FavoriteButton
          class="ml-auto"
          :favorited="favoritesStore.isFavorited(song.id)"
          @toggle="favoritesStore.toggleFavorite({ id: song.id, type: 'song', title: song.title, subtitle: song.artist, data: song })"
        />
      </div>
    </div>
  </button>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { Song } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generateSongCoverFallback, fetchMusicCover } from '@/composables/useImageFallback'
import FavoriteButton from '@/components/ui/FavoriteButton.vue'
import { useFavoritesStore } from '@/stores/favorites'

const props = defineProps<{ song: Song }>()
defineEmits<{ select: [song: Song] }>()

const { isDark } = useTheme()
const favoritesStore = useFavoritesStore()
const coverFailed = ref(false)
const fetchedCover = ref<string | null>(null)

const coverSrc = computed(() => {
  if (coverFailed.value) return null
  return props.song.coverUrl || fetchedCover.value
})

const fallbackCover = computed(() =>
  generateSongCoverFallback(props.song.title, props.song.artist)
)

const isExternal = computed(() => props.song.id.startsWith('ext-'))

onMounted(() => {
  if (props.song.coverUrl) return
  fetchMusicCover(props.song.title, props.song.artist, props.song.album).then((url) => {
    if (url) fetchedCover.value = url
  })
})
</script>
