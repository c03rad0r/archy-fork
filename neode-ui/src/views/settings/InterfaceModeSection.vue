<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { SUPPORTED_LOCALES, setLocale, type SupportedLocale } from '@/i18n'
import { useUIModeStore } from '@/stores/uiMode'
import type { UIMode } from '@/types/api'

const { t, locale } = useI18n()
const uiMode = useUIModeStore()

const interfaceModes = computed<{ id: UIMode; label: string; description: string; iconPaths: string[] }[]>(() => [
  {
    id: 'easy',
    label: t('settings.modeEasy'),
    description: t('settings.modeEasyDesc'),
    iconPaths: ['M14.828 14.828a4 4 0 01-5.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z'],
  },
  {
    id: 'gamer',
    label: t('settings.modePro'),
    description: t('settings.modeProDesc'),
    iconPaths: ['M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z', 'M15 12a3 3 0 11-6 0 3 3 0 016 0z'],
  },
  {
    id: 'chat',
    label: t('settings.modeChat'),
    description: t('settings.modeChatDesc'),
    iconPaths: ['M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z'],
  },
])

const supportedLocales = SUPPORTED_LOCALES
const currentLocale = computed(() => locale.value)

async function changeLocale(code: string) {
  await setLocale(code as SupportedLocale)
}
</script>

<template>
  <!-- Interface Mode Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <h2 class="text-xl font-semibold text-white/96 mb-2">{{ t('settings.interfaceMode') }}</h2>
    <p class="text-sm text-white/60 mb-6">{{ t('settings.interfaceModeDesc') }}</p>
    <div data-controller-container tabindex="0" class="grid grid-cols-1 md:grid-cols-3 gap-4">
      <button
        v-for="m in interfaceModes"
        :key="m.id"
        @click="uiMode.setMode(m.id)"
        class="path-option-card text-left p-5"
        :class="{ 'path-option-card--selected': uiMode.mode === m.id }"
      >
        <div class="flex items-center gap-3 mb-3">
          <svg class="w-6 h-6 text-white/70" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              v-for="(path, index) in m.iconPaths"
              :key="index"
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              :d="path"
            />
          </svg>
          <h3 class="text-lg font-semibold text-white/96">{{ m.label }}</h3>
        </div>
        <p class="text-sm text-white/60 leading-relaxed">{{ m.description }}</p>
      </button>
    </div>
  </div>

  <!-- Language Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <h2 class="text-xl font-semibold text-white/96 mb-2">Language</h2>
    <p class="text-sm text-white/60 mb-4">Choose your preferred language</p>
    <div class="flex gap-3 flex-wrap">
      <button
        v-for="loc in supportedLocales"
        :key="loc.code"
        @click="changeLocale(loc.code)"
        class="glass-button px-4 py-2 rounded-lg text-sm font-medium transition-all"
        :class="currentLocale === loc.code ? 'ring-2 ring-orange-400/60 bg-white/10' : ''"
      >
        <span class="mr-2">{{ loc.flag }}</span>{{ loc.name }}
      </button>
    </div>
  </div>
</template>
