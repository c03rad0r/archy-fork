<template>
  <div class="h-full flex flex-col">
    <div class="p-4 border-b border-white/[0.08]">
      <h3 class="text-sm font-bold text-white/90 mb-3">Discover</h3>

      <!-- Sub-tabs -->
      <div class="flex gap-1.5 flex-wrap">
        <button
          v-for="tab in subTabs"
          :key="tab.id"
          class="text-xs px-2 py-1 rounded-md transition-all duration-150"
          :class="activeSubTab === tab.id
            ? 'nav-tab-active'
            : 'text-white/40 hover:text-white/70 hover:bg-white/5'"
          @click="activeSubTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-2">
      <!-- For You -->
      <template v-if="activeSubTab === 'foryou'">
        <div v-if="forYouItems.length === 0" class="flex flex-col items-center justify-center py-12 gap-2">
          <svg class="w-8 h-8 text-white/10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
          </svg>
          <p class="text-xs text-white/30">Add favorites to get personalized suggestions</p>
        </div>

        <div
          v-for="item in forYouItems.slice(0, 30)"
          :key="item.id"
          class="flex items-center gap-3 p-3 rounded-xl bg-white/[0.03] hover:bg-white/[0.07] border border-white/5 transition-all duration-150 cursor-pointer"
        >
          <span class="text-xs w-6 h-6 rounded flex items-center justify-center shrink-0" :class="typeStyle(item.type)">
            {{ typeIcon(item.type) }}
          </span>
          <div class="flex-1 min-w-0">
            <div class="text-xs font-semibold truncate text-white/80">{{ item.title }}</div>
            <div v-if="item.subtitle" class="text-xs truncate text-white/40">{{ item.subtitle }}</div>
          </div>
          <span class="text-xs text-white/20 shrink-0">{{ item.type }}</span>
        </div>
      </template>

      <!-- Recent -->
      <template v-else-if="activeSubTab === 'recent'">
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs text-white/30">{{ viewHistory.length }} items</span>
          <button
            v-if="viewHistory.length > 0"
            class="text-xs text-red-400/50 hover:text-red-400/80 transition-colors"
            @click="clearHistory"
          >
            Clear
          </button>
        </div>

        <div v-if="viewHistory.length === 0" class="flex flex-col items-center justify-center py-12 gap-2">
          <svg class="w-8 h-8 text-white/10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <p class="text-xs text-white/30">No recently viewed items</p>
        </div>

        <div
          v-for="entry in viewHistory"
          :key="entry.id + entry.viewedAt"
          class="flex items-center gap-3 p-3 rounded-xl bg-white/[0.03] hover:bg-white/[0.07] border border-white/5 transition-all duration-150 cursor-pointer"
        >
          <span class="text-xs w-6 h-6 rounded flex items-center justify-center shrink-0" :class="typeStyle(entry.type)">
            {{ typeIcon(entry.type) }}
          </span>
          <div class="flex-1 min-w-0">
            <div class="text-xs font-semibold truncate text-white/80">{{ entry.title }}</div>
            <div v-if="entry.subtitle" class="text-xs truncate text-white/40">{{ entry.subtitle }}</div>
          </div>
          <span class="text-xs text-white/20 shrink-0">{{ timeAgo(entry.viewedAt) }}</span>
        </div>
      </template>

      <!-- Trending -->
      <template v-else-if="activeSubTab === 'trending'">
        <div v-if="trending.length === 0" class="flex flex-col items-center justify-center py-12 gap-2">
          <svg class="w-8 h-8 text-white/10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" />
          </svg>
          <p class="text-xs text-white/30">No trending items yet</p>
        </div>

        <div
          v-for="item in trending"
          :key="item.id"
          class="flex items-center gap-3 p-3 rounded-xl bg-white/[0.03] hover:bg-white/[0.07] border border-white/5 transition-all duration-150"
        >
          <span class="text-xs w-6 h-6 rounded flex items-center justify-center shrink-0" :class="typeStyle(item.type)">
            {{ typeIcon(item.type) }}
          </span>
          <div class="flex-1 min-w-0">
            <div class="text-xs font-semibold truncate text-white/80">{{ item.title }}</div>
          </div>
          <span class="text-xs px-1.5 py-0.5 rounded bg-accent/15 text-accent/80 shrink-0">
            {{ item.count }}x
          </span>
        </div>
      </template>

      <!-- Collections -->
      <template v-else-if="activeSubTab === 'collections'">
        <!-- Create new -->
        <div class="flex gap-2 mb-3">
          <input
            v-model="newCollectionName"
            type="text"
            placeholder="New collection name..."
            class="flex-1 px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors"
            @keydown.enter="createNewCollection"
          />
          <button
            class="px-2.5 py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
            :disabled="!newCollectionName.trim()"
            @click="createNewCollection"
          >
            Create
          </button>
        </div>

        <div v-if="collections.length === 0" class="flex flex-col items-center justify-center py-12 gap-2">
          <svg class="w-8 h-8 text-white/10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
          </svg>
          <p class="text-xs text-white/30">No collections yet</p>
        </div>

        <div
          v-for="col in collections"
          :key="col.id"
          class="rounded-xl bg-white/[0.03] border border-white/5 p-3 space-y-2"
        >
          <div class="flex items-center justify-between">
            <div class="flex-1 min-w-0">
              <p class="text-xs font-semibold text-white/80 truncate">{{ col.name }}</p>
              <p v-if="col.description" class="text-xs text-white/30 truncate">{{ col.description }}</p>
            </div>
            <div class="flex items-center gap-1 shrink-0">
              <span class="text-xs text-white/25">{{ col.items.length }} items</span>
              <button
                class="text-xs px-1.5 py-0.5 rounded text-red-400/50 hover:text-red-400/80 hover:bg-red-400/10 transition-colors"
                @click="deleteCollection(col.id)"
              >
                Delete
              </button>
            </div>
          </div>

          <!-- Mosaic thumbnails -->
          <div v-if="col.items.length > 0" class="grid grid-cols-4 gap-1">
            <div
              v-for="item in col.items.slice(0, 4)"
              :key="item.id"
              class="aspect-square rounded bg-white/5 flex items-center justify-center"
            >
              <span class="text-xs font-bold" :class="typeStyle(item.type)">{{ typeIcon(item.type) }}</span>
            </div>
          </div>

          <!-- Items list -->
          <div v-for="item in col.items" :key="item.id" class="flex items-center gap-2 text-xs">
            <span :class="typeStyle(item.type)" class="w-4 h-4 rounded flex items-center justify-center text-[7px] shrink-0">{{ typeIcon(item.type) }}</span>
            <span class="text-white/60 truncate flex-1">{{ item.title }}</span>
            <button
              class="text-red-400/40 hover:text-red-400/70 transition-colors text-xs shrink-0"
              @click="removeFromCollection(col.id, item.id)"
            >
              x
            </button>
          </div>
        </div>
      </template>

      <!-- Tags -->
      <template v-else-if="activeSubTab === 'tags'">
        <div v-if="tagCloud.length === 0" class="flex flex-col items-center justify-center py-12 gap-2">
          <svg class="w-8 h-8 text-white/10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A1.994 1.994 0 013 12V7a4 4 0 014-4z" />
          </svg>
          <p class="text-xs text-white/30">No tags yet</p>
          <p class="text-xs text-white/20">Tag items from content cards to organize them</p>
        </div>

        <!-- Tag cloud -->
        <div v-if="tagCloud.length > 0" class="flex flex-wrap gap-1.5 mb-4">
          <button
            v-for="tc in tagCloud"
            :key="tc.tag"
            class="text-xs px-2 py-1 rounded-md transition-all duration-150"
            :class="activeTagFilter === tc.tag
              ? 'nav-tab-active'
              : 'text-white/40 hover:text-white/70 bg-white/5 hover:bg-white/10'"
            @click="activeTagFilter = activeTagFilter === tc.tag ? null : tc.tag"
          >
            {{ tc.tag }} <span class="text-white/20 ml-0.5">{{ tc.count }}</span>
          </button>
        </div>

        <!-- Filtered items by tag -->
        <div v-if="activeTagFilter" class="space-y-2">
          <p class="text-xs text-white/30">Items tagged "{{ activeTagFilter }}"</p>
          <div
            v-for="itemId in getItemsByTag(activeTagFilter)"
            :key="itemId"
            class="flex items-center gap-2 p-2.5 rounded-xl bg-white/[0.03] border border-white/5"
          >
            <span class="text-xs text-white/60 font-mono truncate">{{ itemId }}</span>
            <button
              class="text-xs text-red-400/50 hover:text-red-400/80 transition-colors shrink-0"
              @click="removeTag(itemId, activeTagFilter!)"
            >
              untag
            </button>
          </div>
        </div>
      </template>

      <!-- Smart Playlists -->
      <template v-else-if="activeSubTab === 'playlists'">
        <div v-if="!hasAnySongs" class="flex flex-col items-center justify-center py-12 gap-2">
          <svg class="w-8 h-8 text-white/10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
          </svg>
          <p class="text-xs text-white/30">No music data yet</p>
        </div>

        <template v-else>
          <!-- Recently played songs -->
          <div v-if="recentSongs.length > 0" class="mb-4">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold mb-2">Recently Played</p>
            <div
              v-for="entry in recentSongs.slice(0, 10)"
              :key="entry.id"
              class="flex items-center gap-2 p-2 rounded-lg bg-white/[0.03] hover:bg-white/[0.07] border border-white/5 mb-1 transition-colors"
            >
              <span class="text-xs w-5 h-5 rounded flex items-center justify-center shrink-0 bg-green-500/20 text-green-400">S</span>
              <div class="flex-1 min-w-0">
                <p class="text-xs text-white/70 truncate">{{ entry.title }}</p>
                <p v-if="entry.subtitle" class="text-xs text-white/30 truncate">{{ entry.subtitle }}</p>
              </div>
              <span class="text-xs text-white/20 shrink-0">{{ timeAgo(entry.viewedAt) }}</span>
            </div>
          </div>

          <!-- Most played songs -->
          <div v-if="mostPlayedSongs.length > 0" class="mb-4">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold mb-2">Most Played</p>
            <div
              v-for="item in mostPlayedSongs.slice(0, 10)"
              :key="item.id"
              class="flex items-center gap-2 p-2 rounded-lg bg-white/[0.03] hover:bg-white/[0.07] border border-white/5 mb-1 transition-colors"
            >
              <span class="text-xs w-5 h-5 rounded flex items-center justify-center shrink-0 bg-green-500/20 text-green-400">S</span>
              <div class="flex-1 min-w-0">
                <p class="text-xs text-white/70 truncate">{{ item.title }}</p>
              </div>
              <span class="text-xs px-1.5 py-0.5 rounded bg-green-400/15 text-green-400/80 shrink-0">{{ item.count }}x</span>
            </div>
          </div>

          <!-- By genre -->
          <div v-if="songsByGenre.length > 0" class="mb-4">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold mb-2">By Genre</p>
            <div class="flex flex-wrap gap-1.5">
              <button
                v-for="genre in songsByGenre"
                :key="genre.genre"
                class="text-xs px-2 py-1 rounded-md bg-white/5 text-white/50 hover:text-white/70 hover:bg-white/10 transition-colors"
              >
                {{ genre.genre }} <span class="text-white/20">{{ genre.count }}</span>
              </button>
            </div>
          </div>

          <!-- By decade -->
          <div v-if="songsByDecade.length > 0">
            <p class="text-xs text-accent/60 uppercase tracking-wider font-bold mb-2">By Decade</p>
            <div class="flex flex-wrap gap-1.5">
              <button
                v-for="dec in songsByDecade"
                :key="dec.decade"
                class="text-xs px-2 py-1 rounded-md bg-white/5 text-white/50 hover:text-white/70 hover:bg-white/10 transition-colors"
              >
                {{ dec.decade }}s <span class="text-white/20">{{ dec.count }}</span>
              </button>
            </div>
          </div>
        </template>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useForYouFeed, useContentTags, useViewHistory, useTrending } from '@/composables/useContentDiscovery'
import { useContentCollections } from '@/composables/useContentCollections'
import { useFavoritesStore, type FavoriteType } from '@/stores/favorites'

type SubTab = 'foryou' | 'recent' | 'trending' | 'collections' | 'tags' | 'playlists'

const subTabs: { id: SubTab; label: string }[] = [
  { id: 'foryou', label: 'For You' },
  { id: 'recent', label: 'Recent' },
  { id: 'trending', label: 'Trending' },
  { id: 'collections', label: 'Collections' },
  { id: 'tags', label: 'Tags' },
  { id: 'playlists', label: 'Playlists' },
]

const activeSubTab = ref<SubTab>('foryou')

// M13.1 — For You
const { forYouItems } = useForYouFeed()

// M13.2 — Tags
const { tagCloud, getItemsByTag, removeTag } = useContentTags()
const activeTagFilter = ref<string | null>(null)

// M13.5 — Recent
const { viewHistory, clearHistory } = useViewHistory()

// M13.6 — Collections
const { collections, createCollection, deleteCollection, removeFromCollection } = useContentCollections()
const newCollectionName = ref('')

function createNewCollection() {
  const name = newCollectionName.value.trim()
  if (!name) return
  createCollection(name)
  newCollectionName.value = ''
}

// M13.7 — Trending
const { trending } = useTrending()

// M13.3 — Smart Playlists
const favoritesStore = useFavoritesStore()

const recentSongs = computed(() =>
  viewHistory.value.filter(h => h.type === 'song')
)

const mostPlayedSongs = computed(() =>
  trending.value.filter(t => t.type === 'song')
)

const hasAnySongs = computed(() =>
  recentSongs.value.length > 0 || mostPlayedSongs.value.length > 0 || favoritesStore.getFavoritesByType('song').length > 0
)

const songsByGenre = computed(() => {
  const songFavs = favoritesStore.getFavoritesByType('song')
  const counts = new Map<string, number>()
  for (const fav of songFavs) {
    const data = fav.data as { genres?: string[] } | undefined
    for (const g of data?.genres ?? []) {
      counts.set(g, (counts.get(g) ?? 0) + 1)
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([genre, count]) => ({ genre, count }))
})

const songsByDecade = computed(() => {
  const songFavs = favoritesStore.getFavoritesByType('song')
  const counts = new Map<number, number>()
  for (const fav of songFavs) {
    const data = fav.data as { year?: number } | undefined
    if (data?.year) {
      const decade = Math.floor(data.year / 10) * 10
      counts.set(decade, (counts.get(decade) ?? 0) + 1)
    }
  }
  return [...counts.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([decade, count]) => ({ decade, count }))
})

// Helpers
function typeIcon(type: string): string {
  const icons: Record<string, string> = {
    film: 'F', song: 'S', podcast: 'P', book: 'B', tv: 'T', place: 'L', article: 'A',
  }
  return icons[type] ?? '?'
}

function typeStyle(type: string): string {
  const colors: Record<string, string> = {
    film: 'bg-blue-500/20 text-blue-400',
    song: 'bg-green-500/20 text-green-400',
    podcast: 'bg-orange-500/20 text-orange-400',
    book: 'bg-yellow-500/20 text-yellow-400',
    tv: 'bg-indigo-500/20 text-indigo-400',
    place: 'bg-red-500/20 text-red-400',
    article: 'bg-cyan-500/20 text-cyan-400',
  }
  return colors[type] ?? 'bg-white/10 text-white/40'
}

function timeAgo(ts: number): string {
  const diff = Date.now() - ts
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'just now'
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}
</script>
