import { defineAsyncComponent } from 'vue'
import type { RendererDefinition } from '@aiui/core'

export const filmRenderer: RendererDefinition = {
  id: 'film',
  name: 'Film Renderer',
  contentType: 'film',
  surfaces: ['chat-preview', 'panel-preview', 'panel-play'],
  chatPreview: defineAsyncComponent(() => import('@/components/content/FilmGrid.vue')),
  panelPreview: defineAsyncComponent(() => import('@/components/content/FilmGrid.vue')),
  panelPlay: defineAsyncComponent(() => import('@/components/content/FilmDetail.vue')),
}
