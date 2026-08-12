<script setup lang="ts">
import { computed, ref } from 'vue'
import DOMPurify from 'dompurify'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'

const { t } = useI18n()

// 2FA / TOTP
const totpEnabled = ref(false)
const showTotpSetupModal = ref(false)
const showTotpDisableModal = ref(false)
const totpSetupStep = ref(1)
const totpSetupPassword = ref('')
const totpSetupCode = ref('')
const totpSetupError = ref('')
const totpSetupLoading = ref(false)
const totpQrSvg = ref('')
const sanitizedQrSvg = computed(() => DOMPurify.sanitize(totpQrSvg.value, { USE_PROFILES: { svg: true } }))
const totpSecretBase32 = ref('')
const showTotpSecret = ref(false)
const totpPendingToken = ref('')
const totpBackupCodes = ref<string[]>([])
const backupCodesCopied = ref(false)
const totpDisablePassword = ref('')
const totpDisableCode = ref('')
const totpDisableError = ref('')
const totpDisableLoading = ref(false)

async function loadTotpStatus() {
  try {
    const res = await rpcClient.totpStatus()
    totpEnabled.value = res.enabled
  } catch (e) {
    if (import.meta.env.DEV) console.warn('TOTP status may not be available', e)
  }
}

async function beginTotpSetup() {
  totpSetupError.value = ''
  totpSetupLoading.value = true
  try {
    const res = await rpcClient.totpSetupBegin(totpSetupPassword.value)
    totpQrSvg.value = res.qr_svg
    totpSecretBase32.value = res.secret_base32
    totpPendingToken.value = res.pending_token
    totpSetupStep.value = 2
  } catch (e) {
    totpSetupError.value = e instanceof Error ? e.message : t('settings.setupFailed')
  } finally {
    totpSetupLoading.value = false
  }
}

async function confirmTotpSetup() {
  totpSetupError.value = ''
  totpSetupLoading.value = true
  try {
    const res = await rpcClient.totpSetupConfirm({
      code: totpSetupCode.value,
      password: totpSetupPassword.value,
      pendingToken: totpPendingToken.value,
    })
    totpBackupCodes.value = res.backup_codes
    totpEnabled.value = true
    totpSetupStep.value = 3
  } catch (e) {
    totpSetupError.value = e instanceof Error ? e.message : t('settings.verificationFailed')
  } finally {
    totpSetupLoading.value = false
  }
}

function closeTotpSetup() {
  showTotpSetupModal.value = false
  totpSetupStep.value = 1
  totpSetupPassword.value = ''
  totpSetupCode.value = ''
  totpSetupError.value = ''
  totpQrSvg.value = ''
  totpSecretBase32.value = ''
  totpPendingToken.value = ''
  totpBackupCodes.value = []
  backupCodesCopied.value = false
}

async function disableTotp() {
  totpDisableError.value = ''
  totpDisableLoading.value = true
  try {
    await rpcClient.totpDisable(totpDisablePassword.value, totpDisableCode.value)
    totpEnabled.value = false
    closeTotpDisable()
  } catch (e) {
    totpDisableError.value = e instanceof Error ? e.message : t('settings.disableFailed')
  } finally {
    totpDisableLoading.value = false
  }
}

function closeTotpDisable() {
  showTotpDisableModal.value = false
  totpDisablePassword.value = ''
  totpDisableCode.value = ''
  totpDisableError.value = ''
}

async function copyBackupCodes() {
  const text = totpBackupCodes.value.join('\n')
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  }
  backupCodesCopied.value = true
  setTimeout(() => { backupCodesCopied.value = false }, 2000)
}

loadTotpStatus()
</script>

<template>
  <!-- Two-Factor Authentication -->
  <div class="mb-6">
    <div class="flex items-center justify-between mb-3">
      <div class="flex items-center gap-3">
        <svg class="w-5 h-5 text-white/70" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
        </svg>
        <div>
          <p class="text-sm font-medium text-white/90">{{ t('settings.twoFactorAuth') }}</p>
          <p class="text-xs text-white/50">{{ t('settings.twoFaProtect') }}</p>
        </div>
      </div>
      <span
        class="text-xs font-semibold px-2 py-1 rounded-full"
        :class="totpEnabled ? 'status-success' : 'bg-white/10 text-white/50'"
      >
        {{ totpEnabled ? t('common.enabled') : t('common.disabled') }}
      </span>
    </div>
    <button
      v-if="!totpEnabled"
      @click="showTotpSetupModal = true"
      class="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg glass-button glass-button-warning font-medium"
    >
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
      </svg>
      <span>{{ t('settings.enable2fa') }}</span>
    </button>
    <button
      v-else
      @click="showTotpDisableModal = true"
      class="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg glass-button glass-button-danger font-medium"
    >
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 11V7a4 4 0 118 0m-4 8v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2z" />
      </svg>
      <span>{{ t('settings.disable2fa') }}</span>
    </button>
  </div>

  <!-- TOTP Setup Modal -->
  <Teleport to="body">
    <div
      v-if="showTotpSetupModal"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md"
      @click.self="closeTotpSetup"
      @keydown.escape="closeTotpSetup"
    >
      <div class="glass-card p-6 max-w-md w-full" role="dialog" aria-modal="true" aria-labelledby="totp-setup-title">
        <template v-if="totpSetupStep === 1">
          <h3 id="totp-setup-title" class="text-lg font-semibold text-white mb-2">{{ t('settings.setup2faTitle') }}</h3>
          <p class="text-white/60 text-sm mb-4">{{ t('settings.setup2faPasswordPrompt') }}</p>
          <form @submit.prevent="beginTotpSetup" class="space-y-4">
            <input
              v-model="totpSetupPassword"
              type="password"
              required
              autocomplete="current-password"
              class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
              :placeholder="t('login.enterPasswordPlaceholder')"
            />
            <p v-if="totpSetupError" class="text-sm text-red-400">{{ totpSetupError }}</p>
            <div class="flex gap-3">
              <button
                type="submit"
                :disabled="totpSetupLoading"
                class="flex-1 px-4 py-2 rounded-lg bg-orange-500 text-white font-medium hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                {{ totpSetupLoading ? t('common.loading') : t('common.continue') }}
              </button>
              <button type="button" @click="closeTotpSetup" class="px-4 py-2 rounded-lg bg-white/10 text-white font-medium hover:bg-white/20 transition-colors">{{ t('common.cancel') }}</button>
            </div>
          </form>
        </template>
        <template v-else-if="totpSetupStep === 2">
          <h3 class="text-lg font-semibold text-white mb-2">{{ t('settings.scanQrCode') }}</h3>
          <p class="text-white/60 text-sm mb-4">{{ t('settings.scanQrInstruction') }}</p>
          <div class="flex justify-center mb-4 bg-white rounded-xl p-4 mx-auto w-fit" v-html="sanitizedQrSvg" />
          <div v-if="totpSecretBase32" class="bg-black/30 rounded-lg px-3 py-2 mb-4">
            <p class="text-xs text-white/50 mb-1">Manual entry key (keep secret!):</p>
            <div v-if="showTotpSecret" class="flex items-center gap-2">
              <p class="text-sm font-mono text-orange-400 break-all">{{ totpSecretBase32 }}</p>
              <button type="button" class="glass-button text-xs px-2 py-1" @click="showTotpSecret = false">Hide</button>
            </div>
            <button v-else type="button" class="glass-button text-xs px-3 py-1" @click="showTotpSecret = true">
              Show manual entry key
            </button>
          </div>
          <form @submit.prevent="confirmTotpSetup" class="space-y-4">
            <input
              v-model="totpSetupCode"
              type="text"
              inputmode="numeric"
              pattern="[0-9]{6}"
              maxlength="6"
              required
              autocomplete="one-time-code"
              class="w-full px-3 py-3 rounded-lg bg-white/10 text-white text-center text-2xl tracking-[0.5em] border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500 font-mono"
              :placeholder="t('login.totpPlaceholder')"
            />
            <p v-if="totpSetupError" class="text-sm text-red-400">{{ totpSetupError }}</p>
            <div class="flex gap-3">
              <button
                type="submit"
                :disabled="totpSetupLoading || totpSetupCode.length !== 6"
                class="flex-1 px-4 py-2 rounded-lg bg-orange-500 text-white font-medium hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                {{ totpSetupLoading ? t('login.verifying') : t('settings.verifyAndEnable') }}
              </button>
              <button type="button" @click="closeTotpSetup" class="px-4 py-2 rounded-lg bg-white/10 text-white font-medium hover:bg-white/20 transition-colors">{{ t('common.cancel') }}</button>
            </div>
          </form>
        </template>
        <template v-else-if="totpSetupStep === 3">
          <h3 class="text-lg font-semibold text-white mb-2">{{ t('settings.saveBackupCodes') }}</h3>
          <p class="text-white/60 text-sm mb-4">{{ t('settings.backupCodesInstruction') }}</p>
          <div class="bg-black/30 rounded-xl p-4 mb-4">
            <div class="grid grid-cols-2 gap-2">
              <div
                v-for="(code, i) in totpBackupCodes"
                :key="i"
                class="text-sm font-mono text-white/90 bg-white/5 rounded px-3 py-2 text-center"
              >
                {{ code }}
              </div>
            </div>
          </div>
          <button
            @click="copyBackupCodes"
            class="w-full mb-3 flex items-center justify-center gap-2 px-4 py-2 rounded-lg border border-white/20 text-white/80 font-medium hover:bg-white/5 transition-colors"
          >
            <svg v-if="!backupCodesCopied" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
            <span>{{ backupCodesCopied ? t('common.copiedBang') : t('settings.copyAllCodes') }}</span>
          </button>
          <button
            @click="closeTotpSetup"
            class="w-full px-4 py-2 rounded-lg bg-orange-500 text-white font-medium hover:bg-orange-600 transition-colors"
          >
            {{ t('common.done') }}
          </button>
        </template>
      </div>
    </div>
  </Teleport>

  <!-- TOTP Disable Modal -->
  <Teleport to="body">
    <div
      v-if="showTotpDisableModal"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md"
      @click.self="closeTotpDisable"
      @keydown.escape="closeTotpDisable"
    >
      <div class="glass-card p-6 max-w-md w-full" role="dialog" aria-modal="true" aria-labelledby="totp-disable-title">
        <h3 id="totp-disable-title" class="text-lg font-semibold text-white mb-2">{{ t('settings.disable2faTitle') }}</h3>
        <p class="text-white/60 text-sm mb-4">{{ t('settings.disable2faDesc') }}</p>
        <form @submit.prevent="disableTotp" class="space-y-4">
          <div>
            <label class="block text-sm font-medium text-white/80 mb-2">{{ t('login.password') }}</label>
            <input
              v-model="totpDisablePassword"
              type="password"
              required
              autocomplete="current-password"
              class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
              :placeholder="t('login.enterPasswordPlaceholder')"
            />
          </div>
          <div>
            <label class="block text-sm font-medium text-white/80 mb-2">{{ t('settings.authenticatorCode') }}</label>
            <input
              v-model="totpDisableCode"
              type="text"
              inputmode="numeric"
              pattern="[0-9]{6}"
              maxlength="6"
              required
              autocomplete="one-time-code"
              class="w-full px-3 py-3 rounded-lg bg-white/10 text-white text-center text-2xl tracking-[0.5em] border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500 font-mono"
              :placeholder="t('login.totpPlaceholder')"
            />
          </div>
          <p v-if="totpDisableError" class="text-sm text-red-400">{{ totpDisableError }}</p>
          <div class="flex gap-3">
            <button
              type="submit"
              :disabled="totpDisableLoading"
              class="flex-1 px-4 py-2 rounded-lg bg-red-500 text-white font-medium hover:bg-red-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {{ totpDisableLoading ? t('common.disabling') : t('settings.disable2fa') }}
            </button>
            <button type="button" @click="closeTotpDisable" class="px-4 py-2 rounded-lg bg-white/10 text-white font-medium hover:bg-white/20 transition-colors">{{ t('common.cancel') }}</button>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>
