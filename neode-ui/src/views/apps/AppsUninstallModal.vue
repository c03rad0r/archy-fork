<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="show"
        class="fixed inset-0 z-[3000] flex items-center justify-center p-4"
        @click="$emit('close')"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-md"></div>
        <div
          ref="modalRef"
          @click.stop
          role="dialog"
          aria-modal="true"
          aria-labelledby="uninstall-dialog-title"
          class="glass-card p-6 max-w-2xl w-full relative z-10"
        >
          <div class="flex items-start gap-4 mb-4">
            <div class="p-3 bg-red-500/20 rounded-lg">
              <svg class="w-6 h-6 text-red-400" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
            </div>
            <div class="flex-1">
              <h3 id="uninstall-dialog-title" class="text-xl font-semibold text-white mb-2">{{ t('apps.uninstallTitle') }}</h3>
              <p class="text-white/70">
                {{ t('apps.uninstallConfirm', { name: appTitle }) }}
              </p>
              <div class="mt-4 rounded-xl border border-amber-400/20 bg-amber-500/10 p-4">
                <label class="flex items-start gap-3 cursor-pointer">
                  <input
                    v-model="deleteAppData"
                    type="checkbox"
                    class="mt-1 h-4 w-4 rounded border-white/30 bg-black/30 text-red-500 focus:ring-red-500 focus:ring-offset-0"
                  />
                  <span class="min-w-0">
                    <span class="block text-sm font-medium text-white">{{ t('apps.deleteAppDataLabel') }}</span>
                    <span class="block text-xs text-white/60 mt-1">{{ t('apps.deleteAppDataHelp') }}</span>
                  </span>
                </label>
              </div>
            </div>
          </div>

          <div class="flex gap-3 justify-end">
            <button
              @click="$emit('close')"
              class="px-4 py-2 glass-button rounded-lg text-sm font-medium"
            >
              {{ t('common.cancel') }}
            </button>
            <button
              @click="$emit('confirm', deleteAppData)"
              :disabled="uninstalling"
              class="px-4 py-2 glass-button glass-button-danger rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
            >
              <svg
                v-if="uninstalling"
                class="animate-spin h-4 w-4"
                aria-hidden="true"
                fill="none"
                viewBox="0 0 24 24"
              >
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              <span>{{ uninstalling ? t('common.uninstalling') : t('common.uninstall') }}</span>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useModalKeyboard } from '@/composables/useModalKeyboard'

const { t } = useI18n()

const props = defineProps<{
  show: boolean
  appTitle: string
  uninstalling: boolean
}>()

const emit = defineEmits<{
  close: []
  confirm: [deleteAppData: boolean]
}>()

const modalRef = ref<HTMLElement | null>(null)
const restoreFocusRef = ref<HTMLElement | null>(null)
const deleteAppData = ref(false)

useModalKeyboard(
  modalRef,
  computed(() => props.show),
  () => emit('close'),
  { restoreFocusRef },
)

watch(
  () => props.show,
  (show) => {
    if (show) {
      deleteAppData.value = false
    }
  },
)
</script>
