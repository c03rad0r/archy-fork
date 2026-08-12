<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { useModalKeyboard } from '@/composables/useModalKeyboard'

const { t } = useI18n()

const showChangePasswordModal = ref(false)
const changePasswordModalRef = ref<HTMLElement | null>(null)
const changePasswordRestoreFocusRef = ref<HTMLElement | null>(null)
useModalKeyboard(changePasswordModalRef, showChangePasswordModal, closeChangePasswordModal, { restoreFocusRef: changePasswordRestoreFocusRef })
const changingPassword = ref(false)
const changePasswordError = ref('')
const changePasswordSuccess = ref('')
const changePasswordWarning = ref('')
const changePasswordForm = ref({
  currentPassword: '',
  newPassword: '',
  confirmPassword: '',
  alsoChangeSsh: true,
})

function validatePasswordStrength(pw: string): string | null {
  if (pw.length < 12) return t('settings.passwordMinLength')
  if (!/[A-Z]/.test(pw)) return t('settings.passwordNeedUppercase')
  if (!/[a-z]/.test(pw)) return t('settings.passwordNeedLowercase')
  if (!/\d/.test(pw)) return t('settings.passwordNeedDigit')
  if (!/[^A-Za-z0-9]/.test(pw)) return t('settings.passwordNeedSpecial')
  return null
}

async function handleChangePassword() {
  changePasswordError.value = ''
  changePasswordSuccess.value = ''
  changePasswordWarning.value = ''
  const { currentPassword, newPassword, confirmPassword, alsoChangeSsh } = changePasswordForm.value
  if (!currentPassword || !newPassword || !confirmPassword) {
    changePasswordError.value = t('settings.passwordAllFieldsRequired')
    return
  }
  if (newPassword !== confirmPassword) {
    changePasswordError.value = t('settings.passwordMismatch')
    return
  }
  const strengthError = validatePasswordStrength(newPassword)
  if (strengthError) {
    changePasswordError.value = strengthError
    return
  }
  changingPassword.value = true
  try {
    const result = await rpcClient.changePassword({
      currentPassword,
      newPassword,
      alsoChangeSsh,
    })
    changePasswordSuccess.value = t('settings.passwordUpdatedSuccess')
    if (alsoChangeSsh && result.ssh_error) {
      changePasswordWarning.value = `${t('settings.passwordUpdatedSshFailed')} ${result.ssh_error}`
    }
    changePasswordForm.value = { currentPassword: '', newPassword: '', confirmPassword: '', alsoChangeSsh: true }
    if (!changePasswordWarning.value) {
      setTimeout(() => {
        closeChangePasswordModal()
      }, 2000)
    }
  } catch (e) {
    changePasswordError.value = e instanceof Error ? e.message : t('settings.passwordChangeFailed')
  } finally {
    changingPassword.value = false
  }
}

function closeChangePasswordModal() {
  changePasswordRestoreFocusRef.value?.focus?.()
  showChangePasswordModal.value = false
  changePasswordError.value = ''
  changePasswordSuccess.value = ''
  changePasswordWarning.value = ''
  changePasswordForm.value = { currentPassword: '', newPassword: '', confirmPassword: '', alsoChangeSsh: true }
}
</script>

<template>
  <!-- Change Password -->
  <div class="mb-6">
    <button
      ref="changePasswordRestoreFocusRef"
      @click="showChangePasswordModal = true"
      class="w-full flex items-center justify-center gap-2 mb-4 px-4 py-2 rounded-lg glass-button glass-button-warning font-medium"
    >
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
      </svg>
      <span>{{ t('settings.changePassword') }}</span>
    </button>
  </div>

  <!-- Change Password Modal -->
  <Teleport to="body">
    <div
      v-if="showChangePasswordModal"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md"
      @click.self="closeChangePasswordModal()"
    >
      <div ref="changePasswordModalRef" class="glass-card p-6 max-w-md w-full">
        <h3 class="text-lg font-semibold text-white mb-4">{{ t('settings.changePasswordTitle') }}</h3>
        <p class="text-white/70 text-sm mb-4">{{ t('settings.changePasswordDesc') }}</p>
        <form @submit.prevent="handleChangePassword" class="space-y-4">
          <div>
            <label class="block text-sm font-medium text-white/80 mb-2">{{ t('settings.currentPassword') }}</label>
            <input
              v-model="changePasswordForm.currentPassword"
              type="password"
              required
              autocomplete="current-password"
              class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
              :placeholder="t('login.enterPasswordPlaceholder')"
            />
          </div>
          <div>
            <label class="block text-sm font-medium text-white/80 mb-2">{{ t('settings.newPassword') }}</label>
            <input
              v-model="changePasswordForm.newPassword"
              type="password"
              required
              autocomplete="new-password"
              class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
              :placeholder="t('settings.passwordPlaceholder')"
            />
          </div>
          <div>
            <label class="block text-sm font-medium text-white/80 mb-2">{{ t('settings.confirmNewPassword') }}</label>
            <input
              v-model="changePasswordForm.confirmPassword"
              type="password"
              required
              autocomplete="new-password"
              class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
              :placeholder="t('settings.confirmNewPassword')"
            />
          </div>
          <label class="flex items-center gap-2 text-sm text-white/80">
            <input v-model="changePasswordForm.alsoChangeSsh" type="checkbox" class="rounded border-white/30" />
            {{ t('settings.updateSshCheckbox') }}
          </label>
          <p v-if="changePasswordError" class="text-sm text-red-400">{{ changePasswordError }}</p>
          <p v-if="changePasswordSuccess" class="text-sm text-green-400">{{ changePasswordSuccess }}</p>
          <p v-if="changePasswordWarning" class="text-sm text-yellow-300">{{ changePasswordWarning }}</p>
          <div class="flex gap-3 pt-2">
            <button
              type="submit"
              :disabled="changingPassword"
              class="flex-1 px-4 py-2 rounded-lg bg-orange-500 text-white font-medium hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {{ changingPassword ? t('settings.updatingPassword') : t('settings.updatePassword') }}
            </button>
            <button
              type="button"
              @click="closeChangePasswordModal"
              class="px-4 py-2 rounded-lg bg-white/10 text-white font-medium hover:bg-white/20 transition-colors"
            >
              {{ t('common.cancel') }}
            </button>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>
