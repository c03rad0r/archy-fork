<template>
  <div v-if="embedded" class="space-y-3">
    <p class="text-sm">AI connections and private keys are managed by this node.</p>
    <button class="rounded-lg px-3 py-2 bg-white/10 text-sm" @click="archyBridge.requestAISetup()">Manage AI connection</button>
  </div>
  <div v-else class="space-y-4">
    <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
      API Keys
    </h3>

    <!-- Configured providers -->
    <div class="space-y-2">
      <div
        v-for="provider in providers"
        :key="provider.id"
        class="flex items-center gap-3 p-3 rounded-xl"
        :class="isDark
          ? 'bg-white/[0.03] border border-white/5'
          : 'bg-black/[0.02] border border-black/5'"
      >
        <div class="flex-1 min-w-0">
          <div
            class="text-xs font-medium"
            :class="isDark ? 'text-white/80' : 'text-gray-800'"
          >
            {{ provider.name }}
          </div>
          <div
            class="text-xs font-mono mt-0.5"
            :class="isDark ? 'text-white/30' : 'text-gray-400'"
          >
            {{ provider.masked ?? 'Not configured' }}
          </div>
        </div>
        <button
          v-if="provider.hasKey"
          class="text-xs px-2 py-1 rounded-md transition-colors"
          :class="isDark
            ? 'text-red-400/60 hover:text-red-400 hover:bg-red-500/10'
            : 'text-red-500/60 hover:text-red-600 hover:bg-red-50'"
          @click="removeKey(provider.id)"
        >
          Remove
        </button>
      </div>
    </div>

    <!-- Add new key -->
    <div class="space-y-2">
      <select
        v-model="selectedProvider"
        class="w-full px-3 py-2 rounded-lg text-xs outline-none"
        :class="isDark
          ? 'bg-white/5 text-white/80 border border-white/10'
          : 'bg-gray-50 text-gray-800 border border-gray-200'"
      >
        <option value="">Select provider...</option>
        <option value="claude">Claude (Anthropic)</option>
        <option value="openrouter">OpenRouter</option>
      </select>

      <div v-if="selectedProvider" class="flex gap-2">
        <input
          v-model="newKey"
          type="password"
          placeholder="Paste API key..."
          class="flex-1 px-3 py-2 rounded-lg text-xs outline-none"
          :class="isDark
            ? 'bg-white/5 text-white/80 placeholder:text-white/25 border border-white/10'
            : 'bg-gray-50 text-gray-800 placeholder:text-gray-400 border border-gray-200'"
          style="font-size: 16px"
          @keydown.enter="saveKey"
        />
        <button
          :disabled="!newKey.trim()"
          class="px-3 py-2 rounded-lg text-xs font-medium transition-colors"
          :class="newKey.trim()
            ? 'bg-accent text-white hover:bg-accent/90'
            : isDark
              ? 'bg-white/5 text-white/20 cursor-not-allowed'
              : 'bg-gray-100 text-gray-300 cursor-not-allowed'"
          @click="saveKey"
        >
          Save
        </button>
      </div>

      <p
        v-if="statusMessage"
        class="text-xs"
        :class="statusError ? 'text-red-400' : isDark ? 'text-green-400' : 'text-green-600'"
      >
        {{ statusMessage }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { archyBridge } from '@/services/archyBridge'
import { ref, onMounted } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { storeApiKey, getApiKey, deleteApiKey, listProviders, maskApiKey } from '@/utils/key-vault'

const embedded = archyBridge.isInArchy()
const { isDark } = useTheme()

interface ProviderInfo {
  id: string
  name: string
  hasKey: boolean
  masked: string | null
}

const providers = ref<ProviderInfo[]>([])
const selectedProvider = ref('')
const newKey = ref('')
const statusMessage = ref('')
const statusError = ref(false)

const PROVIDER_NAMES: Record<string, string> = {
  claude: 'Claude (Anthropic)',
  openrouter: 'OpenRouter',
}

async function loadProviders() {
  const configured = await listProviders()
  const allProviders = ['claude', 'openrouter']

  providers.value = await Promise.all(
    allProviders.map(async (id) => {
      const key = await getApiKey(id)
      return {
        id,
        name: PROVIDER_NAMES[id] ?? id,
        hasKey: !!key,
        masked: key ? maskApiKey(key) : null,
      }
    })
  )
}

async function saveKey() {
  if (!selectedProvider.value || !newKey.value.trim()) return
  try {
    await storeApiKey(selectedProvider.value, newKey.value.trim())
    statusMessage.value = `${PROVIDER_NAMES[selectedProvider.value] ?? selectedProvider.value} key saved`
    statusError.value = false
    newKey.value = ''
    selectedProvider.value = ''
    await loadProviders()
  } catch (err) {
    statusMessage.value = err instanceof Error ? err.message : 'Failed to store key — encryption required'
    statusError.value = true
  }
}

async function removeKey(provider: string) {
  await deleteApiKey(provider)
  statusMessage.value = `${PROVIDER_NAMES[provider] ?? provider} key removed`
  statusError.value = false
  await loadProviders()
}

onMounted(() => { if (!embedded) void loadProviders() })
</script>
