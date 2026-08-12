<template>
  <div>
    <!-- Hero Section -->
    <div class="discover-hero glass-card p-8 md:p-12 mb-8 relative overflow-hidden">
      <div class="discover-hero-scanline" aria-hidden="true"></div>
      <div class="discover-hero-layout relative z-10">
        <div class="discover-hero-content">
          <div class="flex items-center gap-3 mb-4">
            <span class="discover-terminal-tag">~ $</span>
            <span class="text-white/40 text-sm font-mono tracking-wider">ARCHIPELAGO://DISCOVER</span>
          </div>
          <h1 class="text-4xl md:text-5xl font-extrabold text-white mb-4 tracking-tight font-archipelago">
            Reclaim Your<br />
            <span class="discover-hero-accent">Digital Sovereignty</span>
          </h1>
          <p class="text-white/70 text-lg md:text-xl max-w-2xl leading-relaxed mb-6">
            Your node. Your rules. Every app runs on <em>your</em> hardware, verified by <em>your</em> Bitcoin node.
            No cloud. No custodians. No permission needed.
          </p>
          <div class="flex flex-wrap gap-4 text-sm">
            <div class="discover-stat-pill">
              <span class="text-white font-bold">{{ totalApps }}</span>
              <span class="text-white/50">apps available</span>
            </div>
            <div class="discover-stat-pill">
              <span class="text-white font-bold">{{ installedCount }}</span>
              <span class="text-white/50">installed</span>
            </div>
            <div class="discover-stat-pill">
              <span class="text-white font-bold">100%</span>
              <span class="text-white/50">self-hosted</span>
            </div>
          </div>
        </div>
        <div class="discover-hero-face">
          <BitcoinFaceAscii />
        </div>
      </div>
    </div>

    <!-- Principles Row -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-10">
      <div class="discover-principle-card">
        <svg class="w-6 h-6 text-orange-400 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
        </svg>
        <h3 class="text-white text-base font-bold mb-1">Privacy First</h3>
        <p class="text-white/50 text-sm leading-relaxed">No telemetry. No tracking. Your data never leaves your hardware.</p>
      </div>
      <div class="discover-principle-card">
        <svg class="w-6 h-6 text-orange-400 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
        </svg>
        <h3 class="text-white text-base font-bold mb-1">Verify, Don't Trust</h3>
        <p class="text-white/50 text-sm leading-relaxed">Run your own node. Validate every transaction. Be your own bank.</p>
      </div>
      <div class="discover-principle-card">
        <svg class="w-6 h-6 text-orange-400 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
        </svg>
        <h3 class="text-white text-base font-bold mb-1">Open Source</h3>
        <p class="text-white/50 text-sm leading-relaxed">Every app is open source. Audit the code. Trust the math, not the company.</p>
      </div>
      <div class="discover-principle-card">
        <svg class="w-6 h-6 text-orange-400 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <h3 class="text-white text-base font-bold mb-1">No Permission Needed</h3>
        <p class="text-white/50 text-sm leading-relaxed">Permissionless commerce. Permissionless money. Permissionless freedom.</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import BitcoinFaceAscii from './BitcoinFaceAscii.vue'

defineProps<{
  totalApps: number
  installedCount: number
}>()
</script>
