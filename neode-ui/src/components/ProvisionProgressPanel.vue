<script setup lang="ts">
import { computed, ref } from 'vue'

interface ProvisionStep {
  id: string
  label: string
  status: string
  detail?: string
}

interface ProvisionProgress {
  provision_id: string
  kind: string
  phase: string
  percent: number
  current: string
  steps: ProvisionStep[]
  done: boolean
  failed: boolean
  error?: string
  host?: string
}

const props = defineProps<{ progress: ProvisionProgress }>()
const emit = defineEmits<{ retry: [] }>()

const expanded = ref(false)
const completed = computed(
  () =>
    props.progress.steps.filter(
      step => step.status === 'done' || step.status === 'skipped',
    ).length,
)
const displayPercent = computed(() =>
  props.progress.done ? 100 : props.progress.percent,
)

function stepIcon(status: string): string {
  return (
    {
      done: '✓',
      running: '●',
      skipped: '–',
      failed: '!',
      pending: '○',
    }[status] ?? '○'
  )
}

function stepClass(status: string): string {
  return status === 'done'
    ? 'text-green-400'
    : status === 'running'
      ? 'text-yellow-300 animate-pulse'
      : status === 'failed'
        ? 'text-red-400'
        : 'text-white/30'
}

function effectiveStatus(status: string): string {
  // The installer ends in `pending` when the router moves to its management
  // network; Archy's post-config then completes the install. Show those steps
  // as done once the overall job is done.
  if (props.progress.done && (status === 'pending' || status === 'running')) {
    return 'done'
  }
  return status
}

function displayDetail(step: ProvisionStep): string | undefined {
  if (
    props.progress.done &&
    step.status === 'skipped' &&
    step.detail?.includes('not reached')
  ) {
    return 'Handed off to Archy'
  }
  if (props.progress.done && step.id === 'install') {
    return 'Installed; Archy completed the post-install steps'
  }
  return step.detail
}
</script>

<template>
  <div
    class="mb-4 rounded-lg border border-white/10 bg-white/[0.02] p-3"
    data-testid="provision-progress"
  >
    <template v-if="progress.done && !progress.failed">
      <div class="flex items-center justify-between gap-3">
        <div class="text-sm text-green-300">
          {{
            progress.kind === 'apply'
              ? 'Configuration applied'
              : 'TollGate install complete'
          }}
        </div>
        <div class="flex items-center gap-2">
          <span class="text-xs text-white/40">
            {{ progress.steps.length }} steps
          </span>
          <button
            type="button"
            class="text-xs text-white/40 hover:text-white"
            data-testid="provision-details"
            @click="expanded = !expanded"
          >
            {{ expanded ? 'Hide details' : 'Details' }}
          </button>
        </div>
      </div>
      <p class="mt-1 text-[11px] text-white/40">
        Post-install steps were handed off to Archy and completed.
      </p>
    </template>

    <template v-else>
      <div class="flex items-start justify-between gap-3">
        <div class="min-w-0">
          <div class="text-xs text-white/50">
            {{
              progress.kind === 'apply'
                ? 'Applying Archy configuration'
                : 'Installing TollGate'
            }}
            <span v-if="!progress.done && !progress.failed">
              · step {{ completed }} of {{ progress.steps.length }}
            </span>
          </div>
          <div
            class="text-sm text-white/80 truncate"
            data-testid="provision-current"
          >
            {{ progress.current || 'Starting…' }}
          </div>
        </div>
        <div
          class="text-sm font-mono shrink-0"
          :class="
            progress.failed
              ? 'text-red-300'
              : progress.done
                ? 'text-green-300'
                : 'text-white/80'
          "
          data-testid="provision-percent"
        >
          {{ displayPercent }}%
        </div>
      </div>
      <div class="mt-2 h-1.5 w-full rounded bg-white/10 overflow-hidden">
        <div
          class="h-full rounded bg-white/60 transition-all"
          :style="{ width: `${displayPercent}%` }"
          data-testid="provision-bar"
        ></div>
      </div>
    </template>

    <ul
      v-if="!progress.done || progress.failed || expanded"
      class="mt-2 space-y-0.5"
    >
      <li
        v-for="step in progress.steps"
        :key="step.id"
        class="flex items-start gap-2 text-[11px]"
        :data-testid="`provision-step-${step.id}`"
      >
        <span
          class="w-3 shrink-0 text-center"
          :class="stepClass(effectiveStatus(step.status))"
        >
          {{ stepIcon(effectiveStatus(step.status)) }}
        </span>
        <span :class="stepClass(effectiveStatus(step.status))">
          {{ step.label }}
        </span>
        <span v-if="displayDetail(step)" class="text-white/30 truncate">
          · {{ displayDetail(step) }}
        </span>
      </li>
    </ul>

    <p v-if="progress.error" class="mt-2 text-xs text-red-400">
      {{ progress.error }}
    </p>
    <button
      v-if="progress.failed"
      class="glass-button glass-button-warning text-xs px-3 py-1.5 mt-2"
      data-testid="provision-retry"
      @click="emit('retry')"
    >
      Retry
    </button>
  </div>
</template>
