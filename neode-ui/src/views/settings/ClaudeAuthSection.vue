<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'

const { t } = useI18n()

const apiKey = ref('')
const saved = ref(false)
const saving = ref(false)
const error = ref('')
const hasKey = ref(false)

async function checkApiKey() {
  try {
    const result = await rpcClient.call({ method: 'system.settings.get', params: { key: 'claude_api_key_set' } }) as { value: boolean } | null
    hasKey.value = !!result?.value
  } catch {
    hasKey.value = false
  }
}

async function saveApiKey() {
  if (!apiKey.value.startsWith('sk-ant-')) {
    error.value = 'API key should start with sk-ant-'
    return
  }
  saving.value = true
  error.value = ''
  saved.value = false
  try {
    await rpcClient.call({ method: 'system.settings.set', params: { key: 'claude_api_key', value: apiKey.value } })
    saved.value = true
    hasKey.value = true
    apiKey.value = ''
    setTimeout(() => { saved.value = false }, 3000)
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to save API key'
  } finally {
    saving.value = false
  }
}

async function removeApiKey() {
  try {
    await rpcClient.call({ method: 'system.settings.set', params: { key: 'claude_api_key', value: '' } })
    hasKey.value = false
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to remove API key'
  }
}

onMounted(checkApiKey)
</script>

<template>
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex items-center gap-3 mb-2">
      <div class="w-10 h-10 rounded-xl bg-orange-500/20 flex items-center justify-center">
        <svg class="w-5 h-5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
        </svg>
      </div>
      <div>
        <h3 class="text-base font-semibold text-white/96">{{ t('settings.claudeAuth') }}</h3>
        <p class="text-sm text-white/50">Enter your Anthropic API key for AI features</p>
      </div>
    </div>

    <div class="mt-4">
      <div v-if="hasKey && !apiKey" class="flex items-center justify-between bg-white/5 rounded-lg px-4 py-3 mb-3">
        <div class="flex items-center gap-2">
          <span class="w-2 h-2 rounded-full bg-green-400"></span>
          <span class="text-sm text-white/80">API key configured</span>
        </div>
        <button @click="removeApiKey" class="text-xs text-red-400 hover:text-red-300 transition-colors">Remove</button>
      </div>

      <div class="flex gap-2">
        <input
          v-model="apiKey"
          type="password"
          :placeholder="hasKey ? 'Replace existing key...' : 'sk-ant-...'"
          class="flex-1 bg-white/5 border border-white/10 rounded-lg px-4 py-2.5 text-sm text-white placeholder-white/30 focus:outline-none focus:border-white/30 transition-colors"
          @keyup.enter="saveApiKey"
        />
        <button
          @click="saveApiKey"
          :disabled="!apiKey || saving"
          class="glass-button px-4 py-2.5 text-sm font-medium disabled:opacity-30"
        >
          {{ saving ? 'Saving...' : 'Save' }}
        </button>
      </div>

      <p v-if="saved" class="text-sm text-green-400 mt-2">API key saved successfully</p>
      <p v-if="error" class="text-sm text-red-400 mt-2">{{ error }}</p>
    </div>
  </div>
</template>
