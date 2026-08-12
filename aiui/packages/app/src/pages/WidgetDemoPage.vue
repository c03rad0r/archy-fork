<template>
  <div class="min-h-full bg-white text-gray-900 font-sans">
    <nav class="border-b border-gray-200 bg-white sticky top-0 z-30">
      <div class="max-w-6xl mx-auto px-6 py-4 flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="w-8 h-8 rounded-lg bg-blue-600 flex items-center justify-center">
            <span class="text-white text-sm font-bold">A</span>
          </div>
          <span class="text-lg font-semibold">Acme App</span>
        </div>
        <div class="flex items-center gap-6 text-sm text-gray-600">
          <a href="#" class="hover:text-gray-900">Dashboard</a>
          <a href="#" class="hover:text-gray-900">Projects</a>
          <a href="#" class="hover:text-gray-900">Settings</a>
          <RouterLink to="/" class="text-accent font-medium hover:underline">
            ← Back to AIUI
          </RouterLink>
        </div>
      </div>
    </nav>

    <div class="max-w-6xl mx-auto px-6 py-12">
      <div class="mb-12">
        <h1 class="text-3xl font-bold mb-2">Widget Integration Demo</h1>
        <p class="text-gray-500 max-w-2xl">
          This page simulates a third-party web application with the AIUI chat widget embedded.
          Click the floating button in the bottom corner to open the AI assistant.
          You can also switch which side it appears on.
        </p>
      </div>

      <div class="grid md:grid-cols-2 lg:grid-cols-3 gap-6 mb-16">
        <div v-for="n in 6" :key="n" class="bg-gray-50 rounded-2xl p-6 border border-gray-100">
          <div class="w-10 h-10 rounded-xl bg-blue-100 text-blue-600 flex items-center justify-center mb-4">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
          </div>
          <h3 class="font-semibold mb-1">Project {{ n }}</h3>
          <p class="text-sm text-gray-500 mb-3">
            Sample content card demonstrating how the widget overlays existing UI without disruption.
          </p>
          <div class="flex items-center gap-2">
            <span class="px-2.5 py-0.5 text-xs font-medium rounded-full bg-green-100 text-green-700">Active</span>
            <span class="text-xs text-gray-400">Updated 2h ago</span>
          </div>
        </div>
      </div>

      <div class="bg-gray-50 rounded-2xl border border-gray-200 p-8 mb-16">
        <h2 class="text-xl font-bold mb-4">How to embed AIUI in your app</h2>
        <div class="space-y-6 text-sm text-gray-700">
          <div>
            <h3 class="font-semibold text-gray-900 mb-2">Option 1: Script Tag (simplest)</h3>
            <pre class="bg-gray-900 text-gray-100 rounded-lg p-4 overflow-x-auto text-xs"><code>&lt;script src="https://cdn.example.com/aiui-widget.js"&gt;&lt;/script&gt;
&lt;script&gt;
  AIUI.init({
    apiKey: 'your-openrouter-key',
    position: 'bottom-right',  // or 'bottom-left'
    theme: 'dark',
  })
&lt;/script&gt;</code></pre>
          </div>

          <div>
            <h3 class="font-semibold text-gray-900 mb-2">Option 2: Web Component</h3>
            <pre class="bg-gray-900 text-gray-100 rounded-lg p-4 overflow-x-auto text-xs"><code>&lt;!-- Load the custom element --&gt;
&lt;script type="module" src="https://cdn.example.com/aiui-element.js"&gt;&lt;/script&gt;

&lt;!-- Use anywhere in your HTML --&gt;
&lt;aiui-chat
  api-key="your-key"
  position="bottom-right"
  theme="dark"
&gt;&lt;/aiui-chat&gt;</code></pre>
          </div>

          <div>
            <h3 class="font-semibold text-gray-900 mb-2">Option 3: npm package (Vue/React/Svelte)</h3>
            <pre class="bg-gray-900 text-gray-100 rounded-lg p-4 overflow-x-auto text-xs"><code>npm install @aiui/widget

// Vue 3
import { AIUIWidget } from '@aiui/widget'
app.use(AIUIWidget, {
  apiKey: 'your-key',
  position: 'bottom-right',
})

// React
import { AIUIProvider } from '@aiui/widget/react'
&lt;AIUIProvider apiKey="your-key" position="bottom-right" /&gt;</code></pre>
          </div>

          <div>
            <h3 class="font-semibold text-gray-900 mb-2">Option 4: iframe (maximum isolation)</h3>
            <pre class="bg-gray-900 text-gray-100 rounded-lg p-4 overflow-x-auto text-xs"><code>&lt;iframe
  src="https://app.aiui.dev/embed?key=your-key&amp;theme=dark"
  style="position:fixed;bottom:80px;right:24px;width:420px;height:640px;
         border:none;border-radius:16px;z-index:9999;"
  allow="microphone"
&gt;&lt;/iframe&gt;</code></pre>
          </div>
        </div>
      </div>

      <div class="bg-gray-50 rounded-2xl border border-gray-200 p-8">
        <h2 class="text-xl font-bold mb-4">Technical Architecture</h2>
        <div class="grid md:grid-cols-2 gap-8 text-sm text-gray-700">
          <div>
            <h3 class="font-semibold text-gray-900 mb-2">Script Tag / npm</h3>
            <ul class="space-y-1 list-disc list-inside">
              <li>Injects a Vue micro-app into a shadow DOM container</li>
              <li>Styles are encapsulated — no CSS conflicts with host</li>
              <li>Communicates via CustomEvents or a global AIUI API object</li>
              <li>Host app can pass context (current page, user info)</li>
              <li>Smallest footprint: ~80KB gzipped</li>
            </ul>
          </div>
          <div>
            <h3 class="font-semibold text-gray-900 mb-2">Web Component</h3>
            <ul class="space-y-1 list-disc list-inside">
              <li>Framework-agnostic Custom Element (works in any HTML)</li>
              <li>Shadow DOM encapsulation by default</li>
              <li>Attributes for configuration, events for callbacks</li>
              <li>Can be lazy-loaded with dynamic import()</li>
              <li>Works in React, Angular, Svelte, plain HTML</li>
            </ul>
          </div>
          <div>
            <h3 class="font-semibold text-gray-900 mb-2">iframe</h3>
            <ul class="space-y-1 list-disc list-inside">
              <li>Maximum isolation — separate browsing context</li>
              <li>Zero risk of CSS/JS conflicts</li>
              <li>Communication via postMessage API</li>
              <li>API key stays on embedded origin (more secure)</li>
              <li>Slightly larger overhead, but simplest integration</li>
            </ul>
          </div>
          <div>
            <h3 class="font-semibold text-gray-900 mb-2">Host Communication</h3>
            <ul class="space-y-1 list-disc list-inside">
              <li>Host can send page context to AI for better answers</li>
              <li>Widget can trigger actions in host app via callbacks</li>
              <li>Conversation history syncs via encrypted IndexedDB</li>
              <li>Deep-link support: <code class="bg-gray-200 px-1 rounded">?aiui=open&q=help</code></li>
            </ul>
          </div>
        </div>
      </div>
    </div>

    <AIUIWidget />
  </div>
</template>

<script setup lang="ts">
import { RouterLink } from 'vue-router'
import AIUIWidget from '@/components/widget/AIUIWidget.vue'
</script>
