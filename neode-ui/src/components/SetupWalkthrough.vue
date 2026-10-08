<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'

export interface SetupStep { id: string; title: string; description: string; complete: boolean }
const props = defineProps<{ steps: SetupStep[]; busy?: boolean; continueLabel?: string; continueDisabled?: boolean }>()
const emit = defineEmits<{ next: [step: string] }>()
const active = defineModel<string>({ required: true })
const root = ref<HTMLElement>()
const index = computed(() => Math.max(0, props.steps.findIndex(step => step.id === active.value)))
const completed = computed(() => props.steps.filter(step => step.complete).length)
watch(() => props.steps, steps => {
  if (!steps.some(step => step.id === active.value)) active.value = steps.find(step => !step.complete)?.id ?? steps[0]?.id ?? ''
})
watch(active, async () => {
  await nextTick()
  const heading = root.value?.querySelector<HTMLButtonElement>('button[aria-expanded="true"]')
  heading?.focus({ preventScroll: true })
  heading?.scrollIntoView?.({ block: 'nearest', behavior: window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' })
})
function move(offset: number) {
  const step = props.steps[index.value + offset]
  if (step) active.value = step.id
}
</script>

<template>
  <div ref="root">
    <div class="mb-8">
      <div class="flex items-center justify-between mb-2">
        <span class="text-sm text-white/60">Step {{ index + 1 }} of {{ steps.length }}</span>
        <span class="goal-status-badge goal-status-badge-in-progress">In progress</span>
      </div>
      <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden" role="progressbar" aria-label="Saved setup steps" :aria-valuenow="completed" :aria-valuemax="steps.length" aria-valuemin="0">
        <div class="h-full rounded-full bg-orange-400 transition-all duration-500 ease-out" :style="{ width: `${(completed / steps.length) * 100}%` }" />
      </div>
    </div>
    <div class="space-y-3">
      <section v-for="(step, stepIndex) in steps" :key="step.id" class="glass-card p-0 overflow-hidden">
        <div class="goal-step" :class="{ 'goal-step-completed': step.complete, 'goal-step-active': active === step.id, 'goal-step-pending': stepIndex > index && !step.complete }">
          <button type="button" class="flex items-start gap-4 w-full text-left" :disabled="busy" :aria-expanded="active === step.id" :aria-controls="`setup-step-${step.id}`" @click="active = step.id">
            <span class="mt-0.5 shrink-0 w-6 h-6 rounded-full flex items-center justify-center" :class="step.complete ? 'bg-green-500/20' : 'bg-white/10'">
              <svg v-if="step.complete" class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
              <span v-else class="text-xs text-white/40 font-medium">{{ stepIndex + 1 }}</span>
            </span>
            <span class="flex-1 min-w-0"><span class="block text-base font-semibold text-white/90 mb-1">{{ step.title }}</span><span class="block text-sm text-white/55 leading-relaxed">{{ step.description }}</span></span>
          </button>
          <div v-if="active === step.id" :id="`setup-step-${step.id}`" class="mt-5 sm:ml-10">
            <fieldset :disabled="busy" class="space-y-6 min-w-0"><slot :name="step.id" /></fieldset>
            <div class="flex flex-wrap justify-between gap-3 mt-6 pt-4 border-t border-white/10">
              <button v-if="stepIndex > 0" type="button" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="move(-1)">Back</button>
              <button v-if="stepIndex < steps.length - 1" type="button" class="path-action-button path-action-button--continue ml-auto" :disabled="busy || continueDisabled" @click="emit('next', steps[stepIndex + 1]!.id)">{{ continueLabel || (step.id === 'storage' ? 'Continue without installing' : 'Continue') }}<span aria-hidden="true">→</span></button>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.goal-step-pending { opacity: .72; }
button:focus-visible { outline: 2px solid #fb923c; outline-offset: 4px; }
@media (prefers-reduced-motion: reduce) { .transition-all { transition: none; } }
</style>
