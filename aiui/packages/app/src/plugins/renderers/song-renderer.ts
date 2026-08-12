import { defineAsyncComponent } from 'vue'
import type { RendererDefinition } from '@aiui/core'

export const songRenderer: RendererDefinition = {
  id: 'song',
  name: 'Song Renderer',
  contentType: 'song',
  surfaces: ['chat-preview', 'panel-preview', 'panel-play'],
  chatPreview: defineAsyncComponent(() => import('@/components/content/SongGrid.vue')),
  panelPreview: defineAsyncComponent(() => import('@/components/content/SongGrid.vue')),
  panelPlay: defineAsyncComponent(() => import('@/components/content/SongDetail.vue')),
}
