import { registerPlugin, registerRenderer } from '@aiui/core'

export async function initializePlugins(): Promise<void> {
  // Register built-in AI provider
  const { claudeProvider } = await import('./claude-provider')
  registerPlugin(claudeProvider)
  await claudeProvider.init({
    settings: {
      get: () => undefined,
      set: () => {},
    },
    events: {
      emit: () => {},
      on: () => () => {},
    },
    logger: {
      info: console.info.bind(console, '[AIUI plugin]'),
      warn: console.warn.bind(console, '[AIUI plugin]'),
      error: console.error.bind(console, '[AIUI plugin]'),
    },
  })

  // Register built-in content renderers
  const { filmRenderer } = await import('./renderers/film-renderer')
  const { songRenderer } = await import('./renderers/song-renderer')
  registerRenderer(filmRenderer)
  registerRenderer(songRenderer)

  // Register built-in search plugins
  const { wikipediaPlugin } = await import('./wikipedia')
  const { openLibraryPlugin } = await import('./openlibrary')
  registerPlugin(wikipediaPlugin)
  registerPlugin(openLibraryPlugin)

  if (import.meta.env.DEV) {
    console.log('[AIUI] Plugins initialized:', claudeProvider.id)
    console.log('[AIUI] Renderers registered: film, song')
    console.log('[AIUI] Search plugins: wikipedia, openlibrary')
  }
}
