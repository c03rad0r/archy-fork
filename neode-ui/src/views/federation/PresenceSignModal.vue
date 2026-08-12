<template>
  <Teleport to="body">
    <Transition name="presence-sign">
      <div
        v-if="show"
        class="fixed inset-0 z-[3100] flex items-center justify-center p-4"
        @click="$emit('cancel')"
      >
        <!-- Backdrop — frosted blur, same treatment as the app signer overlay -->
        <div class="absolute inset-0 bg-black/40 backdrop-blur-2xl"></div>

        <div
          ref="modalRef"
          @click.stop
          role="dialog"
          aria-modal="true"
          aria-label="Sign and publish node presence"
          class="relative z-10 w-full max-w-lg flex flex-col min-h-0 max-h-full"
        >
          <!-- Header: glass disc + radial viz ring, mirroring NostrIdentityPicker -->
          <div class="relative mb-4 sm:mb-6 flex flex-col items-center shrink-0">
            <div class="presence-hero">
              <div class="presence-viz-ring">
                <div
                  v-for="(_, i) in 48"
                  :key="i"
                  class="presence-viz-segment"
                  :style="{ '--seg-i': i, '--seg-deg': `${(i / 48) * 360}deg` }"
                />
              </div>
              <div class="presence-glass-border">
                <div class="presence-glass-inner">
                  <svg class="w-9 h-9 text-white/90" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M16.5 10.5V6.75a4.5 4.5 0 10-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 002.25-2.25v-6.75a2.25 2.25 0 00-2.25-2.25H6.75a2.25 2.25 0 00-2.25 2.25v6.75a2.25 2.25 0 002.25 2.25z" />
                  </svg>
                </div>
              </div>
            </div>

            <h2 class="mt-5 text-lg font-semibold text-white">Sign &amp; Publish Presence</h2>
            <p class="mt-1 text-white/25 tracking-widest uppercase" style="font-size: 10px;">Nostr node discovery</p>
          </div>

          <!-- Scrollable content: on short viewports (small phones) this card
               shrinks and scrolls internally so the action buttons stay in view -->
          <div class="glass-card p-4 space-y-4 overflow-y-auto min-h-0 overscroll-contain">
            <!-- Signer: single locked row, same look as the identity picker rows.
                 Node discovery always signs with the node's own key — never a
                 personal identity — so there is deliberately nothing to pick. -->
            <div>
              <div class="text-[10px] uppercase tracking-wide text-white/40 mb-1.5">Signer</div>
              <div class="w-full p-3 rounded-lg bg-white/10 ring-1 ring-white/20">
                <div class="flex items-center gap-3">
                  <div class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0 bg-white/10 text-white/80">
                    <span class="text-sm font-bold">{{ (serverName || 'N').charAt(0).toUpperCase() }}</span>
                  </div>
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-white font-semibold text-sm truncate">{{ serverName || 'This node' }}</span>
                      <span class="text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-white/60">node identity</span>
                    </div>
                    <div class="mt-0.5">
                      <span class="text-white/35 text-xs font-mono truncate">{{ npub ? truncateNpub(npub) : 'loading…' }}</span>
                    </div>
                  </div>
                  <svg class="w-4 h-4 text-white/30 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" aria-label="Locked">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M16.5 10.5V6.75a4.5 4.5 0 10-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 002.25-2.25v-6.75a2.25 2.25 0 00-2.25-2.25H6.75a2.25 2.25 0 00-2.25 2.25v6.75a2.25 2.25 0 002.25 2.25z" />
                  </svg>
                </div>
              </div>
              <p class="text-[11px] text-white/40 mt-1.5">
                Presence events are always signed with this node's dedicated discovery key —
                your personal identities are never used, so there is nothing to choose.
              </p>
            </div>

            <!-- Human-readable view of the exact content that gets signed -->
            <div>
              <div class="text-[10px] uppercase tracking-wide text-white/40 mb-1.5">What gets signed &amp; published</div>
              <dl class="text-xs space-y-1.5">
                <div class="flex flex-col sm:flex-row sm:gap-2">
                  <dt class="text-white/50 shrink-0 sm:w-28">Identity (DID)</dt>
                  <dd class="text-white/80 font-mono break-all">{{ did || '—' }}</dd>
                </div>
                <div class="flex flex-col sm:flex-row sm:gap-2">
                  <dt class="text-white/50 shrink-0 sm:w-28">Signing key</dt>
                  <dd class="text-white/80 font-mono break-all">{{ npub || '—' }}</dd>
                </div>
                <div class="flex flex-col sm:flex-row sm:gap-2">
                  <dt class="text-white/50 shrink-0 sm:w-28">Software version</dt>
                  <dd class="text-white/80">{{ version || '—' }}</dd>
                </div>
                <div class="flex flex-col sm:flex-row sm:gap-2">
                  <dt class="text-white/50 shrink-0 sm:w-28">Event format</dt>
                  <dd class="text-white/80">Nostr kind 30078, replaceable (NIP-33), tag <span class="font-mono">archipelago-node</span></dd>
                </div>
              </dl>
              <p class="text-[11px] text-white/40 mt-2">
                Every presence event carries a Schnorr signature (NIP-01) made with the key
                above — relays reject unsigned events. Your onion address is never part of
                this event; it is only shared over encrypted DMs (NIP-44) after you approve
                a peer.
              </p>
            </div>
          </div>

          <!-- Actions -->
          <div class="flex gap-3 mt-4 shrink-0">
            <button @click="$emit('cancel')" class="glass-button flex-1 py-3 rounded-lg text-sm font-medium text-white/70">
              Cancel
            </button>
            <button
              @click="$emit('confirm')"
              :disabled="busy"
              class="flex-1 py-3 rounded-lg text-sm font-semibold transition-all duration-200 bg-white/10 text-white hover:bg-white/15 disabled:opacity-30 disabled:cursor-not-allowed"
            >
              {{ busy ? 'Publishing…' : 'Sign & Publish' }}
            </button>
          </div>

          <p class="mt-3 text-center text-[10px] text-white/20 tracking-widest shrink-0">
            NIP-01 &middot; SECP256K1 &middot; SCHNORR &middot; Signed locally
          </p>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useModalKeyboard } from '@/composables/useModalKeyboard'

const props = defineProps<{
  show: boolean
  serverName: string
  npub: string
  did: string
  version: string
  busy?: boolean
}>()

const emit = defineEmits<{
  confirm: []
  cancel: []
}>()

const modalRef = ref<HTMLElement | null>(null)
useModalKeyboard(modalRef, computed(() => props.show), () => emit('cancel'))

function truncateNpub(npub: string): string {
  if (npub.length <= 20) return npub
  return npub.slice(0, 12) + '...' + npub.slice(-6)
}
</script>

<style scoped>
/* Hero container — same dimensions/pattern as the NostrIdentityPicker overlay */
.presence-hero {
  position: relative;
  width: 148px;
  height: 148px;
}

/* Small phones: shrink the hero so the signed-info card keeps most of the
   viewport; the card itself scrolls internally when it still doesn't fit. */
@media (max-width: 480px), (max-height: 740px) {
  .presence-hero {
    width: 96px;
    height: 96px;
    transform: scale(0.9);
  }
  .presence-viz-segment {
    --seg-radius: -40px;
    height: 10px;
  }
  .presence-glass-border {
    width: 72px;
    height: 72px;
  }
}

.presence-viz-ring {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}

.presence-viz-segment {
  position: absolute;
  left: 50%;
  top: 50%;
  width: 2.5px;
  height: 14px;
  margin-left: -1.25px;
  margin-top: -7px;
  background: linear-gradient(to bottom, rgba(250, 250, 250, 0.4), rgba(250, 250, 250, 0.06));
  border-radius: 1.5px;
  transform-origin: center center;
  transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px));
  animation: presence-seg-pulse 14s ease-in-out infinite;
  animation-delay: calc(var(--seg-i) * 0.02s);
}

@keyframes presence-seg-pulse {
  0%     { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
  7.1%   { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(1); }
  14.3%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
  21.4%  { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(1); }
  28.6%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
  35.7%  { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(1); }
  42.9%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
  50%    { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(1); }
  57.1%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
  64.3%  { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(1); }
  71.4%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
  78.6%  { opacity: 1;    transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(1.5); }
  85.7%  { opacity: 1;    transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(1.5); }
  92.9%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
  100%   { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(var(--seg-radius, -60px)) scaleY(0.4); }
}

.presence-glass-border {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  width: 104px;
  height: 104px;
  border-radius: 9999px;
  padding: 3px;
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.6) 0%, rgba(0, 0, 0, 0.8) 100%);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  filter: drop-shadow(0 0 24px rgba(255, 255, 255, 0.08));
}

.presence-glass-inner {
  width: 100%;
  height: 100%;
  border-radius: 9999px;
  background: #000;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* Modal transitions — same curve as the identity picker */
.presence-sign-enter-active,
.presence-sign-leave-active {
  transition: opacity 0.4s ease;
}
.presence-sign-enter-active > .relative {
  transition: transform 0.5s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.4s ease;
}
.presence-sign-leave-active > .relative {
  transition: transform 0.25s ease, opacity 0.2s ease;
}
.presence-sign-enter-from { opacity: 0; }
.presence-sign-enter-from > .relative { transform: translateY(24px) scale(0.94); opacity: 0; }
.presence-sign-leave-to { opacity: 0; }
.presence-sign-leave-to > .relative { transform: translateY(10px) scale(0.98); opacity: 0; }
</style>
