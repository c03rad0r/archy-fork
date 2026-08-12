<template>
  <div class="min-h-screen bg-black flex items-center justify-center p-6">
    <div class="glass-card p-8 w-full max-w-lg">
      <div class="text-center mb-6">
        <h1 class="text-2xl font-bold text-white mb-1">{{ t('kioskRecovery.title') }}</h1>
        <p class="text-sm text-white/50">{{ t('kioskRecovery.subtitle') }}</p>
      </div>

      <!-- Server IP -->
      <div class="bg-white/5 rounded-lg p-4 mb-4">
        <div class="text-xs text-white/50 mb-1">{{ t('kioskRecovery.serverAddress') }}</div>
        <div class="text-lg font-mono text-white font-medium">{{ serverIp || t('common.loading') }}</div>
        <div v-if="serverIp" class="text-xs text-white/40 mt-1">{{ t('kioskRecovery.webUi', { address: serverIp }) }}</div>
      </div>

      <!-- QR Code -->
      <div v-if="serverIp" class="bg-white/5 rounded-lg p-4 mb-4 flex flex-col items-center">
        <div class="text-xs text-white/50 mb-2">{{ t('kioskRecovery.scanForMobile') }}</div>
        <div class="bg-white p-3 rounded-lg inline-block">
          <img :src="qrCodeUrl" alt="QR Code" class="w-32 h-32" />
        </div>
      </div>

      <!-- Diagnostics -->
      <div class="space-y-2 mb-6">
        <div class="flex items-center justify-between bg-white/5 rounded-lg p-3">
          <span class="text-sm text-white/70">{{ t('kioskRecovery.backend') }}</span>
          <div class="flex items-center gap-2">
            <div class="w-2 h-2 rounded-full" :class="backendHealthy ? 'bg-green-400' : 'bg-red-400'"></div>
            <span class="text-sm" :class="backendHealthy ? 'text-green-400' : 'text-red-400'">
              {{ backendHealthy ? t('common.healthy') : t('kioskRecovery.unreachable') }}
            </span>
          </div>
        </div>
        <div class="flex items-center justify-between bg-white/5 rounded-lg p-3">
          <span class="text-sm text-white/70">{{ t('kioskRecovery.containers') }}</span>
          <span class="text-sm text-white font-medium">{{ containerCount }}</span>
        </div>
        <div class="flex items-center justify-between bg-white/5 rounded-lg p-3">
          <span class="text-sm text-white/70">{{ t('monitoring.diskUsage') }}</span>
          <span class="text-sm text-white font-medium">{{ diskUsage }}</span>
        </div>
      </div>

      <!-- Actions -->
      <div class="flex gap-3">
        <button @click="refreshDiagnostics" class="glass-button px-4 py-2 rounded-lg text-sm flex-1">
          {{ t('common.refresh') }}
        </button>
        <button @click="goToLogin" class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm flex-1">
          {{ t('kioskRecovery.goToLogin') }}
        </button>
      </div>

      <div class="text-center mt-4">
        <p class="text-xs text-white/30">{{ t('kioskRecovery.lastChecked', { time: lastChecked }) }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

const router = useRouter()
const { t } = useI18n()

const serverIp = ref('')
const backendHealthy = ref(false)
const containerCount = ref('—')
const diskUsage = ref('—')
const lastChecked = ref('—')

const qrCodeUrl = computed(() => {
  if (!serverIp.value) return ''
  const url = `http://${serverIp.value}`
  return `https://api.qrserver.com/v1/create-qr-code/?size=128x128&data=${encodeURIComponent(url)}`
})

async function refreshDiagnostics() {
  lastChecked.value = new Date().toLocaleTimeString()

  // Detect server IP from window location
  serverIp.value = window.location.hostname !== 'localhost'
    ? window.location.hostname
    : '127.0.0.1'

  // Check backend health
  try {
    const res = await fetch('/health', { signal: AbortSignal.timeout(5000) })
    backendHealthy.value = res.ok
  } catch {
    backendHealthy.value = false
  }

  // Get system stats (unauthenticated won't work for RPC, but try health)
  if (backendHealthy.value) {
    try {
      const statsRes = await fetch('/rpc/', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ method: 'system.stats' }),
        signal: AbortSignal.timeout(5000),
      })
      const data = await statsRes.json()
      if (data.result) {
        const disk = data.result.disk
        if (disk) {
          const usedPct = ((disk.used / disk.total) * 100).toFixed(0)
          diskUsage.value = `${usedPct}% used`
        }
        containerCount.value = String(data.result.containers?.running ?? '—')
      }
    } catch {
      // Stats require auth — show defaults
      containerCount.value = '—'
      diskUsage.value = '—'
    }
  }
}

function goToLogin() {
  router.push('/login')
}

onMounted(() => {
  refreshDiagnostics()
})
</script>
