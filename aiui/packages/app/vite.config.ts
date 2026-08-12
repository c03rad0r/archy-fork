import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { VitePWA } from 'vite-plugin-pwa'
import { resolve, sep } from 'path'
import { tmdbPlugin } from './vite-tmdb'
import { devChatsPlugin } from './vite-dev-chats'
import { musicSearchPlugin } from './vite-music-search'
import { webSearchPlugin } from './vite-web-search'
import { rssPlugin } from './vite-rss'
import { fsPlugin } from './vite-fs'

export default defineConfig({
  base: process.env.VITE_BASE_PATH || '/',
  // Always define VITE_DEMO_CONTENT as a string literal so the
  // DEMO_CONTENT_ENABLED gate constant-folds at build time (an UNDEFINED
  // env var leaves `undefined === 'true'` in the bundle, which rollup's
  // treeshaker refuses to evaluate — that is how mock hosts survived in
  // the 2026-08-07 RC1 prod bundle despite the gate).
  define: {
    'import.meta.env.VITE_DEMO_CONTENT': JSON.stringify(process.env.VITE_DEMO_CONTENT ?? 'false'),
  },
  plugins: [
    vue(),
    tailwindcss(),
    tmdbPlugin(),
    devChatsPlugin(),
    musicSearchPlugin(),
    webSearchPlugin(),
    rssPlugin(),
    fsPlugin(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: [
        'favicon.svg',
        'icon.svg',
        'apple-touch-icon-180x180.png',
        'pwa-192x192.png',
        'pwa-512x512.png',
      ],
      manifest: {
        id: './',
        name: 'AIUI',
        short_name: 'AIUI',
        description: 'AI chat interface with rich content surfaces',
        theme_color: '#0a0a0a',
        background_color: '#0a0a0a',
        display: 'standalone',
        display_override: ['standalone', 'minimal-ui'],
        orientation: 'any',
        start_url: './',
        scope: './',
        icons: [
          {
            src: 'pwa-192x192.png',
            sizes: '192x192',
            type: 'image/png',
            purpose: 'any',
          },
          {
            src: 'pwa-512x512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'any',
          },
          {
            src: 'pwa-512x512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'maskable',
          },
        ],
      },
      workbox: {
        globPatterns: ['**/*.{js,css,html,svg,woff2}'],
        globIgnores: ['**/assets/img/films/**', '**/assets/img/tv/**'],
        maximumFileSizeToCacheInBytes: 5 * 1024 * 1024,
        runtimeCaching: [
          {
            urlPattern: /^https:\/\/api\.anthropic\.com\/.*/i,
            handler: 'NetworkOnly',
          },
          {
            urlPattern: /^https:\/\/openrouter\.ai\/.*/i,
            handler: 'NetworkOnly',
          },
          {
            urlPattern: /\/api\/web-search\?.*/i,
            handler: 'NetworkOnly',
          },
          {
            urlPattern: /\/api\/rss-articles\?.*/i,
            handler: 'NetworkOnly',
          },
          {
            urlPattern: /\/api\/tmdb\/.*/i,
            handler: 'StaleWhileRevalidate',
            options: {
              cacheName: 'tmdb-cache',
              expiration: { maxEntries: 200, maxAgeSeconds: 86400 },
            },
          },
          {
            urlPattern: /^https:\/\/image\.tmdb\.org\/.*/i,
            handler: 'CacheFirst',
            options: {
              cacheName: 'tmdb-images',
              expiration: { maxEntries: 500, maxAgeSeconds: 604800 },
            },
          },
          {
            urlPattern: /^https:\/\/upload\.wikimedia\.org\/.*/i,
            handler: 'CacheFirst',
            options: {
              cacheName: 'wiki-images',
              expiration: { maxEntries: 200, maxAgeSeconds: 604800 },
            },
          },
          {
            urlPattern: /^https:\/\/d12wklypp119aj\.cloudfront\.net\/image\/.*/i,
            handler: 'CacheFirst',
            options: {
              cacheName: 'wavlake-images',
              expiration: { maxEntries: 300, maxAgeSeconds: 604800 },
            },
          },
        ],
      },
      devOptions: {
        enabled: true,
      },
    }),
  ],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
      '@aiui/core': resolve(__dirname, '../core/src'),
    },
  },
  build: {
    rollupOptions: {
      treeshake: {
        // Mock modules are pure data by design — but a module-level
        // `allGenres = [...new Set(mockFilms.flatMap(...))]` export made
        // films.ts look side-effectful, so the treeshaker kept the whole
        // module (and every mock host string) in the prod bundle even with
        // zero live references. Declaring the mocks directory
        // side-effect-free lets the demo-content gate actually drop them.
        moduleSideEffects: (id) => !id.includes(`${sep}src${sep}mocks${sep}`),
      },
    },
  },
  server: {
    port: 5173,
    host: process.env.VITE_HOST === 'false' ? 'localhost' : true,
    open: false,
    proxy: {
      '/api/claude': {
        target: 'http://localhost:3141',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api\/claude/, ''),
      },
      '/api/openrouter': {
        target: 'http://localhost:3141',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api\/openrouter/, '/v1/openrouter'),
      },
    },
  },
})
