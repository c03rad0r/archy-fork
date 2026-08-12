<template>
  <div class="glass-card p-6 mb-6">
    <div class="flex items-start md:items-center gap-4 md:gap-6">
      <img
        :src="icon"
        :alt="pkg.manifest.title"
        class="app-detail-icon archy-app-icon w-20 h-20 shadow-xl flex-shrink-0"
        @error="handleImageError"
      />

      <div class="flex-1 min-w-0">
        <h1 class="text-xl md:text-2xl font-bold text-white mb-1">{{ pkg.manifest.title }}</h1>
        <p class="text-white/70 text-xs md:text-sm mb-2 line-clamp-2 md:line-clamp-none">{{ pkg.manifest.description.short }}</p>
        <div class="flex flex-wrap items-center gap-2">
          <span
            class="inline-flex items-center px-2 py-0.5 md:px-2.5 md:py-1 rounded-lg text-xs font-medium"
            :class="getStatusClass(pkg.state, pkg.health, pkg['exit-code'])"
          >
            <span class="w-1.5 h-1.5 rounded-full mr-1 md:mr-1.5" :class="getStatusDotClass(pkg.state, pkg.health, pkg['exit-code'])"></span>
            {{ getStatusLabel(pkg.state, pkg.health, pkg['exit-code']) }}
          </span>
          <span class="text-white/50 text-xs">{{ $ver(pkg.manifest.version) }}</span>
        </div>
      </div>

      <div class="app-detail-actions-top items-center gap-2 flex-shrink-0">
        <span
          v-if="pkg.state === 'updating'"
          class="px-4 py-2.5 bg-orange-500/20 border border-orange-500/40 rounded-lg text-orange-200 text-sm font-medium"
        >
          Updating...
        </span>
        <button
          v-for="action in actionItems"
          :key="`top-${action.key}`"
          type="button"
          :disabled="controlsDisabled"
          :class="['app-detail-action-btn px-4 py-2.5 rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed', action.class]"
          @click="emitAction(action.emit)"
        >
          {{ action.label }}
        </button>
      </div>
    </div>

    <div class="app-detail-actions-bottom mt-5 grid grid-cols-2 gap-3">
      <template v-if="pkg.state === 'updating'">
        <span
          class="col-span-2 mobile-card-action bg-orange-500/20 border border-orange-500/40 rounded-lg text-orange-200 text-sm font-medium"
        >
          Updating...
        </span>
      </template>
      <button
        v-for="action in actionItems"
        :key="`bottom-${action.key}`"
        type="button"
        :disabled="controlsDisabled"
        :class="[
          'mobile-card-action rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed',
          action.class,
          actionItems.length === 1 || action.full ? 'col-span-2' : '',
        ]"
        @click="emitAction(action.emit)"
      >
        {{ action.label }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { computed } from 'vue'
import type { PackageDataEntry } from '@/types/api'
import { resolveAppIcon } from '@/views/apps/appsConfig'
import { DEFAULT_APP_ICON } from '@/views/apps/appsConfig'
import { getStatusClass, getStatusDotClass, getStatusLabel } from './appDetailsData'
import { displayVersion } from '@/utils/version'

const { t } = useI18n()

const props = defineProps<{
  pkg: PackageDataEntry
  appId: string
  packageKey: string
  canLaunch: boolean
  isWebOnly: boolean
  pendingAction: 'start' | 'stop' | 'restart' | 'update' | 'uninstall' | null
}>()

const icon = computed(() => resolveAppIcon(props.pkg.manifest?.id || props.appId, props.pkg))
const controlsDisabled = computed(() => props.pendingAction !== null || props.pkg.state === 'updating')

const emit = defineEmits<{
  launch: []
  start: []
  stop: []
  restart: []
  uninstall: []
  update: []
  channels: []
}>()

type ActionEmit = 'launch' | 'start' | 'stop' | 'restart' | 'uninstall' | 'update' | 'channels'

const actionItems = computed(() => {
  const actions: Array<{ key: string; emit: ActionEmit; label: string; class: string; full?: boolean }> = []

  if (props.pkg['available-update'] && props.pkg.state !== 'updating') {
    actions.push({
      key: 'update',
      emit: 'update',
      label: props.pendingAction === 'update' ? 'Updating...' : `Update to ${displayVersion(props.pkg['available-update'])}`,
      class: 'bg-orange-500/20 border border-orange-500/40 text-orange-200 hover:bg-orange-500/30',
      full: true,
    })
  }

  if (props.packageKey === 'lnd') {
    actions.push({
      key: 'channels',
      emit: 'channels',
      label: t('appDetails.channels'),
      class: 'glass-button',
    })
  }

  if (props.canLaunch) {
    actions.push({
      key: 'launch',
      emit: 'launch',
      label: t('common.launch'),
      class: 'glass-button font-semibold',
    })
  }

  if (!props.isWebOnly) {
    if (props.pkg.state === 'stopped' || props.pkg.state === 'exited') {
      actions.push({
        key: 'start',
        emit: 'start',
        label: props.pendingAction === 'start' ? 'Starting...' : props.pkg.state === 'exited' ? 'Restart' : t('common.start'),
        class: props.pkg.state === 'exited' ? 'glass-button glass-button-danger' : 'glass-button glass-button-success',
      })
    }

    if (props.pkg.state === 'running') {
      actions.push({
        key: 'stop',
        emit: 'stop',
        label: props.pendingAction === 'stop' ? 'Stopping...' : t('common.stop'),
        class: 'glass-button text-yellow-200 border-yellow-500/30 hover:bg-yellow-500/10',
      })
    }

    actions.push({
      key: 'restart',
      emit: 'restart',
      label: props.pendingAction === 'restart' ? 'Restarting...' : t('common.restart'),
      class: 'glass-button',
    })

    actions.push({
      key: 'uninstall',
      emit: 'uninstall',
      label: props.pendingAction === 'uninstall' ? 'Uninstalling...' : t('common.uninstall'),
      class: 'glass-button text-red-300 border-red-500/30 hover:bg-red-500/10',
    })
  }

  return actions
})

function emitAction(action: ActionEmit) {
  switch (action) {
    case 'launch':
      emit('launch')
      break
    case 'start':
      emit('start')
      break
    case 'stop':
      emit('stop')
      break
    case 'restart':
      emit('restart')
      break
    case 'uninstall':
      emit('uninstall')
      break
    case 'update':
      emit('update')
      break
    case 'channels':
      emit('channels')
      break
  }
}

function handleImageError(e: Event) {
  const target = e.target as HTMLImageElement
  if (!target.src.includes(DEFAULT_APP_ICON)) {
    target.src = DEFAULT_APP_ICON
    target.dataset.defaultIcon = '1'
  }
}
</script>

<style scoped>
.app-detail-actions-top {
  display: none;
}

.app-detail-actions-bottom {
  display: grid;
}

.app-detail-action-btn {
  min-height: 44px;
  white-space: nowrap;
}

@media (min-width: 1280px) {
  .app-detail-actions-top {
    display: flex;
  }

  .app-detail-actions-bottom {
    display: none;
  }
}
</style>
