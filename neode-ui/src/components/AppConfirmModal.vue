<template>
  <BaseModal
    :show="!!state"
    :title="state?.title || 'Are you sure?'"
    z-index="z-[4000]"
    @close="resolveConfirm(false)"
  >
    <p class="text-white/80 text-sm whitespace-pre-line">{{ state?.message }}</p>
    <template #footer>
      <div class="flex gap-2">
        <button
          type="button"
          class="flex-1 rounded-lg bg-white/10 hover:bg-white/20 text-white text-sm font-medium py-2.5 transition-colors"
          @click="resolveConfirm(false)"
        >
          {{ state?.cancelLabel || t('common.cancel') }}
        </button>
        <button
          type="button"
          class="flex-1 glass-button rounded-lg text-sm font-semibold py-2.5"
          :class="state?.danger ? 'text-orange-400 border-orange-400/30' : ''"
          @click="resolveConfirm(true)"
        >
          {{ state?.confirmLabel || 'Confirm' }}
        </button>
      </div>
    </template>
  </BaseModal>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import BaseModal from '@/components/BaseModal.vue'
import { confirmState as state, resolveConfirm } from '@/composables/useAppConfirm'

const { t } = useI18n()
</script>
