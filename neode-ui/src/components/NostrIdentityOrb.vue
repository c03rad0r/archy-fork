<template>
  <div
    class="nostr-orb"
    :class="[`nostr-orb-${size}`, `nostr-orb-${state}`]"
    role="img"
    :aria-label="ariaLabel"
  >
    <div class="nostr-orb-ring" aria-hidden="true">
      <span
        v-for="(_, i) in 48"
        :key="i"
        class="nostr-orb-segment"
        :style="{ '--seg-i': i, '--seg-deg': `${(i / 48) * 360}deg` }"
      />
    </div>
    <div class="nostr-orb-border" aria-hidden="true">
      <div class="nostr-orb-inner">
        <svg viewBox="0 0 122.88 88.39" xmlns="http://www.w3.org/2000/svg" class="nostr-orb-mark">
          <path fill="#FAFAFA" fill-rule="evenodd" clip-rule="evenodd" d="M87.51,21.16c5.26,1.45,10.79,1.84,16.58,1.18c1.42-0.16,2.81-0.35,4.16-0.53c6.46-0.84,11.86-1.32,13.78,3.52 c3.39,8.55-4.28,27.07-8.32,34.56c-8.32,15.43-24.9,32.69-44.08,27.57c-2.99-0.8-5.68-2.1-8.08-3.86 c6.3-3.51,11.28-8.9,15.13-15.24l-0.01,0.02c4.77,0.26,9.73,2.78,14.27,5.44c0.33-5.99-5.46-9.97-10.62-12.45 c4.14-9.29,6.33-19.72,7.01-29.03C87.53,29.46,87.64,25.53,87.51,21.16L87.51,21.16z M2.61,6.51c1.56-1.48,3.92-1.87,6.6-1.7 c5.03,0.31,10.23,1.86,15.11,3.18c10.61,2.86,20.99,1.93,31.1-2.74c1.36-0.63,2.69-1.28,3.98-1.9C65.56,0.37,70.8-1.9,74.31,2.3 c6.21,7.42,4.68,28.44,3.13,37.25c-3.2,18.15-14.03,40.87-34.88,42.1c-11.06,0.65-20.49-5.57-28.61-17.32 c-5.17-8-8.9-16.22-11.18-24.67C1.13,33.5-2.46,11.34,2.61,6.51L2.61,6.51z M12.94,34.3c-1.91-0.5-3.01-1.12-3.38-1.85 c-1.47-2.92,10.66-10.29,19.22-3.52C40.95,38.4,17.26,35.58,12.94,34.3L12.94,34.3z M32.63,62.79c-3.23-2.31-4.96-5.16-5.9-9.02 c10.67,5.4,20.66,5.01,29.96-2.42c-0.37,3.29-1.44,6.24-3.28,8.83C47.98,67.83,40.04,68.08,32.63,62.79L32.63,62.79z M67.07,30.06 c1.79-0.84,2.76-1.65,2.99-2.44c0.92-3.14-12.35-8.19-19.54,0.03C40.27,39.18,63.06,32.1,67.07,30.06L67.07,30.06z M90.82,42.07 c5.04-4.04,11.94-3.22,16.74,0.73c1.22,1.01,4.57,3.95,2.64,5.56c-0.53,0.44-1.41,0.69-2.63,0.75c-2.98,0.34-7.32-0.28-10.78-1.71 C94.07,46.3,92.01,44.83,90.82,42.07L90.82,42.07z"/>
        </svg>
      </div>
    </div>
    <div v-if="state === 'success'" class="nostr-orb-check" aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="m5 12 4 4L19 6"/>
      </svg>
    </div>
  </div>
</template>

<script setup lang="ts">
withDefaults(defineProps<{
  size?: 'small' | 'medium' | 'large'
  state?: 'idle' | 'loading' | 'success'
  ariaLabel?: string
}>(), {
  size: 'large',
  state: 'idle',
  ariaLabel: 'Nostr identity',
})
</script>

<style scoped>
.nostr-orb {
  --orb-size: 148px;
  --disc-size: 104px;
  --mark-width: 42px;
  --ring-radius: 60px;
  --bar-height: 14px;
  position: relative;
  width: var(--orb-size);
  height: var(--orb-size);
  flex: 0 0 auto;
}
.nostr-orb-medium { --orb-size: 112px; --disc-size: 78px; --mark-width: 32px; --ring-radius: 45px; --bar-height: 11px; }
.nostr-orb-small { --orb-size: 80px; --disc-size: 56px; --mark-width: 23px; --ring-radius: 32px; --bar-height: 8px; }
.nostr-orb-ring { position: absolute; inset: 0; pointer-events: none; }
.nostr-orb-segment {
  position: absolute;
  left: 50%; top: 50%; width: 2.5px; height: var(--bar-height);
  margin-left: -1.25px; margin-top: calc(var(--bar-height) / -2);
  border-radius: 2px;
  background: linear-gradient(to bottom, rgba(250,250,250,.45), rgba(250,250,250,.06));
  transform: rotate(var(--seg-deg)) translateY(calc(var(--ring-radius) * -1));
  animation: nostr-orb-pulse 4s ease-in-out infinite;
  animation-delay: calc(var(--seg-i) * 20ms);
}
.nostr-orb-loading .nostr-orb-segment { animation-duration: 1.35s; }
.nostr-orb-success .nostr-orb-segment { background: linear-gradient(to bottom, rgba(74,222,128,.75), rgba(74,222,128,.08)); animation-duration: 2.4s; }
.nostr-orb-border {
  position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%);
  width: var(--disc-size); height: var(--disc-size); border-radius: 9999px; padding: 3px;
  background: linear-gradient(135deg, rgba(255,255,255,.6), rgba(0,0,0,.8));
  box-shadow: 0 8px 24px rgba(0,0,0,.5); filter: drop-shadow(0 0 24px rgba(255,255,255,.08));
}
.nostr-orb-inner { width: 100%; height: 100%; border-radius: inherit; background: #000; display: flex; align-items: center; justify-content: center; }
.nostr-orb-mark { width: var(--mark-width); height: auto; filter: drop-shadow(0 0 12px rgba(250,250,250,.12)); animation: nostr-orb-breathe 4s ease-in-out infinite; }
.nostr-orb-loading .nostr-orb-mark { animation-duration: 1.5s; }
.nostr-orb-check {
  position: absolute; right: 8%; bottom: 8%; width: 27%; height: 27%; border-radius: 9999px;
  display: flex; align-items: center; justify-content: center; color: #dcfce7;
  background: #16a34a; border: 2px solid rgba(255,255,255,.8); box-shadow: 0 4px 18px rgba(22,163,74,.45);
}
.nostr-orb-check svg { width: 65%; height: 65%; }
@keyframes nostr-orb-pulse {
  0%, 100% { opacity: .15; transform: rotate(var(--seg-deg)) translateY(calc(var(--ring-radius) * -1)) scaleY(.45); }
  45%, 60% { opacity: 1; transform: rotate(var(--seg-deg)) translateY(calc(var(--ring-radius) * -1)) scaleY(1.15); }
}
@keyframes nostr-orb-breathe {
  0%, 100% { opacity: .72; transform: scale(1); }
  50% { opacity: 1; transform: scale(1.08); filter: drop-shadow(0 0 20px rgba(250,250,250,.22)); }
}
@media (prefers-reduced-motion: reduce) {
  .nostr-orb-segment, .nostr-orb-mark { animation: none; }
}
</style>
