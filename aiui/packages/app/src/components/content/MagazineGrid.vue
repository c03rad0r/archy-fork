<template>
  <div class="magazine h-full flex flex-col"
       :class="isDark ? 'magazine-dark' : 'magazine-light'">
    <!-- Masthead -->
    <header class="shrink-0 px-5 py-4 flex items-center justify-between border-b"
            :class="isDark ? 'border-white/10' : 'border-black/10'">
      <h1 class="font-serif text-xl font-bold tracking-tight"
          :class="isDark ? 'text-white' : 'text-black'">
        AI Brief
      </h1>
      <div class="shrink-0">
        <slot name="header-actions" />
      </div>
    </header>

    <div class="flex-1 overflow-y-auto custom-scrollbar">
      <!-- Query context hero -->
      <div v-if="headlineText" class="relative overflow-hidden"
           :style="{ minHeight: '180px' }">
        <!-- Background image or gradient -->
        <div class="absolute inset-0">
          <img v-if="heroImageUrl"
               :src="heroImageUrl"
               alt=""
               class="w-full h-full object-cover"
               style="filter: saturate(0.3) contrast(1.1);" />
          <div v-else class="w-full h-full"
               :class="isDark
                 ? 'bg-gradient-to-br from-white/[0.04] via-white/[0.02] to-transparent'
                 : 'bg-gradient-to-br from-black/[0.06] via-black/[0.03] to-transparent'" />
        </div>
        <!-- Dark overlay -->
        <div class="absolute inset-0"
             :class="isDark
               ? 'bg-gradient-to-t from-[#0a0a0a] via-[#0a0a0a]/80 to-[#0a0a0a]/60'
               : 'bg-gradient-to-t from-[#faf9f6] via-[#faf9f6]/85 to-[#faf9f6]/65'" />
        <!-- Content -->
        <div class="relative z-10 flex flex-col justify-end h-full px-5 pb-5 pt-12"
             style="min-height: 180px;">
          <p class="text-xs uppercase tracking-[0.3em] font-medium mb-2"
             :class="isDark ? 'text-white/40' : 'text-black/40'">
            In response to
          </p>
          <p class="font-serif text-2xl italic leading-tight"
             :class="isDark ? 'text-white/70' : 'text-black/60'">
            {{ headlineText }}
          </p>
        </div>
      </div>

      <!-- Tile grid -->
      <div class="px-3 pt-2 pb-8">
        <div class="grid grid-cols-2 gap-px"
             :class="isDark ? 'bg-white/12' : 'bg-black/10'">
          <template v-for="(tile, i) in tiles" :key="i">
            <!-- Banner tile: full width with icon -->
            <div v-if="tile.type === 'banner'"
                 class="col-span-2 flex flex-col items-center justify-center py-8 px-5"
                 :class="isDark ? 'bg-[#0a0a0a]' : 'bg-[#faf9f6]'">
              <svg class="w-5 h-5 mb-2.5" :class="isDark ? 'text-white/20' : 'text-black/15'"
                   viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path v-if="tile.icon === 'compass'" stroke-linecap="round" stroke-linejoin="round"
                      d="M12 2a10 10 0 100 20 10 10 0 000-20zm0 0v2m0 16v2m10-10h-2M4 12H2m15.07-5.07l-1.41 1.41M8.34 15.66l-1.41 1.41m0-11.14l1.41 1.41m7.32 7.32l1.41 1.41" />
                <path v-else-if="tile.icon === 'bookmark'" stroke-linecap="round" stroke-linejoin="round"
                      d="M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z" />
                <path v-else-if="tile.icon === 'lightning'" stroke-linecap="round" stroke-linejoin="round"
                      d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
                <path v-else stroke-linecap="round" stroke-linejoin="round"
                      d="M4 6h16M4 12h16M4 18h7" />
              </svg>
              <p class="text-xs uppercase tracking-[0.3em] font-semibold text-center"
                 :class="isDark ? 'text-white/30' : 'text-black/30'">
                {{ tile.label }}
              </p>
            </div>

            <!-- Wide tile: full width, for lead/summary -->
            <button v-else-if="tile.type === 'wide'"
                 class="col-span-2 text-left px-5 py-5 transition-colors cursor-pointer"
                 :class="isDark
                   ? 'bg-[#0a0a0a] hover:bg-white/[0.03]'
                   : 'bg-[#faf9f6] hover:bg-black/[0.02]'"
                 @click="tile.section && openTile(tile.section)">
              <p v-if="tile.label"
                 class="text-xs uppercase tracking-[0.3em] font-semibold mb-2"
                 :class="isDark ? 'text-white/30' : 'text-black/35'">
                {{ tile.label }}
              </p>
              <h2 class="font-serif text-lg font-bold leading-snug mb-2"
                  :class="isDark ? 'text-white/95' : 'text-black/90'">
                {{ tile.title }}
              </h2>
              <p v-if="tile.author"
                 class="text-xs mb-2"
                 :class="isDark ? 'text-white/40' : 'text-black/40'">
                By {{ tile.author }}
              </p>
              <p class="font-serif text-sm leading-relaxed"
                 :class="isDark ? 'text-white/70' : 'text-black/60'">
                {{ tile.text }}
              </p>
            </button>

            <!-- Standard tile: half width -->
            <button v-else
                 class="text-left px-4 py-4 transition-colors flex flex-col cursor-pointer"
                 :class="[
                   isDark
                     ? 'bg-[#0a0a0a] hover:bg-white/[0.03]'
                     : 'bg-[#faf9f6] hover:bg-black/[0.02]',
                   tile.type === 'dark'
                     ? isDark ? 'bg-white/[0.04]' : 'bg-black/[0.04]'
                     : ''
                 ]"
                 @click="tile.section && openTile(tile.section)">
              <p v-if="tile.label"
                 class="text-xs uppercase tracking-[0.25em] font-semibold mb-1.5"
                 :class="isDark ? 'text-white/25' : 'text-black/30'">
                {{ tile.label }}
              </p>
              <h3 v-if="tile.title"
                  class="font-serif text-sm font-bold leading-snug mb-1"
                  :class="isDark ? 'text-white/90' : 'text-black/85'">
                {{ tile.title }}
              </h3>
              <p class="font-serif text-xs leading-relaxed flex-1"
                 :class="[
                   isDark ? 'text-white/55' : 'text-black/50',
                   !tile.title ? 'italic' : ''
                 ]">
                {{ tile.text }}
              </p>
            </button>
          </template>
        </div>
      </div>

      <!-- Empty state -->
      <div v-if="sections.length === 0" class="flex items-center justify-center py-16 px-4">
        <p class="text-sm" :class="isDark ? 'text-white/40' : 'text-gray-400'">
          No sections to display
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useContentPanel, type MagazineSection } from '@/composables/useContentPanel'

interface Tile {
  type: 'wide' | 'half' | 'dark' | 'banner'
  title: string
  text: string
  label?: string
  author?: string
  icon?: string
  section?: MagazineSection
}

const props = withDefaults(defineProps<{
  sections: MagazineSection[]
  heroImageUrl?: string | null
  title?: string
  query?: string
}>(), {
  heroImageUrl: null,
  title: 'Brief',
  query: '',
})

const { isDark } = useTheme()
const { openWebsiteDetail, openMagazineSectionDetail } = useContentPanel()

const bannerIcons = ['compass', 'bookmark', 'lightning', 'lines'] as const
const bannerLabels = ['Perspectives', 'Worth Noting', 'Key Signals', 'Analysis']

function cleanText(text: string): string {
  return text
    .replace(/\[([^\]]*)\]\([^)]+\)/g, '$1')     // [text](url) → text
    .replace(/https?:\/\/\S+/g, '')                // bare URLs
    .replace(/\uFE0F/g, '')                         // variation selectors
    .replace(/(?:^|(?<=\s))[\p{Emoji_Presentation}\p{Extended_Pictographic}]+\s*/gu, '') // standalone emojis
    .replace(/---+/g, '')                           // horizontal rules
    .replace(/^#+\s*/gm, '')
    .replace(/\*\*/g, '')
    .replace(/\*([^*\n]+)\*/g, '$1')               // *italic* → italic
    .replace(/\|/g, ', ')                           // pipes → comma-space
    .replace(/,\s*,+/g, ',')                        // collapse multiple commas
    .replace(/^\s*[-•]\s+/gm, '')                  // bullets at line start only
    .replace(/\n+/g, ' ')
    .replace(/(^|\s),\s*/g, '$1')                  // trim stray leading commas
    .trim()
}

function truncate(text: string, max: number): string {
  const clean = cleanText(text)
  if (clean.length <= max) return clean
  return clean.slice(0, max).replace(/\s+\S*$/, '') + '\u2009...'
}

/** Break a section's content into individual points (split on bullets/newlines) */
function splitIntoBullets(content: string): string[] {
  return content
    .split(/\n\s*[-•]\s*|\n{2,}/)
    .map(s => s.replace(/^[-•]\s*/, '').replace(/\*\*/g, '').replace(/[\p{Emoji_Presentation}\p{Extended_Pictographic}]\s*/gu, '').trim())
    .filter(s => s.length > 10)
}

const tiles = computed<Tile[]>(() => {
  const result: Tile[] = []
  const secs = props.sections
  if (!secs.length) return result

  // Seeded pseudo-random based on query for consistent layout
  let seed = 0
  for (const c of (props.query || 'brief')) seed = ((seed << 5) - seed + c.charCodeAt(0)) | 0
  const rand = () => { seed = (seed * 16807 + 0) % 2147483647; return (seed & 0x7fffffff) / 2147483647 }

  let lastGroup = ''
  let bannerIdx = 0
  let pairToggle = false // track half-tile pairing

  secs.forEach((section, i) => {
    if (i === 0 && !section.group) {
      // Lead section: always wide
      result.push({
        type: 'wide',
        title: section.title,
        text: truncate(section.content, 200),
        label: 'The Lead',
        author: section.author,
        section,
      })
      return
    }

    // Insert a banner when entering a new heading group
    const group = section.group || ''
    if (group && group !== lastGroup) {
      // Pad any unpaired half tile before the banner
      if (pairToggle) {
        result.push({ type: 'dark', title: '', text: '' })
        pairToggle = false
      }
      result.push({
        type: 'banner',
        title: '',
        text: '',
        icon: bannerIcons[bannerIdx % bannerIcons.length],
        label: group,
      })
      bannerIdx++
      lastGroup = group
    }

    // Sections within a group get alternating half/dark tiles
    if (group) {
      const variant = pairToggle ? 'dark' : 'half'
      // If title is basically the same as content start, skip the title and just show content
      const contentClean = cleanText(section.content)
      const titleClean = cleanText(section.title)
      const titleIsContent = contentClean.toLowerCase().startsWith(titleClean.toLowerCase().slice(0, 30))
      result.push({
        type: variant,
        title: titleIsContent ? '' : section.title,
        text: truncate(section.content, titleIsContent ? 160 : 100),
        section,
      })
      pairToggle = !pairToggle
    } else {
      // Non-grouped sections: use wide layout
      // Pad any unpaired half tile
      if (pairToggle) {
        result.push({ type: 'dark', title: '', text: '' })
        pairToggle = false
      }
      result.push({
        type: 'wide',
        title: section.title,
        text: truncate(section.content, 180),
        author: section.author,
        section,
      })
    }
  })

  // Pad final unpaired half tile
  if (pairToggle) {
    result.push({ type: 'dark', title: '', text: '' })
  }

  return result
})

/** Extract bold title from a bullet point like "**Title** - rest" */
function extractBulletTitle(text: string): string {
  const m = /^\*\*([^*]+)\*\*/.exec(text)
  return m ? m[1].trim() : ''
}

function cleanBulletTitle(text: string): string {
  return text.replace(/^\*\*[^*]+\*\*\s*[-–—:]\s*/, '').trim()
}

function openTile(section: MagazineSection) {
  const idx = props.sections.indexOf(section)
  openMagazineSectionDetail(section, idx >= 0 ? idx : 0)
}

const headlineText = computed(() => {
  const q = (props.query ?? '').trim()
  if (!q) return props.title
  return q.length > 100 ? q.slice(0, 97) + '...' : q
})
</script>

<style scoped>
.magazine {
  font-family: Georgia, 'Times New Roman', Times, serif;
}
.magazine-light {
  background-color: #faf9f6;
}
.magazine-dark {
  background-color: #0a0a0a;
}
</style>
