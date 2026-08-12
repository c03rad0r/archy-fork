<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
  <!-- System Updates Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h2 class="text-xl font-semibold text-white/96">{{ t('settings.systemUpdates') }}</h2>
        <p class="text-sm text-white/60 mt-1">{{ t('settings.systemUpdatesDesc') }}</p>
      </div>
      <RouterLink
        to="/dashboard/settings/update"
        class="glass-button px-4 py-2 rounded-lg text-sm flex w-full items-center justify-center gap-2 sm:w-auto"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        {{ t('common.manageUpdates') }}
      </RouterLink>
    </div>
  </div>
</template>
