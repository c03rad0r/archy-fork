<template>
  <!-- Companion app banner — same format as the featured app banner, with a
       phone mockup rising out of the right edge. Clicking anywhere (or the
       Install button) opens the Remote Companion download/pairing modal.
       Not rendered at all inside the companion app's WebView: pitching
       "install the companion" to someone already in it is noise (#61). -->
  <div
    v-if="showPitch"
    class="featured-banner companion-banner glass-card mb-8 relative overflow-hidden cursor-pointer"
    @click="openCompanionIntro()"
  >
    <img
      src="/assets/img/companion-banner-bg.webp"
      alt=""
      class="featured-banner-img"
      @error="(e: Event) => ((e.target as HTMLImageElement).style.display = 'none')"
    />
    <div class="featured-banner-overlay companion-banner-overlay">
      <div class="flex items-center gap-3 mb-2">
        <span class="discover-terminal-tag">companion</span>
        <span class="text-white/50 text-sm font-mono">REMOTE CONTROL // IN YOUR POCKET</span>
      </div>
      <h2 class="text-3xl md:text-4xl font-extrabold text-white mb-2 tracking-tight">Your Node. In Your Pocket.</h2>
      <p class="text-white/80 text-base md:text-lg max-w-2xl leading-relaxed mb-4">
        The Archipelago Companion puts your node on your phone — remote control and typing,
        your cloud files on the go, and one scan to connect. Sovereignty doesn't stay home.
      </p>
      <div class="flex items-center gap-3">
        <button
          @click.stop="openCompanionIntro()"
          class="glass-button rounded-lg px-6 py-2.5 text-sm font-medium"
        >Install</button>
        <span class="text-white/40 text-sm">Archipelago Companion · Android</span>
      </div>
    </div>

    <div class="companion-phone-wrap" aria-hidden="true">
      <div class="companion-phone">
        <span class="companion-phone-lens"></span>
        <img src="/assets/img/companion-phone-screen.webp" alt="" class="companion-phone-screen" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { isCompanionApp } from '@/utils/openExternal'
import { openCompanionIntro } from '@/composables/useCompanionIntro'

const showPitch = computed(() => !isCompanionApp())
</script>

<style scoped>
.companion-banner {
  border-color: rgba(251, 146, 60, 0.25);
}

/* Keep the copy clear of the phone mockup on wide screens */
@media (min-width: 768px) {
  .companion-banner-overlay {
    padding-right: 320px;
  }
}

.companion-phone-wrap {
  display: none;
}
@media (min-width: 768px) {
  .companion-phone-wrap {
    display: block;
    position: absolute;
    top: 2rem;
    right: 3.5rem;
    z-index: 2;
    transform: rotate(6deg);
    transition: transform 0.4s ease;
  }
  .companion-banner:hover .companion-phone-wrap {
    transform: rotate(4deg) translateY(-8px);
  }
}

.companion-phone {
  position: relative;
  width: 200px;
  padding: 9px;
  border-radius: 30px;
  background: #0b0b12;
  border: 1px solid rgba(255, 255, 255, 0.25);
  box-shadow:
    0 24px 60px rgba(0, 0, 0, 0.55),
    0 0 46px rgba(251, 146, 60, 0.28),
    inset 0 0 2px rgba(255, 255, 255, 0.18);
}

/* Punch-hole camera */
.companion-phone-lens {
  position: absolute;
  top: 17px;
  left: 50%;
  transform: translateX(-50%);
  width: 9px;
  height: 9px;
  border-radius: 9999px;
  background: #000;
  border: 1px solid rgba(255, 255, 255, 0.14);
  z-index: 2;
}

.companion-phone-screen {
  display: block;
  width: 100%;
  border-radius: 22px;
}

/* Screen glare */
.companion-phone::after {
  content: '';
  position: absolute;
  inset: 9px;
  border-radius: 22px;
  background: linear-gradient(115deg, rgba(255, 255, 255, 0.14) 0%, rgba(255, 255, 255, 0.04) 28%, transparent 45%);
  pointer-events: none;
}
</style>
