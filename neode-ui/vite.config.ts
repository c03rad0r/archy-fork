import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { VitePWA } from 'vite-plugin-pwa'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['assets/icon/favico-black-v2.svg', 'assets/icon/pwa-64x64-v2.png', 'assets/icon/pwa-192x192-v2.png', 'assets/icon/pwa-512x512-v2.png', 'assets/icon/maskable-icon-512x512-v2.png', 'assets/icon/apple-touch-icon-180x180-v2.png', 'favicon-v2.ico'],
      manifest: {
        name: 'Archipelago',
        short_name: 'Archipelago',
        description: 'Your sovereign personal server',
        theme_color: '#000000',
        background_color: '#000000',
        display: 'standalone',
        display_override: ['standalone', 'minimal-ui'],
        orientation: 'any',
        scope: '/',
        start_url: '/',
        id: '/',
        categories: ['productivity', 'utilities'],
        prefer_related_applications: false,
        icons: [
          { src: '/assets/icon/pwa-64x64-v2.png', sizes: '64x64', type: 'image/png', purpose: 'any' },
          { src: '/assets/icon/pwa-192x192-v2.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
          { src: '/assets/icon/pwa-512x512-v2.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
          { src: '/assets/icon/maskable-icon-512x512-v2.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' }
        ],
        shortcuts: [
          { name: 'Dashboard', short_name: 'Dashboard', description: 'Open the dashboard', url: '/dashboard', icons: [{ src: '/assets/icon/pwa-192x192-v2.png', sizes: '192x192' }] },
          { name: 'My Apps', short_name: 'Apps', description: 'Manage your apps', url: '/dashboard/apps', icons: [{ src: '/assets/icon/pwa-192x192-v2.png', sizes: '192x192' }] },
          { name: 'App Store', short_name: 'Store', description: 'Browse and install apps', url: '/dashboard/marketplace', icons: [{ src: '/assets/icon/pwa-192x192-v2.png', sizes: '192x192' }] }
        ]
      },
      workbox: {
        // /packages/ must bypass the SPA fallback — otherwise clicking the
        // companion APK download link gets index.html instead of the file.
        // The signer broker is a security-sensitive cross-window protocol,
        // not an offline screen. Serving an old precached SPA shell here can
        // preserve a broken session probe after the node has been updated.
        navigateFallbackDenylist: [
          /^\/app\//,
          /^\/rpc\//,
          /^\/ws/,
          /^\/aiui\//,
          /^\/packages\//,
          /^\/nostr-signer(?:\/|$)/,
        ],
        cleanupOutdatedCaches: true,
        globPatterns: ['**/*.{js,css,html,ico,png,svg,jpg,jpeg,mp4,webp}'],
        globIgnores: [
          '**/*-backup-*.mp4',
          '**/*-1.47mb.mp4',
          '**/bg-*.mp4', // Exclude large background videos from precache
          '**/video-intro*.mp4', // Exclude all intro video variants from precache
          '**/assets/icon/**', // Icons are in includeAssets — don't duplicate in glob precache
          '**/nostr-provider.js', // Always use the node's current signer protocol
        ],
        maximumFileSizeToCacheInBytes: 10 * 1024 * 1024, // 10MB limit
        skipWaiting: false, // Wait for user to accept update
        clientsClaim: false, // Don't claim clients immediately
        runtimeCaching: [
          {
            urlPattern: /^https:\/\/fonts\.googleapis\.com\/.*/i,
            handler: 'CacheFirst',
            options: {
              cacheName: 'google-fonts-cache',
              expiration: {
                maxEntries: 10,
                maxAgeSeconds: 60 * 60 * 24 * 365 // 1 year
              },
              cacheableResponse: {
                statuses: [0, 200]
              }
            }
          },
          {
            urlPattern: /^https:\/\/fonts\.gstatic\.com\/.*/i,
            handler: 'CacheFirst',
            options: {
              cacheName: 'gstatic-fonts-cache',
              expiration: {
                maxEntries: 10,
                maxAgeSeconds: 60 * 60 * 24 * 365 // 1 year
              },
              cacheableResponse: {
                statuses: [0, 200]
              }
            }
          },
          {
            urlPattern: /\/rpc\/v1\/.*/i,
            handler: 'NetworkFirst',
            options: {
              cacheName: 'api-cache',
              expiration: {
                maxEntries: 50,
                maxAgeSeconds: 60 * 5 // 5 minutes
              },
              networkTimeoutSeconds: 10
            }
          },
          {
            urlPattern: /\/assets\/.*/i,
            handler: 'CacheFirst',
            options: {
              cacheName: 'assets-cache-v3',
              expiration: {
                maxEntries: 100,
                maxAgeSeconds: 60 * 60 * 24 * 30 // 30 days
              }
            }
          }
        ]
      },
      devOptions: {
        enabled: false,
      }
    })
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src')
    }
  },
  server: {
    host: true, // listen on 0.0.0.0 so the dev UI is reachable over the LAN (e.g. http://192.0.2.12:8100)
    port: 8100,
    proxy: {
      '/rpc/v1': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        changeOrigin: true,
        secure: false,
      },
      '/ws': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        ws: true,
        changeOrigin: true,
        secure: false,
        rewrite: (path) => path, // Don't rewrite the path
        timeout: 0, // No timeout for WebSocket connections
      },
      '/public': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        changeOrigin: true,
        secure: false,
      },
      '/rest': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        changeOrigin: true,
        secure: false,
      },
      // Peer-content streaming, blob upload, app catalog — the real node's
      // nginx proxies /api/* to the daemon; dev forwards it to the mock.
      '/api': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        changeOrigin: true,
        secure: false,
      },
      // Mock filebrowser (Cloud page) — same backend as everything else. Point
      // BACKEND_URL at a real node to develop against its filebrowser instead.
      '/app/filebrowser': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        changeOrigin: true,
        secure: false,
      },
      '/app/bitcoin-ui': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        changeOrigin: true,
        secure: false,
      },
      '/app/cuprate-ui': {
        target: process.env.BACKEND_URL || 'http://localhost:5959',
        changeOrigin: true,
        secure: false,
      },
      // Demo mock app UIs (electrumx, lnd, fedimint) + generic notice page.
      '/app/electrumx': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/app/electrs': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/app/lnd': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/app/fedimint': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/app/bitcoin-core': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/app/bitcoin-knots': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      // Catch-all for the remaining mock app UIs (btcpay-server, grafana, …) and
      // the generic "Not available in the demo" notice. Registered after the
      // specific /app/* keys so those still win.
      '/app': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/electrs-status': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/proxy': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      '/lnd-connect-info': { target: process.env.BACKEND_URL || 'http://localhost:5959', changeOrigin: true, secure: false },
      // Serve the node's deployed AIUI same-origin like production (set VITE_AIUI_URL=/aiui/)
      '/aiui': {
        target: process.env.AIUI_PROXY_TARGET || 'http://127.0.0.1:80',
        changeOrigin: true,
        secure: false,
      },
    },
  },
  build: {
    // Output to dist for Docker builds, or to ../web/dist/neode-ui for local development
    outDir: process.env.DOCKER_BUILD === 'true' ? 'dist' : '../web/dist/neode-ui',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          vendor: ['vue', 'vue-router', 'pinia'],
        }
      }
    }
  },
})
