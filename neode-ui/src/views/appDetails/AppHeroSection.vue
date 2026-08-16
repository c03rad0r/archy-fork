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
          :class="['app-detail-action-btn px-4 py-2.5 rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed inline-flex items-center justify-center gap-2', action.class]"
          @click="emitAction(action.emit)"
        >
          <svg v-if="action.busy" class="w-4 h-4 animate-spin shrink-0" fill="none" viewBox="0 0 24 24" aria-hidden="true">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          <span>{{ action.label }}</span>
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
          'mobile-card-action rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed inline-flex items-center justify-center gap-2',
          action.class,
          actionItems.length === 1 || action.full ? 'col-span-2' : '',
        ]"
        @click="emitAction(action.emit)"
      >
        <svg v-if="action.busy" class="w-4 h-4 animate-spin shrink-0" fill="none" viewBox="0 0 24 24" aria-hidden="true">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <span>{{ action.label }}</span>
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

// The lifecycle RPCs are fire-and-forget: package.restart returns
// {"status":"restarting"} in milliseconds and does the real work in the
// background. Keying the button purely off the RPC promise made
// "Restarting..." flash and vanish while the app was still down, which reads
// as "nothing happened" (or worse, "it's broken"). The backend's own state —
// pushed over the WebSocket — is what actually says when the app is back, so
// the busy state is driven from there, with `pendingAction` covering only the
// gap before the first state push arrives.
const backendBusy = computed(() => {
  const s = props.pkg.state
  if (s === 'starting' || s === 'stopping' || s === 'restarting' || s === 'updating') return true
  // Container is up but the app hasn't passed its health check yet — still
  // not usable, so keep the spinner rather than declaring victory early.
  return s === 'running' && props.pkg.health === 'starting'
})

const isBusy = computed(() => props.pendingAction !== null || backendBusy.value)
const controlsDisabled = computed(() => isBusy.value || props.pkg.state === 'updating')

// What the node is doing right now, for the button label. Prefers the
// operator's just-clicked action so the label is instant, then falls back to
// whatever the backend reports (covers a restart started from another device).
const busyLabel = computed(() => {
  const s = props.pkg.state
  if (props.pendingAction === 'restart' || s === 'restarting') return 'Restarting…'
  if (props.pendingAction === 'start' || s === 'starting') return 'Starting…'
  if (props.pendingAction === 'stop' || s === 'stopping') return 'Stopping…'
  if (props.pendingAction === 'update' || s === 'updating') return 'Updating…'
  if (props.pendingAction === 'uninstall') return 'Uninstalling…'
  if (s === 'running' && props.pkg.health === 'starting') return 'Starting…'
  return ''
})

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
  const actions: Array<{
    key: string
    emit: ActionEmit
    label: string
    class: string
    full?: boolean
    busy?: boolean
  }> = []

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
      // While anything is in flight this button carries the status for the
      // whole card (it is the one action always present), so the operator
      // sees a spinner + "Restarting…" for as long as the node is working.
      label: isBusy.value ? busyLabel.value || t('common.restart') : t('common.restart'),
      busy: isBusy.value,
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
