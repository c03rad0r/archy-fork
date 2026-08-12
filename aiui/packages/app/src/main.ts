import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import './styles/main.css'
import { initializePlugins } from './plugins'

// PWA cache version — bump this when icons or critical assets change
const PWA_CACHE_VERSION = '2'

// Purge all service worker caches when version changes
;(async () => {
  const key = 'aiui-cache-version'
  const stored = localStorage.getItem(key)
  if (stored !== PWA_CACHE_VERSION) {
    localStorage.setItem(key, PWA_CACHE_VERSION)
    if ('caches' in window) {
      const names = await caches.keys()
      await Promise.all(names.map((name) => caches.delete(name)))
    }
    if ('serviceWorker' in navigator) {
      const registrations = await navigator.serviceWorker.getRegistrations()
      await Promise.all(registrations.map((r) => r.unregister()))
    }
    if (stored !== null) {
      // Only reload if this is an upgrade, not a fresh install
      window.location.reload()
    }
  }
})()

// Capture embedded flag before router init (survives SPA navigation)
// Only embedded when explicitly requested via ?embedded param
const _embeddedFlag = new URLSearchParams(window.location.search).has('embedded')
;(window as unknown as Record<string, unknown>).__AIUI_EMBEDDED__ = _embeddedFlag

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'chat',
      component: () => import('./pages/ChatPage.vue'),
    },
    {
      path: '/guide',
      name: 'guide',
      component: () => import('./pages/GuidePage.vue'),
    },
    {
      path: '/widget-demo',
      name: 'widget-demo',
      component: () => import('./pages/WidgetDemoPage.vue'),
    },
    {
      path: '/browse',
      name: 'file-browser',
      component: () => import('./pages/BrowsePage.vue'),
    },
    {
      path: '/view/:nostrAddr',
      name: 'conversation-viewer',
      component: () => import('./pages/ConversationViewerPage.vue'),
    },
  ],
})

const app = createApp(App)
app.use(createPinia())
app.use(router)

initializePlugins().catch((err) => {
  console.error('[AIUI] Failed to initialize plugins:', err)
})

app.mount('#app')
