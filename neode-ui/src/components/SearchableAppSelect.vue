<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
const props = defineProps<{ options: { id: string; name: string }[]; disabled?: boolean }>()
const selected = defineModel<string>({ required: true })
const root = ref<HTMLElement>()
const trigger = ref<HTMLButtonElement>()
const search = ref<HTMLInputElement>()
const open = ref(false)
const query = ref('')
const active = ref(0)
const uid = useId()
const filtered = computed(() => props.options.filter(option => `${option.name} ${option.id}`.toLowerCase().includes(query.value.trim().toLowerCase())))
const label = computed(() => props.options.find(option => option.id === selected.value)?.name || 'Choose an app')
watch(query, () => { active.value = 0 })
watch(() => props.disabled, value => { if (value) open.value = false })
watch(() => props.options, options => { if (selected.value && !options.some(option => option.id === selected.value)) selected.value = '' })
function toggle() { open.value = !open.value; query.value = ''; active.value = 0 }
function choose(id: string) { selected.value = id; open.value = false; trigger.value?.focus() }
function outside(event: Event) { if (!root.value?.contains(event.target as Node)) open.value = false }
async function keyboard(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); open.value = false; trigger.value?.focus(); return }
  if (!['ArrowDown', 'ArrowUp', 'Enter'].includes(event.key)) return
  if (!open.value) {
    if (event.key === 'Enter') return // native button click opens it
    event.preventDefault(); toggle(); await nextTick(); search.value?.focus(); return
  }
  if (event.key === 'Enter' && event.target === search.value) { event.preventDefault(); if (filtered.value[active.value]) choose(filtered.value[active.value]!.id); return }
  if (event.key === 'Enter') return
  event.preventDefault()
  active.value = Math.max(0, Math.min(filtered.value.length - 1, active.value + (event.key === 'ArrowDown' ? 1 : -1)))
  search.value?.focus()
  await nextTick()
  document.getElementById(`${uid}-option-${active.value}`)?.scrollIntoView?.({ block: 'nearest' })
}
onMounted(() => { document.addEventListener('pointerdown', outside); document.addEventListener('focusin', outside) })
onBeforeUnmount(() => { document.removeEventListener('pointerdown', outside); document.removeEventListener('focusin', outside) })
</script>

<template>
  <div ref="root" class="min-w-0" @keydown="keyboard">
    <span :id="`${uid}-label`" class="block mb-2">Application</span>
    <button ref="trigger" type="button" class="w-full flex items-center justify-between gap-3 rounded-lg border border-white/15 bg-black/20 px-3 py-3 text-left text-sm text-white/90" :disabled="disabled" :aria-labelledby="`${uid}-label ${uid}-value`" aria-haspopup="listbox" :aria-expanded="open" :aria-controls="`${uid}-list`" @click="toggle">
      <span :id="`${uid}-value`" class="truncate">{{ label }}</span>
      <svg class="w-4 h-4 shrink-0 text-white/55" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
    </button>
    <!-- In normal flow so a walkthrough card, keyboard or WebView cannot clip it. -->
    <div v-if="open" class="glass-card rounded-lg mt-2 p-2 border border-white/10 shadow-xl">
      <input ref="search" v-model="query" type="search" role="combobox" aria-label="Search apps" aria-autocomplete="list" aria-expanded="true" :aria-controls="`${uid}-list`" :aria-activedescendant="filtered.length ? `${uid}-option-${active}` : undefined" autocomplete="off" class="w-full rounded-lg border border-white/15 bg-black/20 px-3 py-3 text-sm text-white mb-2" placeholder="Search apps…" />
      <div :id="`${uid}-list`" role="listbox" aria-label="Supported applications" class="max-h-56 overflow-y-auto overscroll-contain">
        <button v-for="(option, index) in filtered" :id="`${uid}-option-${index}`" :key="option.id" type="button" role="option" :aria-selected="selected === option.id" class="w-full flex items-center justify-between gap-3 rounded-lg px-3 py-3 text-left text-sm text-white/80 hover:bg-white/10 focus-visible:bg-white/10" :class="{ 'bg-white/10 text-white': active === index || selected === option.id }" @click="choose(option.id)">
          <span class="truncate">{{ option.name }}</span><span v-if="selected === option.id" aria-hidden="true" class="text-orange-300">✓</span>
        </button>
      </div>
      <p v-if="!filtered.length" role="status" class="px-3 py-4 text-sm text-white/60">No matching apps. Try another name.</p>
    </div>
  </div>
</template>
