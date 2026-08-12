<template>
  <div class="magazine-section-detail h-full flex flex-col overflow-hidden"
       :class="isDark ? 'bg-[#0a0a0a]' : 'bg-[#faf9f6]'"
       style="font-family: Georgia, 'Times New Roman', Times, serif;">

    <!-- Header with back + nav counter -->
    <div class="shrink-0 flex items-center justify-between px-4 py-3"
         :style="isDark
           ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
      <button
        class="absolute top-3 left-3 min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" :class="isDark ? 'text-white/70' : 'text-gray-600'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="flex-1 text-center pl-8">
        <span class="text-xs uppercase tracking-[0.3em] font-semibold"
              :class="isDark ? 'text-white/30' : 'text-black/30'">
          AI Brief
        </span>
      </div>
      <span class="text-xs font-mono tabular-nums shrink-0"
            :class="isDark ? 'text-white/25' : 'text-black/25'">
        {{ currentIndex + 1 }}/{{ totalSections }}
      </span>
    </div>

    <!-- Content area -->
    <div class="flex-1 min-h-0 overflow-y-auto custom-scrollbar flex flex-col">
      <div class="px-6 py-8 md:px-8 md:py-10 max-w-lg mx-auto my-auto">
        <!-- Group label -->
        <p v-if="section.group"
           class="text-xs uppercase tracking-[0.3em] font-semibold mb-4"
           :class="isDark ? 'text-white/25' : 'text-black/30'">
          {{ section.group }}
        </p>

        <!-- Title -->
        <h2 class="text-2xl md:text-3xl font-bold leading-tight mb-4"
            :class="isDark ? 'text-white/95' : 'text-black/90'">
          {{ section.title }}
        </h2>

        <!-- Author -->
        <p v-if="section.author"
           class="text-xs mb-6"
           :class="isDark ? 'text-white/40' : 'text-black/40'">
          By {{ section.author }}
        </p>

        <!-- Decorative rule -->
        <div class="w-12 h-px mb-6"
             :class="isDark ? 'bg-white/15' : 'bg-black/15'" />

        <!-- Content as quote-style paragraphs -->
        <div class="space-y-4">
          <p v-for="(paragraph, i) in paragraphs" :key="i"
             class="text-base md:text-lg leading-relaxed"
             :class="isDark ? 'text-white/75' : 'text-black/65'">
            {{ paragraph }}
          </p>
        </div>

        <!-- Source link -->
        <a v-if="section.url"
           :href="section.url"
           target="_blank"
           rel="noopener noreferrer"
           class="inline-flex items-center gap-2 mt-6 min-h-[44px] text-xs transition-colors"
           :class="isDark ? 'text-white/40 hover:text-white/70' : 'text-black/40 hover:text-black/70'">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
          </svg>
          Source
        </a>
      </div>
    </div>

    <!-- Navigation footer -->
    <div class="shrink-0 flex items-center justify-between px-4 py-3"
         :style="isDark
           ? 'border-top: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-top: 1px solid rgba(0, 0, 0, 0.06)'">
      <button
        class="flex items-center gap-1.5 px-3 min-h-[44px] rounded-lg text-xs transition-colors"
        :class="isDark
          ? 'text-white/50 hover:text-white/80 hover:bg-white/5'
          : 'text-black/40 hover:text-black/70 hover:bg-black/5'"
        @click="$emit('navigate', 'prev')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
        Prev
      </button>

      <!-- Dot indicators -->
      <div class="flex items-center gap-1">
        <div v-for="n in totalSections" :key="n"
             class="w-1.5 h-1.5 rounded-full transition-all duration-200"
             :class="n - 1 === currentIndex
               ? isDark ? 'bg-white/70 scale-125' : 'bg-black/60 scale-125'
               : isDark ? 'bg-white/15' : 'bg-black/15'" />
      </div>

      <button
        class="flex items-center gap-1.5 px-3 min-h-[44px] rounded-lg text-xs transition-colors"
        :class="isDark
          ? 'text-white/50 hover:text-white/80 hover:bg-white/5'
          : 'text-black/40 hover:text-black/70 hover:bg-black/5'"
        @click="$emit('navigate', 'next')"
      >
        Next
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { MagazineSection } from '@/composables/useContentPanel'
import { useTheme } from '@/composables/useTheme'

const props = defineProps<{
  section: MagazineSection
  currentIndex: number
  totalSections: number
}>()

defineEmits<{
  back: []
  navigate: [direction: 'prev' | 'next']
}>()

const { isDark } = useTheme()

const paragraphs = computed(() => {
  const text = props.section.content
  return text
    .replace(/\[([^\]]*)\]\([^)]+\)/g, '$1')     // [text](url) → text
    .replace(/https?:\/\/\S+/g, '')                // bare URLs
    .replace(/\uFE0F/g, '')                         // variation selectors
    .replace(/\*\*/g, '')                           // bold markers
    .replace(/\*([^*\n]+)\*/g, '$1')               // *italic* → italic
    .replace(/(?:^|(?<=\s))[\p{Emoji_Presentation}\p{Extended_Pictographic}]+\s*/gu, '') // standalone emojis
    .replace(/---+/g, '')                           // horizontal rules
    .replace(/^#+\s*/gm, '')                        // heading markers
    .replace(/\|/g, ', ')                           // pipes → comma-space
    .replace(/,\s*,+/g, ',')                        // collapse multiple commas
    .split(/\n{2,}|\n\s*[-•]\s+/)
    .map(p => p.replace(/^\s*[-•]\s+/, '').replace(/(^|\n)\s*,\s*/g, '$1').trim())
    .filter(p => p.length > 0)
})
</script>
