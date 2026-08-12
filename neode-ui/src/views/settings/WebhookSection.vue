<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import ToggleSwitch from '@/components/ToggleSwitch.vue'

const { t } = useI18n()

interface WebhookConfigData {
  enabled: boolean
  url: string
  secret: string
  events: string[]
}

const webhookConfig = ref<WebhookConfigData>({
  enabled: false,
  url: '',
  secret: '',
  events: [],
})
const savingWebhook = ref(false)
const testingWebhook = ref(false)
const webhookStatusMsg = ref('')
const webhookStatusType = ref<'success' | 'error'>('success')

const webhookEventTypes = computed(() => [
  { id: 'container_crash', label: t('settings.containerCrash'), description: t('settings.containerCrashDesc') },
  { id: 'update_available', label: t('settings.updateAvailableEvent'), description: t('settings.updateAvailableDesc') },
  { id: 'disk_warning', label: t('settings.diskSpaceWarning'), description: t('settings.diskWarningDesc') },
  { id: 'backup_complete', label: t('settings.backupComplete'), description: t('settings.backupCompleteDesc') },
])

function showWebhookStatus(msg: string, type: 'success' | 'error') {
  webhookStatusMsg.value = msg
  webhookStatusType.value = type
  setTimeout(() => { webhookStatusMsg.value = '' }, 5000)
}

function toggleWebhookEvent(id: string) {
  const idx = webhookConfig.value.events.indexOf(id)
  if (idx >= 0) {
    webhookConfig.value.events.splice(idx, 1)
  } else {
    webhookConfig.value.events.push(id)
  }
}

function toggleWebhookEnabled() {
  webhookConfig.value.enabled = !webhookConfig.value.enabled
}

async function loadWebhookConfig() {
  try {
    const res = await rpcClient.call<{ enabled: boolean; url: string; events: string[]; has_secret: boolean }>({ method: 'webhook.get-config' })
    webhookConfig.value.enabled = res.enabled
    webhookConfig.value.url = res.url
    webhookConfig.value.events = res.events || []
  } catch {
    // Webhook system may not be available
  }
}

async function saveWebhookConfig() {
  savingWebhook.value = true
  try {
    await rpcClient.call({
      method: 'webhook.configure',
      params: {
        enabled: webhookConfig.value.enabled,
        url: webhookConfig.value.url,
        secret: webhookConfig.value.secret || null,
        events: webhookConfig.value.events,
      },
    })
    showWebhookStatus(t('settings.webhookSaved'), 'success')
  } catch {
    showWebhookStatus(t('settings.webhookSaveFailed'), 'error')
  } finally {
    savingWebhook.value = false
  }
}

async function testWebhook() {
  testingWebhook.value = true
  try {
    const res = await rpcClient.call<{ sent: boolean; url: string }>({ method: 'webhook.test' })
    if (res.sent) {
      showWebhookStatus(t('settings.webhookTestSent'), 'success')
    } else {
      showWebhookStatus(t('settings.webhookTestFailed'), 'error')
    }
  } catch {
    showWebhookStatus(t('settings.webhookSendFailed'), 'error')
  } finally {
    testingWebhook.value = false
  }
}

loadWebhookConfig()
</script>

<template>
  <!-- Webhook Notifications Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex items-center justify-between mb-4">
      <div>
        <h2 class="text-xl font-semibold text-white/96">{{ t('settings.webhookNotifications') }}</h2>
        <p class="text-sm text-white/60 mt-1">{{ t('settings.webhookNotificationsDesc') }}</p>
      </div>
      <ToggleSwitch :model-value="webhookConfig.enabled" @update:model-value="toggleWebhookEnabled" />
    </div>
    <div class="space-y-4">
      <div>
        <label class="text-xs text-white/50 block mb-1">{{ t('settings.webhookUrlLabel') }}</label>
        <input
          v-model="webhookConfig.url"
          type="url"
          :placeholder="t('settings.webhookUrlPlaceholder')"
          class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-orange-500/50"
        />
      </div>
      <div>
        <label class="text-xs text-white/50 block mb-1">{{ t('settings.webhookSecretLabel') }}</label>
        <input
          v-model="webhookConfig.secret"
          type="password"
          :placeholder="t('settings.webhookSecretPlaceholderFull')"
          class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-orange-500/50"
        />
      </div>
      <div>
        <label class="text-xs text-white/50 block mb-2">{{ t('settings.eventsToNotify') }}</label>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <button
            v-for="evt in webhookEventTypes"
            :key="evt.id"
            @click="toggleWebhookEvent(evt.id)"
            role="checkbox"
            :aria-checked="webhookConfig.events.includes(evt.id)"
            :aria-label="evt.label"
            class="flex items-center gap-3 p-3 rounded-lg border transition-colors text-left"
            :class="webhookConfig.events.includes(evt.id)
              ? 'bg-orange-500/10 border-orange-500/30'
              : 'bg-white/5 border-white/10 hover:border-white/20'"
          >
            <div
              class="w-5 h-5 rounded border-2 flex items-center justify-center shrink-0 transition-colors"
              :class="webhookConfig.events.includes(evt.id)
                ? 'border-orange-500 bg-orange-500'
                : 'border-white/30'"
            >
              <svg v-if="webhookConfig.events.includes(evt.id)" class="w-3 h-3 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
              </svg>
            </div>
            <div class="min-w-0">
              <p class="text-sm text-white/90 font-medium">{{ evt.label }}</p>
              <p class="text-xs text-white/50">{{ evt.description }}</p>
            </div>
          </button>
        </div>
      </div>
      <div class="flex flex-col sm:flex-row gap-2 pt-2">
        <button
          @click="saveWebhookConfig"
          :disabled="savingWebhook"
          class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm flex items-center justify-center gap-2 disabled:opacity-50"
        >
          {{ savingWebhook ? t('settings.savingWebhook') : t('common.saveConfiguration') }}
        </button>
        <button
          @click="testWebhook"
          :disabled="testingWebhook || !webhookConfig.url"
          class="glass-button px-4 py-2 rounded-lg text-sm flex items-center justify-center gap-2 disabled:opacity-50"
        >
          {{ testingWebhook ? t('common.sending') : t('common.sendTest') }}
        </button>
      </div>
    </div>
    <div v-if="webhookStatusMsg" role="status" aria-live="polite" class="mt-3 text-xs px-3 py-2 rounded-lg" :class="webhookStatusType === 'error' ? 'alert-error' : 'alert-success'">
      {{ webhookStatusMsg }}
    </div>
  </div>
</template>
