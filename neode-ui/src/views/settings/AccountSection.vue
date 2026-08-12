<script setup lang="ts">
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import AccountInfoSection from '@/views/settings/AccountInfoSection.vue'
import ChangePasswordSection from '@/views/settings/ChangePasswordSection.vue'
import TwoFactorSection from '@/views/settings/TwoFactorSection.vue'
import SessionTimeoutSection from '@/views/settings/SessionTimeoutSection.vue'

const router = useRouter()
const { t } = useI18n()
const store = useAppStore()

async function handleLogout() {
  try { await store.logout() } catch (e) { if (import.meta.env.DEV) console.warn('Logout failed, proceeding anyway', e) }
  router.push('/login').catch(() => { window.location.href = '/login' })
}
</script>

<template>
  <!-- Account Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <h2 class="text-xl font-semibold text-white/96 mb-6">{{ t('settings.account') }}</h2>

    <AccountInfoSection />
    <ChangePasswordSection />
    <TwoFactorSection />
    <SessionTimeoutSection />

    <!-- Logout Button -->
    <button
      @click="handleLogout"
      class="w-full path-action-button path-action-button--continue flex items-center justify-center gap-2"
    >
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
      </svg>
      <span>{{ t('settings.logout') }}</span>
    </button>
  </div>
</template>
