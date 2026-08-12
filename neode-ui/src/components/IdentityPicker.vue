<template>
  <div class="relative" ref="pickerRef">
    <button
      @click="isOpen = !isOpen"
      class="w-full flex items-center gap-3 px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-sm text-white hover:bg-white/10 transition-colors"
    >
      <!-- Selected Identity -->
      <div v-if="selectedIdentity" class="flex items-center gap-2 flex-1 min-w-0">
        <div class="w-6 h-6 rounded-full flex items-center justify-center shrink-0" :class="purposeColor(selectedIdentity.purpose)">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
          </svg>
        </div>
        <span class="truncate">{{ selectedIdentity.name }}</span>
        <span class="text-white/40 text-xs font-mono truncate">{{ truncateDid(selectedIdentity.did) }}</span>
      </div>
      <div v-else class="flex-1 text-white/50">Select identity...</div>

      <!-- Chevron -->
      <svg class="w-4 h-4 text-white/40 shrink-0 transition-transform" :class="{ 'rotate-180': isOpen }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </button>

    <!-- Dropdown -->
    <Transition name="content-fade">
      <div v-if="isOpen" class="absolute left-0 right-0 mt-1 z-20 glass-card p-1 rounded-lg max-h-48 overflow-y-auto">
        <div v-if="loading" class="p-3 text-center text-white/50 text-sm">Loading...</div>
        <div v-else-if="identities.length === 0" class="p-3 text-center text-white/50 text-sm">No identities</div>
        <button
          v-for="id in identities"
          :key="id.id"
          @click="selectIdentity(id)"
          class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-sm text-white/80 hover:bg-white/10 transition-colors"
          :class="{ 'bg-white/10': modelValue === id.id }"
        >
          <div class="w-6 h-6 rounded-full flex items-center justify-center shrink-0" :class="purposeColor(id.purpose)">
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
            </svg>
          </div>
          <div class="flex-1 min-w-0 text-left">
            <div class="flex items-center gap-1">
              <span class="truncate">{{ id.name }}</span>
              <span v-if="id.is_default" class="text-yellow-400 text-xs">&#9733;</span>
            </div>
            <p class="text-white/40 text-xs font-mono truncate">{{ truncateDid(id.did) }}</p>
          </div>
        </button>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { rpcClient } from '@/api/rpc-client'

interface Identity {
  id: string
  name: string
  purpose: string
  pubkey: string
  did: string
  is_default: boolean
}

const props = defineProps<{
  modelValue?: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', id: string): void
  (e: 'select', identity: Identity): void
}>()

const isOpen = ref(false)
const loading = ref(false)
const identities = ref<Identity[]>([])
const pickerRef = ref<HTMLElement | null>(null)

const selectedIdentity = computed(() =>
  identities.value.find(i => i.id === props.modelValue)
)

function purposeColor(purpose: string): string {
  switch (purpose) {
    case 'personal': return 'bg-blue-500/20 text-blue-400'
    case 'business': return 'bg-orange-500/20 text-orange-400'
    case 'anonymous': return 'bg-purple-500/20 text-purple-400'
    default: return 'bg-white/10 text-white/60'
  }
}

function truncateDid(did: string): string {
  if (did.length <= 30) return did
  return did.slice(0, 18) + '...' + did.slice(-8)
}

function selectIdentity(id: Identity) {
  emit('update:modelValue', id.id)
  emit('select', id)
  isOpen.value = false
}

function onClickOutside(e: MouseEvent) {
  if (pickerRef.value && !pickerRef.value.contains(e.target as Node)) {
    isOpen.value = false
  }
}

async function loadIdentities() {
  loading.value = true
  try {
    const res = await rpcClient.call<{ identities: Identity[] }>({ method: 'identity.list' })
    identities.value = res.identities || []
    // Auto-select default if no value set
    if (!props.modelValue) {
      const defaultId = identities.value.find(i => i.is_default)
      if (defaultId) {
        emit('update:modelValue', defaultId.id)
        emit('select', defaultId)
      }
    }
  } catch {
    identities.value = []
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  loadIdentities()
  document.addEventListener('pointerdown', onClickOutside)
})

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onClickOutside)
})
</script>
