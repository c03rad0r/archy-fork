<template>
  <div class="flex flex-col h-full">
    <div class="shrink-0 px-4 py-3 flex items-center justify-between gap-2"
         :style="isDark
           ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
      <span class="text-sm font-semibold"
            :class="isDark ? 'text-white/90' : 'text-gray-900'">
        Design System
      </span>
      <p class="text-xs"
         :class="isDark ? 'text-white/30' : 'text-gray-400'">
        {{ filteredItems.length }} items
      </p>
    </div>

    <!-- Category filter -->
    <div class="shrink-0 px-4 py-2 flex gap-1.5 overflow-x-auto scrollbar-hide">
      <button
        v-for="cat in categories"
        :key="cat.id"
        class="text-xs px-2.5 py-1 rounded-md font-medium whitespace-nowrap transition-colors"
        :class="activeCategory === cat.id
          ? 'bg-accent/20 text-accent'
          : isDark
            ? 'bg-white/5 text-white/50 hover:bg-white/10'
            : 'bg-black/5 text-gray-500 hover:bg-black/10'"
        @click="activeCategory = cat.id"
      >
        {{ cat.label }}
      </button>
    </div>

    <!-- Items grid -->
    <div class="flex-1 overflow-y-auto px-4 py-3">
      <div class="grid grid-cols-2 gap-2">
        <button
          v-for="item in filteredItems"
          :key="item.id"
          class="text-left p-3 rounded-xl transition-all duration-150 group relative"
          :class="[
            codeMode && isDesignTokenSelected(item.id)
              ? 'ring-2 ring-accent/50 bg-accent/10 cursor-pointer'
              : isDark
                ? 'bg-white/[0.03] hover:bg-white/[0.07] cursor-pointer'
                : 'bg-black/[0.02] hover:bg-black/[0.05] cursor-pointer',
          ]"
          @click="selectItem(item)"
        >
          <!-- Selection toggle (top-right) — only this area toggles context selection -->
          <div
            v-if="codeMode"
            class="absolute top-2 right-2 min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center z-10 cursor-pointer transition-colors"
            :class="isDesignTokenSelected(item.id)
              ? 'bg-accent'
              : isDark ? 'bg-white/10 hover:bg-white/20' : 'bg-black/10 hover:bg-black/20'"
            @click.stop="toggleDesignToken(item.id)"
          >
            <svg v-if="isDesignTokenSelected(item.id)" class="w-3 h-3 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
            </svg>
          </div>
          <!-- Preview swatch for colors -->
          <div v-if="item.category === 'colors' && item.preview === 'inline'"
               class="h-8 rounded-md mb-2 border"
               :class="isDark ? 'border-white/10' : 'border-black/10'"
               :style="{ background: extractColorValue(item.code) }" />

          <!-- Preview for spacing -->
          <div v-else-if="item.category === 'spacing' && item.preview === 'inline'"
               class="h-8 flex items-end gap-0.5 mb-2">
            <div class="bg-accent/40 rounded-sm" style="width: 4px; height: 30%" />
            <div class="bg-accent/40 rounded-sm" style="width: 4px; height: 50%" />
            <div class="bg-accent/40 rounded-sm" style="width: 4px; height: 70%" />
            <div class="bg-accent/40 rounded-sm" style="width: 4px; height: 100%" />
          </div>

          <!-- Generic icon for components -->
          <div v-else class="h-8 flex items-center mb-2">
            <svg class="w-5 h-5 transition-colors"
                 :class="isDark ? 'text-white/20 group-hover:text-white/40' : 'text-black/15 group-hover:text-black/30'"
                 fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path v-if="item.category === 'atoms'" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                    d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
              <path v-else-if="item.category === 'molecules'" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                    d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
              <path v-else-if="item.category === 'organisms'" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                    d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
              <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                    d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01" />
            </svg>
          </div>

          <h3 class="text-xs font-semibold leading-tight mb-0.5"
              :class="isDark ? 'text-white/80' : 'text-gray-800'">
            {{ item.name }}
          </h3>
          <p class="text-xs leading-snug line-clamp-2"
             :class="isDark ? 'text-white/40' : 'text-gray-400'">
            {{ item.description }}
          </p>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useContentPanel, type DesignSystemItem } from '@/composables/useContentPanel'
import { useCodeContext } from '@/composables/useCodeContext'

const { isDark } = useTheme()
const { openDesignSystemItem } = useContentPanel()
const { codeMode, toggleDesignToken, isDesignTokenSelected } = useCodeContext()

const activeCategory = ref<string>('all')

const categories = [
  { id: 'all', label: 'All' },
  { id: 'colors', label: 'Colors' },
  { id: 'typography', label: 'Typography' },
  { id: 'spacing', label: 'Spacing' },
  { id: 'atoms', label: 'Atoms' },
  { id: 'molecules', label: 'Molecules' },
  { id: 'organisms', label: 'Organisms' },
]

const items: DesignSystemItem[] = [
  // Colors
  { id: 'color-bg', name: 'Background', category: 'colors', preview: 'inline', description: 'Primary app background', code: 'background-color: #0a0a0a;\n/* Tailwind: bg-[#0a0a0a] */', usedIn: 'ChatPage, all panels, base layout' },
  { id: 'color-accent', name: 'Accent / Bitcoin', category: 'colors', preview: 'inline', description: 'Primary action color, Bitcoin orange', code: 'color: #F7931A;\n/* Tailwind: text-accent */', usedIn: 'Gradient buttons, active tabs, zap counts, CTA elements' },
  { id: 'color-primary', name: 'Primary', category: 'colors', preview: 'inline', description: 'Primary neutral tone', code: 'color: #606060;\n/* Tailwind: text-primary */', usedIn: 'Secondary text, borders, muted elements' },
  { id: 'color-surface', name: 'Glass Surface', category: 'colors', preview: 'inline', description: 'Glass morphism panel background', code: 'background: rgba(0, 0, 0, 0.35);\nbackdrop-filter: blur(18px);\nborder: 1px solid rgba(255, 255, 255, 0.18);\n/* Tailwind: .glass */', usedIn: 'ChatInput, ContentPanel, all overlay panels' },
  { id: 'color-text-scale', name: 'Text Opacity Scale', category: 'colors', preview: 'inline', description: '/25 placeholder, /40 muted, /60 secondary, /80 body, /90 emphasis', code: '/* Text opacity scale */\n.placeholder { color: rgba(255,255,255, 0.25); }\n.muted      { color: rgba(255,255,255, 0.40); }\n.secondary  { color: rgba(255,255,255, 0.60); }\n.body       { color: rgba(255,255,255, 0.80); }\n.emphasis   { color: rgba(255,255,255, 0.90); }\n.heading    { color: rgba(255,255,255, 0.96); }', usedIn: 'Every component — consistent hierarchy across the system' },

  // Typography
  { id: 'type-body', name: 'Body Font', category: 'typography', description: 'Inter / system-ui for all body text', code: 'font-family: Inter, system-ui, -apple-system, sans-serif;\n/* Applied globally */', usedIn: 'Global default — ChatMessage, grids, detail views' },
  { id: 'type-mono', name: 'Monospace Font', category: 'typography', description: 'Menlo / Monaco for code and IDs', code: 'font-family: Menlo, Monaco, "Courier New", monospace;\n/* Tailwind: font-mono */', usedIn: 'CodeDetail, conversation IDs, relay URLs, metadata' },
  { id: 'type-serif', name: 'Serif Font', category: 'typography', description: 'Georgia for magazine/editorial layouts', code: 'font-family: Georgia, "Times New Roman", Times, serif;\n/* Used in MagazineGrid, AI Brief */', usedIn: 'MagazineGrid, MagazineSectionDetail, AI Brief' },
  { id: 'type-sizes', name: 'Text Sizes', category: 'typography', description: 'Compact scale: 10px labels to 2xl headings', code: '/* Key sizes used */\ntext-xs  /* labels, metadata */\ntext-xs      /* 12px - secondary text */\ntext-sm      /* 14px - body text */\ntext-base    /* 16px - primary text */\ntext-lg      /* 18px - section headings */\ntext-xl      /* 20px - page headings */\ntext-2xl     /* 24px - hero text */', usedIn: 'Globally — see specific usage in each size bracket' },

  // Spacing
  { id: 'space-grid', name: '4px Grid', category: 'spacing', preview: 'inline', description: 'All spacing follows a 4px base grid', code: '/* 4px grid system */\n1  = 4px    /* micro gap */\n2  = 8px    /* tight gap */\n3  = 12px   /* small padding */\n4  = 16px   /* standard padding */\n5  = 20px   /* section padding */\n6  = 24px   /* large gap */\n8  = 32px   /* section spacing */\n12 = 48px   /* large sections */', usedIn: 'Every layout — padding, margins, gaps between elements' },
  { id: 'space-radius', name: 'Border Radius', category: 'spacing', preview: 'inline', description: 'Rounded corners from subtle to full', code: '/* Border radius scale */\nrounded-md    /* 6px - badges, tags */\nrounded-lg    /* 8px - buttons, inputs */\nrounded-xl    /* 12px - cards, panels */\nrounded-2xl   /* 16px - large panels */\nrounded-full  /* pill buttons */', usedIn: 'Badges (md), buttons (lg), cards (xl), panels (2xl)' },

  // Atoms
  { id: 'atom-glass-btn', name: 'Glass Button', category: 'atoms', description: '48px height, glass morphism background', code: '<button class="glass-button">\n  Action\n</button>\n\n/* glass-button:\n   height: 48px\n   background: rgba(0,0,0,0.6)\n   backdrop-filter: blur(18px)\n   border-radius: 12px\n   border: 1px solid rgba(255,255,255,0.12)\n*/', usedIn: 'ChatInput send, modal actions, primary controls' },
  { id: 'atom-glass-btn-sm', name: 'Glass Button Small', category: 'atoms', description: 'Compact glass button variant', code: '<button class="glass-button-sm">\n  Small\n</button>\n\n/* Compact variant of glass-button */', usedIn: 'ChatInput send/stop buttons, inline actions' },
{ id: 'atom-icon-btn', name: 'Icon Button', category: 'atoms', description: 'Path glass icon, 32-36px square', code: '<button class="w-9 h-9 rounded-xl path-glass-icon\n  flex items-center justify-center">\n  <svg class="w-4 h-4" ...>\n</button>\n\n/* path-glass-icon:\n   background: transparent\n   transition: colors\n   hover: bg-white/10\n*/', usedIn: 'ChatHeader toolbar, detail back buttons, close buttons' },
  { id: 'atom-badge', name: 'Genre Badge', category: 'atoms', description: 'Tiny pill badge for tags/genres', code: '<span class="text-xs px-2 py-1 rounded-md\n  font-medium bg-white/10 text-white/60">\n  Science Fiction\n</span>', usedIn: 'FilmGrid, SongGrid, BookGrid, TVSeriesGrid genre filters' },
  { id: 'atom-nav-tab', name: 'Nav Tab', category: 'atoms', description: 'Content panel tab with active state', code: '<button class="nav-tab-active">\n  Films\n</button>\n\n/* Active: accent underline\n   Inactive: text-white/50 hover:text-white\n   Transition: 200ms */', usedIn: 'ContentPanel tab bar, mobile content tab filters' },
  { id: 'atom-input', name: 'Text Input', category: 'atoms', description: 'Search/filter input field', code: '<input\n  class="w-full px-3 py-2 rounded-lg text-xs\n    outline-none transition-colors\n    bg-white/5 text-white/80\n    placeholder:text-white/25\n    focus:bg-white/10"\n  placeholder="Search..."\n/>', usedIn: 'All grid search bars, ProjectGrid new project' },
  { id: 'atom-scrollbar', name: 'Custom Scrollbar', category: 'atoms', description: 'Thin translucent scrollbar for scroll areas', code: '.custom-scrollbar::-webkit-scrollbar {\n  width: 4px;\n}\n.custom-scrollbar::-webkit-scrollbar-thumb {\n  background: rgba(255,255,255, 0.1);\n  border-radius: 2px;\n}\n/* Also: .scrollbar-hide hides completely */', usedIn: 'Content grids, chat message list, file trees' },

  // Molecules
  { id: 'mol-glass-card', name: 'Glass Card', category: 'molecules', description: 'Frosted glass card with border', code: '<div class="glass-card">\n  <h3>Title</h3>\n  <p>Content</p>\n</div>\n\n/* glass-card:\n   background: rgba(0,0,0,0.65)\n   backdrop-filter: blur(18px)\n   border: 1px solid rgba(255,255,255,0.12)\n   border-radius: 16px\n   padding: 16px\n*/', usedIn: 'ChatWindow container, content panel wrapper' },
  { id: 'mol-gradient-card', name: 'Gradient Card', category: 'molecules', description: 'Card with gradient background', code: '<div class="gradient-card">\n  <h3>Featured</h3>\n  <p>Content</p>\n</div>\n\n/* gradient-card:\n   background: linear-gradient(135deg, ...)\n   border-radius: 16px\n*/', usedIn: 'Featured content highlights, promotional sections' },
  { id: 'mol-source-link', name: 'Source Link Row', category: 'molecules', description: 'Icon + label + external link arrow', code: '<a class="flex items-center justify-between\n  p-3 rounded-xl bg-white/5\n  hover:bg-white/10 transition-colors">\n  <div class="flex items-center gap-2.5">\n    <span class="text-sm">icon</span>\n    <div>\n      <p class="text-xs font-medium\n        text-white/80">Name</p>\n      <p class="text-xs\n        text-white/30">Description</p>\n    </div>\n  </div>\n  <svg><!-- external link icon --></svg>\n</a>', usedIn: 'FilmDetail, SongDetail, PodcastDetail sources' },
  { id: 'mol-banner-hero', name: 'Banner Hero', category: 'molecules', description: 'Aspect 16/7 image with gradient overlay', code: '<div class="relative w-full aspect-[16/7]\n  overflow-hidden">\n  <img :src="url" class="absolute inset-0\n    w-full h-full object-cover" />\n  <div class="absolute inset-0\n    bg-gradient-to-t from-black/80\n    via-black/30 to-transparent" />\n  <div class="absolute bottom-0 p-4">\n    <h2 class="text-lg font-bold\n      text-white">Title</h2>\n  </div>\n</div>', usedIn: 'FilmDetail, TVSeriesDetail, BookDetail banners' },
  { id: 'mol-cover-card', name: 'Cover Card', category: 'molecules', description: 'Poster/cover image card with overlay text', code: '<button class="group rounded-2xl overflow-hidden">\n  <div class="aspect-[2/3] relative">\n    <img class="w-full h-full object-cover\n      group-hover:scale-110\n      transition-transform duration-300" />\n    <div class="absolute inset-0\n      bg-gradient-to-t from-black/60\n      to-transparent" />\n    <div class="absolute bottom-0 p-2">\n      <p class="text-xs text-white/90">\n        Title</p>\n    </div>\n  </div>\n</button>', usedIn: 'FilmGrid, TVSeriesGrid, SongGrid, BookGrid cards' },

  // Organisms
  { id: 'org-chat-bubble', name: 'Chat Bubble', category: 'organisms', description: 'AI/User message bubble with streaming', code: '<!-- User bubble -->\n<div class="flex justify-end">\n  <div class="glass-card max-w-[85%]\n    px-4 py-3 text-sm text-white/90">\n    Message text\n  </div>\n</div>\n\n<!-- AI bubble -->\n<div class="flex justify-start">\n  <div class="max-w-[85%] px-4 py-3\n    text-sm text-white/80">\n    Response with markdown\n  </div>\n</div>', usedIn: 'ChatMessage.vue — the primary chat interface' },
  { id: 'org-content-panel', name: 'Content Panel', category: 'organisms', description: 'Tabs + grid + detail navigation', code: '<!-- Structure -->\n<div class="flex flex-col h-full">\n  <!-- Tab bar -->\n  <div class="flex gap-1 px-3 py-2">\n    <button class="nav-tab">Tab</button>\n  </div>\n  <!-- Grid view -->\n  <ContentGridView />\n  <!-- or Detail view -->\n  <DetailView />\n</div>', usedIn: 'ChatPage middle column, mobile Content tab' },
  { id: 'org-detail-view', name: 'Detail View', category: 'organisms', description: 'Full detail with banner, back button, metadata', code: '<!-- Pattern: Banner → Meta → Content -->\n<div class="h-full overflow-y-auto">\n  <!-- Banner with back button -->\n  <div class="relative aspect-[16/7]">\n    <img class="object-cover" />\n    <div class="gradient-overlay" />\n    <button class="absolute top-3 left-3\n      path-glass-icon">Back</button>\n    <div class="absolute bottom-0 p-4">\n      <h2>Title</h2>\n      <div>Metadata</div>\n    </div>\n  </div>\n  <!-- Body -->\n  <div class="p-4 space-y-4">\n    <p>Description</p>\n    <div>Genre badges</div>\n    <div>Source links</div>\n  </div>\n</div>', usedIn: 'FilmDetail, BookDetail, TVSeriesDetail, SongDetail, PodcastDetail' },
  { id: 'org-magazine', name: 'Magazine Grid', category: 'organisms', description: 'Editorial tile layout with hero, wide, and half tiles', code: '<!-- Magazine structure -->\n<div class="grid grid-cols-2 gap-px\n  bg-white/12">\n  <!-- Wide tile (col-span-2) -->\n  <button class="col-span-2 px-5 py-5\n    bg-[#0a0a0a]">\n    <p class="text-xs uppercase\n      tracking-[0.3em]">Label</p>\n    <h2 class="font-serif text-lg\n      font-bold">Title</h2>\n    <p class="font-serif text-sm">Text</p>\n  </button>\n  <!-- Half tiles -->\n  <button class="px-4 py-4 bg-[#0a0a0a]">\n    <h3 class="font-serif text-sm\n      font-bold">Title</h3>\n    <p class="font-serif text-xs">Text</p>\n  </button>\n</div>', usedIn: 'MagazineGrid.vue — AI Brief editorial view' },
  { id: 'org-nostr-note', name: 'Nostr Note', category: 'organisms', description: 'Note card with avatar, author, content, zaps', code: '<div class="p-3 rounded-xl bg-white/[0.03]\n  border border-white/5">\n  <div class="flex items-start gap-2.5">\n    <div class="w-8 h-8 rounded-full\n      bg-purple-500/20 text-purple-400">\n      F\n    </div>\n    <div class="flex-1">\n      <span class="text-xs font-semibold">\n        author</span>\n      <p class="text-xs text-white/60">\n        Note content...</p>\n      <span class="text-xs\n        text-amber-500/70">21000 sats</span>\n    </div>\n  </div>\n</div>', usedIn: 'NostrGrid.vue — Nostr feed tab' },

  // Animations
  { id: 'anim-fade-up', name: 'Fade Up', category: 'atoms', description: 'Entry animation: translate + opacity', code: '.animate-fade-up {\n  animation: fadeUp 900ms ease-out;\n}\n@keyframes fadeUp {\n  from {\n    opacity: 0;\n    transform: translateY(16px);\n  }\n  to {\n    opacity: 1;\n    transform: translateY(0);\n  }\n}\n/* Also: animate-fade-up-fast (400ms) */', usedIn: 'Empty states, initial load elements, ChatWindow' },
  { id: 'anim-scale-in', name: 'Scale In', category: 'atoms', description: 'Micro entrance with scale and opacity', code: '.animate-scale-in {\n  animation: scaleIn 250ms ease-out;\n}\n@keyframes scaleIn {\n  from {\n    opacity: 0;\n    transform: scale(0.95);\n  }\n  to {\n    opacity: 1;\n    transform: scale(1);\n  }\n}', usedIn: 'Modal entries, tooltip appearances, popovers' },
]

const filteredItems = computed(() => {
  if (activeCategory.value === 'all') return items
  return items.filter(i => i.category === activeCategory.value)
})

function selectItem(item: DesignSystemItem) {
  openDesignSystemItem(item)
}

function extractColorValue(code: string): string {
  const match = /(?:background-color|color|background):\s*([^;]+)/i.exec(code)
  if (!match) return '#333'
  const val = match[1].trim()
  if (val.startsWith('#') || val.startsWith('rgb') || val.startsWith('hsl')) return val
  return '#333'
}
</script>
