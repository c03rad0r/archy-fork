<template>
  <div>
    <!-- Quick Actions -->
    <div class="glass-card p-6 mb-6">
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <!-- Link Your Nodes (Trusted) -->
        <div data-controller-container tabindex="0" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
          <div class="flex items-center gap-3 min-w-0">
            <svg class="w-5 h-5 text-green-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
            </svg>
            <div class="min-w-0">
              <p class="text-sm font-medium text-white">Link Your Nodes</p>
              <p class="text-xs text-white/60">Full trust, sync everything</p>
            </div>
          </div>
          <button
            @click="$emit('generate-invite', 'trusted')"
            class="w-full sm:w-fit px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
            :disabled="generatingInvite"
          >
            {{ generatingInvite && inviteType === 'trusted' ? 'Generating...' : 'Generate Code' }}
          </button>
        </div>

        <!-- Invite a Peer (Observer) -->
        <div data-controller-container tabindex="0" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
          <div class="flex items-center gap-3 min-w-0">
            <svg class="w-5 h-5 text-orange-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
            <div class="min-w-0">
              <p class="text-sm font-medium text-white">Invite a Peer</p>
              <p class="text-xs text-white/60">Share public content</p>
            </div>
          </div>
          <button
            @click="$emit('generate-invite', 'observer')"
            class="w-full sm:w-fit px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
            :disabled="generatingInvite"
          >
            {{ generatingInvite && inviteType === 'observer' ? 'Generating...' : 'Generate Code' }}
          </button>
        </div>

        <!-- Join (accept code) -->
        <div data-controller-container tabindex="0" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
          <div class="flex items-center gap-3 min-w-0">
            <svg class="w-5 h-5 text-blue-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 16l-4-4m0 0l4-4m-4 4h14m-5 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h7a3 3 0 013 3v1" />
            </svg>
            <div class="min-w-0">
              <p class="text-sm font-medium text-white">Join</p>
              <p class="text-xs text-white/60">Accept an invite code</p>
            </div>
          </div>
          <button
            @click="$emit('show-join')"
            class="w-full sm:w-fit px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
          >
            Enter Code
          </button>
        </div>

        <!-- Sync State -->
        <div data-controller-container tabindex="0" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
          <div class="flex items-center gap-3 min-w-0">
            <svg class="w-5 h-5 text-green-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            <div class="min-w-0">
              <p class="text-sm font-medium text-white">Sync</p>
              <p class="text-xs text-white/60">Refresh all node states</p>
            </div>
          </div>
          <button
            @click="$emit('sync')"
            class="w-full sm:w-fit px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
            :disabled="syncing"
          >
            {{ syncing ? 'Syncing...' : 'Sync Now' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Invite Code Modal -->
    <Teleport to="body">
      <Transition name="modal">
        <div v-if="inviteCode" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click.self="$emit('clear-invite')">
          <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
          <div class="glass-card p-6 max-w-md w-full relative z-10">
            <div class="flex items-center justify-between mb-4">
              <h2 class="text-lg font-semibold text-white">{{ inviteType === 'trusted' ? 'Link Your Nodes — Invite Code' : 'Peer Invite Code' }}</h2>
              <button @click="$emit('clear-invite')" class="text-white/40 hover:text-white/70 transition-colors">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <p class="text-xs text-white/60 mb-3">Share this code with the node you want to federate with. It can only be used once.</p>
            <div class="bg-black/30 rounded-lg p-4 font-mono text-xs text-orange-300 break-all select-all">{{ inviteCode }}</div>
            <button
              @click="handleCopyInvite"
              class="mt-3 px-4 py-2 glass-button rounded text-sm text-white/90 hover:text-white transition-colors"
            >
              {{ copiedInvite ? 'Copied' : 'Copy to Clipboard' }}
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{
  generatingInvite: boolean
  inviteType: 'trusted' | 'observer'
  inviteCode: string
  syncing: boolean
}>()

defineEmits<{
  'generate-invite': [type: 'trusted' | 'observer']
  'show-join': []
  'sync': []
  'clear-invite': []
}>()

const copiedInvite = ref(false)

async function handleCopyInvite() {
  try {
    await window.navigator.clipboard.writeText(props.inviteCode)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = props.inviteCode
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  }
  copiedInvite.value = true
  setTimeout(() => { copiedInvite.value = false }, 2000)
}
</script>
