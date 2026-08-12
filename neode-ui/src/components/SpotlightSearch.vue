<template>
  <Teleport to="body">
    <Transition name="spotlight">
      <div
        v-if="spotlightStore.isOpen"
        class="fixed inset-0 z-[2500] flex items-center justify-center p-4"
        @click.self="spotlightStore.close()"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div
          ref="panelRef"
          class="glass-card w-full max-w-2xl relative z-10 overflow-hidden flex flex-col"
          :style="panelStyle"
          @mousedown="onPanelMouseDown"
        >
          <!-- Header: drag handle grip + search -->
          <div class="flex items-center gap-3 px-4 py-3 border-b border-white/10">
            <div
              ref="dragHandleRef"
              class="flex items-center justify-center w-8 h-8 rounded cursor-grab hover:bg-white/10 transition-colors shrink-0"
              :class="{ 'cursor-grabbing': isDragging }"
              title="Drag to move"
            >
              <svg class="w-4 h-4 text-white/50" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 6h2v2H8V6zm0 5h2v2H8v-2zm0 5h2v2H8v-2zm5-10h2v2h-2V6zm0 5h2v2h-2v-2zm0 5h2v2h-2v-2z" />
              </svg>
            </div>
            <div class="flex-1 flex items-center gap-3 min-w-0">
              <svg class="w-5 h-5 text-white/60 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
              <input
                ref="inputRef"
                v-model="query"
                type="text"
                placeholder="Search or type a command..."
                class="flex-1 bg-transparent text-white placeholder-white/50 outline-none text-base"
                @keydown="onInputKeydown"
              />
            </div>
            <kbd class="hidden sm:inline-flex px-2 py-1 text-xs text-white/50 bg-white/10 rounded">Esc</kbd>
          </div>

          <div class="flex-1 overflow-y-auto max-h-[60vh] min-h-[200px]">
            <!-- Recent items (when no query and we have recent) -->
            <div v-if="!query.trim() && spotlightStore.recentItems.length > 0" class="p-2 border-b border-white/10">
              <div class="px-3 py-2 text-xs font-medium text-white/50 uppercase tracking-wider">Recent</div>
              <button
                v-for="(item, idx) in spotlightStore.recentItems"
                :key="`recent-${item.id}-${item.timestamp}`"
                type="button"
                class="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors"
                :class="getItemClass(idx)"
                @click="selectRecent(item)"
              >
                <span class="text-white/90">{{ item.label }}</span>
                <span class="text-xs text-white/40">{{ item.type }}</span>
              </button>
            </div>

            <!-- Search results or help tree -->
            <template v-if="query.trim()">
              <div v-if="filteredItems.length > 0" class="p-2">
                <button
                  v-for="(item, idx) in filteredItems"
                  :key="item.id + item.section"
                  type="button"
                  class="w-full flex items-center justify-between gap-3 px-3 py-2.5 rounded-lg text-left transition-colors"
                  :class="getItemClass(idx)"
                  @click="selectItem(item)"
                >
                  <span class="text-white/90">{{ item.label }}</span>
                  <span class="text-xs text-white/40">{{ item.section }}</span>
                </button>
              </div>
              <div v-else class="px-8 pt-8 pb-2 text-center text-white/50">
                No results for "{{ query }}"
              </div>

              <!-- Hand the typed text to AIUI. Always offered while there is a
                   query — it is the whole point when nothing matched, and a
                   useful escape hatch when something did. -->
              <div class="p-2" :class="filteredItems.length > 0 ? 'border-t border-white/10' : ''">
                <button
                  type="button"
                  class="group w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors"
                  :class="getItemClass(askAiuiIndex)"
                  @click="askAiui()"
                >
                  <span
                    class="relative shrink-0 flex items-center justify-center w-9 h-9 rounded-lg overflow-hidden
                           bg-gradient-to-br from-blue-500/30 via-sky-400/15 to-transparent border border-blue-400/30"
                  >
                    <span class="absolute inset-0 transition-colors group-hover:bg-blue-400/10"></span>
                    <svg
                      class="relative w-[18px] h-[18px] text-blue-300"
                      fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24" aria-hidden="true"
                    >
                      <path stroke-linecap="round" stroke-linejoin="round" d="M20.5 11.5a8 8 0 01-11.9 6.97L4 19.5l1.06-4.3A8 8 0 1120.5 11.5z" />
                      <path stroke-linecap="round" stroke-linejoin="round" d="M12.2 8.4l.78 2.02 2.02.78-2.02.78-.78 2.02-.78-2.02-2.02-.78 2.02-.78z" />
                    </svg>
                  </span>
                  <span class="min-w-0 flex-1">
                    <span class="block text-white/90">Talk to AIUI about it</span>
                    <span class="block text-xs text-white/40 truncate">“{{ query.trim() }}”</span>
                  </span>
                  <kbd class="hidden sm:inline-flex px-2 py-1 text-xs text-white/50 bg-white/10 rounded shrink-0">↵</kbd>
                </button>
              </div>
            </template>
            <template v-else>
              <!-- Help tree when no search -->
              <div v-for="section in helpTree" :key="section.id" class="p-2">
                <div class="px-3 py-2 text-xs font-medium text-white/50 uppercase tracking-wider">{{ section.label }}</div>
                <button
                  v-for="(item, idx) in section.items"
                  :key="item.id"
                  type="button"
                  class="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors"
                  :class="getItemClass(recentOffset + getFlatIndex(section.id, idx))"
                  @click="selectHelpItem(section, item)"
                >
                  <span class="text-white/90">{{ item.label }}</span>
                </button>
              </div>
            </template>
          </div>
        </div>
      </div>
    </Transition>

  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import Fuse from 'fuse.js'
import { useSpotlightStore } from '@/stores/spotlight'
import { useCLIStore } from '@/stores/cli'
import { useAppStore } from '@/stores/app'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { helpTree, flattenForSearch, type SearchableItem } from '@/data/helpTree'

const router = useRouter()
const spotlightStore = useSpotlightStore()
const cliStore = useCLIStore()
const appStore = useAppStore()
const appLauncherStore = useAppLauncherStore()

const inputRef = ref<HTMLInputElement | null>(null)
const panelRef = ref<HTMLElement | null>(null)
const dragHandleRef = ref<HTMLElement | null>(null)

const query = ref('')
const isDragging = ref(false)
const dragStart = ref<{ x: number; y: number; panelX: number; panelY: number } | null>(null)

const staticItems = flattenForSearch()

// Build dynamic app items from installed packages
const dynamicAppItems = computed<SearchableItem[]>(() => {
  const pkgs = appStore.packages
  return Object.entries(pkgs).map(([id, pkg]) => ({
    id: `app-${id}`,
    label: pkg.manifest?.title || id,
    path: `__launch_app__:${id}`,
    type: 'action' as const,
    section: 'Installed Apps',
  }))
})

const allSearchableItems = computed(() => [...staticItems, ...dynamicAppItems.value])

const fuse = computed(() => new Fuse(allSearchableItems.value, {
  keys: ['label', 'section'],
  threshold: 0.4,
}))

const filteredItems = computed(() => {
  const q = query.value.trim()
  if (!q) return []
  const results = fuse.value.search(q)
  return results.map((r) => r.item)
})

const recentOffset = computed(() =>
  !query.value.trim() && spotlightStore.recentItems.length > 0 ? spotlightStore.recentItems.length : 0
)

// "Talk to AIUI about it" is appended after the matches, so it is the last
// selectable row whenever there is a query (including the zero-match case,
// where it is the only one).
const askAiuiIndex = computed(() => filteredItems.value.length)

const selectableCount = computed(() => {
  if (query.value.trim()) return filteredItems.value.length + 1
  return recentOffset.value + allSearchableItems.value.length
})

const panelStyle = computed(() => {
  const pos = savedPosition.value
  if (!pos) return {}
  return {
    transform: `translate(${pos.x}px, ${pos.y}px)`,
    margin: 0,
  }
})

const SAVED_POSITION_KEY = 'archipelago-spotlight-position'
const savedPosition = ref<{ x: number; y: number } | null>(null)

function loadSavedPosition() {
  try {
    const raw = localStorage.getItem(SAVED_POSITION_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      savedPosition.value = { x: parsed.x ?? 0, y: parsed.y ?? 0 }
    } else {
      savedPosition.value = null
    }
  } catch (e) {
    if (import.meta.env.DEV) console.warn('Failed to load saved spotlight position', e)
    savedPosition.value = null
  }
}

function savePosition(x: number, y: number) {
  savedPosition.value = { x, y }
  try {
    localStorage.setItem(SAVED_POSITION_KEY, JSON.stringify({ x, y }))
  } catch (e) {
    if (import.meta.env.DEV) console.warn('Failed to save spotlight position', e)
  }
}

function getFlatIndex(sectionId: string, itemIdx: number): number {
  let idx = 0
  for (const s of helpTree) {
    if (s.id === sectionId) return idx + itemIdx
    idx += s.items.length
  }
  return -1
}

function getItemClass(index: number) {
  const selected = spotlightStore.selectedIndex
  return index === selected
    ? 'bg-amber-500/20 text-amber-200'
    : 'hover:bg-white/10 text-white/90'
}

function launchInstalledApp(appId: string) {
  const pkg = appStore.packages[appId]
  if (!pkg) return
  let lanAddress = pkg.installed?.['interface-addresses']?.main?.['lan-address']
  if (lanAddress && lanAddress.includes('localhost')) {
    lanAddress = lanAddress.replace('localhost', window.location.hostname)
  }
  if (lanAddress) {
    appLauncherStore.open({ url: lanAddress, title: pkg.manifest?.title || appId })
  } else {
    router.push(`/dashboard/apps/${appId}`).catch(() => {})
  }
}

function selectItem(item: SearchableItem) {
  spotlightStore.addRecentItem({
    id: item.id,
    label: item.label,
    path: item.path,
    type: item.type,
  })
  spotlightStore.close()
  if (item.path?.startsWith('__launch_app__:')) {
    launchInstalledApp(item.path.replace('__launch_app__:', ''))
  } else if (item.path === '__cli__') {
    cliStore.open()
  } else if (item.path) {
    router.push(item.path)
  } else if (item.content) {
    spotlightStore.showHelpModal({ title: item.label, content: item.content, relatedPath: item.relatedPath })
  }
}

// Hand the raw typed text to AIUI instead of trying to match it to a screen.
// The nonce is what makes re-asking the identical question work: without a
// changing query the router treats the push as a no-op and Chat.vue never sees
// a new `ask` to forward.
function askAiui() {
  const text = query.value.trim()
  if (!text) return
  spotlightStore.close()
  router.push({ path: '/dashboard/chat', query: { ask: text, askedAt: String(Date.now()) } })
}

function selectHelpItem(section: { id: string }, item: { id: string; label: string; path?: string; content?: string; relatedPath?: string }) {
  const type = section.id === 'navigate' ? 'navigate' : section.id === 'learn' ? 'learn' : 'action'
  spotlightStore.addRecentItem({
    id: item.id,
    label: item.label,
    path: item.path,
    type,
  })
  spotlightStore.close()
  if (item.path?.startsWith('__launch_app__:')) {
    launchInstalledApp(item.path.replace('__launch_app__:', ''))
  } else if (item.path === '__cli__') {
    cliStore.open()
  } else if (item.path) {
    router.push(item.path)
  } else if (item.content) {
    spotlightStore.showHelpModal({ title: item.label, content: item.content, relatedPath: item.relatedPath })
  }
}

function selectRecent(item: { id: string; label: string; path?: string; type: 'navigate' | 'learn' | 'action' | 'goal' }) {
  spotlightStore.close()
  if (item.path?.startsWith('__launch_app__:')) {
    launchInstalledApp(item.path.replace('__launch_app__:', ''))
    return
  }
  if (item.path === '__cli__') {
    cliStore.open()
    return
  }
  if (item.path) {
    router.push(item.path)
    return
  }
  if (item.type === 'learn') {
    for (const s of helpTree) {
      const helpItem = s.items.find((i) => i.id === item.id)
      if (helpItem?.content) {
        spotlightStore.showHelpModal({ title: helpItem.label, content: helpItem.content, relatedPath: helpItem.relatedPath })
        return
      }
    }
  }
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    spotlightStore.close()
    e.preventDefault()
    e.stopPropagation()
    return
  }
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    spotlightStore.setSelectedIndex(
      Math.min(spotlightStore.selectedIndex + 1, Math.max(0, selectableCount.value - 1))
    )
    return
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault()
    spotlightStore.setSelectedIndex(Math.max(spotlightStore.selectedIndex - 1, 0))
    return
  }
  if (e.key === 'Enter') {
    e.preventDefault()
    const idx = spotlightStore.selectedIndex
    if (query.value.trim()) {
      if (idx === askAiuiIndex.value) { askAiui(); return }
      const item = filteredItems.value[idx]
      if (item) selectItem(item)
      return
    }
    if (idx < recentOffset.value) {
      const item = spotlightStore.recentItems[idx]
      if (item) selectRecent(item)
      return
    }
    const helpIdx = idx - recentOffset.value
    let count = 0
    for (const s of helpTree) {
      for (const item of s.items) {
        if (count === helpIdx) {
          selectHelpItem(s, item)
          return
        }
        count++
      }
    }
  }
}

function onPanelMouseDown(e: MouseEvent) {
  if (!dragHandleRef.value?.contains(e.target as Node)) return
  isDragging.value = true
  const rect = panelRef.value?.getBoundingClientRect()
  if (!rect) return
  const currentX = savedPosition.value?.x ?? 0
  const currentY = savedPosition.value?.y ?? 0
  dragStart.value = { x: e.clientX, y: e.clientY, panelX: currentX, panelY: currentY }
}

function onMouseMove(e: MouseEvent) {
  if (!dragStart.value) return
  const dx = e.clientX - dragStart.value.x
  const dy = e.clientY - dragStart.value.y
  savePosition(dragStart.value.panelX + dx, dragStart.value.panelY + dy)
}

function onMouseUp() {
  isDragging.value = false
  dragStart.value = null
}

watch(
  () => spotlightStore.isOpen,
  (open) => {
    if (open) {
      query.value = ''
      loadSavedPosition()
      nextTick(() => {
        inputRef.value?.focus()
        spotlightStore.setSelectedIndex(0)
      })
    }
  }
)

watch(
  [query, filteredItems],
  () => {
    spotlightStore.setSelectedIndex(0)
  }
)

onMounted(() => {
  loadSavedPosition()
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
})

onBeforeUnmount(() => {
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)
})
</script>

<style scoped>
.spotlight-enter-active,
.spotlight-leave-active {
  transition: opacity 0.2s ease;
}
.spotlight-enter-from,
.spotlight-leave-to {
  opacity: 0;
}
</style>
