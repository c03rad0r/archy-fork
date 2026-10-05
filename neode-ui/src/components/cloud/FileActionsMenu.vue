<template>
  <span class="file-actions" @click.stop @keydown.stop>
    <button ref="trigger" type="button" class="file-actions-trigger" :aria-label="`Actions for ${name}`"
      title="File actions" aria-haspopup="dialog" :aria-expanded="open" @click="toggle">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
        <circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/>
      </svg>
    </button>
    <Teleport v-if="open" :to="portalTarget">
      <div class="file-actions-overlay" @click.self.stop="close" @keydown.stop="onKeydown">
        <section ref="panel" class="file-actions-panel" role="dialog" aria-modal="true" :aria-label="`File actions for ${name}`"
          :style="position" @click.stop>
          <p class="file-actions-name" :title="name">{{ name }}</p>
          <template v-if="!confirmDelete">
            <button v-if="canShare" type="button" @click="choose('share')">Share with peers</button>
            <a v-if="downloadHref" :href="downloadHref" :download="name" @click="close">Download</a>
            <button v-if="canDelete" type="button" class="file-actions-danger" @click="confirmDelete = true">Delete…</button>
            <button type="button" @click="close">Close menu</button>
          </template>
          <template v-else>
            <p class="file-actions-warning">Permanently delete this file?</p>
            <button type="button" @click="confirmDelete = false">Keep file</button>
            <button type="button" class="file-actions-danger" @click="choose('delete')">Delete file</button>
          </template>
        </section>
      </div>
    </Teleport>
  </span>
</template>
<script setup lang="ts">
import { ref, nextTick, watch, onUnmounted } from 'vue'
defineProps<{ name: string; downloadHref?: string; canShare?: boolean; canDelete?: boolean }>()
const emit = defineEmits<{ share: []; delete: [] }>()
const trigger = ref<HTMLButtonElement | null>(null)
const panel = ref<HTMLElement | null>(null)
const open = ref(false)
const confirmDelete = ref(false)
const portalTarget = ref<Element | string>('body')
const position = ref<Record<string, string>>({})
function close() { open.value = false; confirmDelete.value = false; trigger.value?.focus() }
async function toggle() {
  if (open.value) { close(); return }
  portalTarget.value = document.fullscreenElement ?? 'body'
  const r = trigger.value!.getBoundingClientRect()
  const width = Math.min(248, window.innerWidth - 24)
  position.value = { width: `${width}px`, left: `${Math.max(12, Math.min(r.right - width, window.innerWidth - width - 12))}px`,
    ...(r.top > window.innerHeight / 2 ? { bottom: `${window.innerHeight - r.top + 8}px` } : { top: `${r.bottom + 8}px` }) }
  open.value = true
  await nextTick()
  const height = panel.value?.getBoundingClientRect().height ?? 0
  const top = Math.max(12, Math.min(r.top > height + 20 ? r.top - height - 8 : r.bottom + 8, window.innerHeight - height - 12))
  position.value = { width: `${width}px`, left: position.value.left!, top: `${top}px` }
  panel.value?.querySelector<HTMLElement>('button, a')?.focus()
}
function choose(action: 'share' | 'delete') { close(); if (action === 'share') emit('share'); else emit('delete') }
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); close() }
  if (event.key !== 'Tab') return
  const controls = Array.from(panel.value?.querySelectorAll<HTMLElement>('button, a') ?? [])
  const current = controls.indexOf(document.activeElement as HTMLElement)
  event.preventDefault()
  controls[(current + (event.shiftKey ? -1 : 1) + controls.length) % controls.length]?.focus()
}
watch(confirmDelete, async () => { await nextTick(); panel.value?.querySelector<HTMLElement>('button, a')?.focus() })
// A viewport change invalidates the anchor; dismiss rather than strand actions offscreen.
watch(open, visible => {
  window[visible ? 'addEventListener' : 'removeEventListener']('resize', close)
})
onUnmounted(() => window.removeEventListener('resize', close))
</script>
<style scoped>
.file-actions { display: inline-flex; flex: none; }
.file-actions-trigger {
  width: 44px; height: 44px; display: inline-flex; align-items: center; justify-content: center;
  border-radius: 50%; color: #fff; background: rgba(16, 20, 24, .5);
  border: 1px solid rgba(255,255,255,.16); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
  cursor: pointer; padding: 0; flex: none;
}
.file-actions-trigger:hover { background: rgba(50,55,60,.7); }
.file-actions-trigger:focus-visible, .file-actions-panel :is(button,a):focus-visible { outline: 2px solid white; outline-offset: 2px; }
.file-actions-overlay { position: fixed; inset: 0; z-index: 5000; background: rgba(0,0,0,.12); }
.file-actions-panel {
  position: fixed; padding: 8px; border-radius: 16px; border: 1px solid rgba(255,255,255,.16);
  background: rgba(20,24,28,.94); color: #fff; backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px);
  box-shadow: 0 12px 40px rgba(0,0,0,.4); max-height: calc(100dvh - 100px); overflow-y: auto;
}
.file-actions-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: #bbb; padding: 8px 12px; }
.file-actions-panel :is(button,a) { display: flex; align-items: center; width: 100%; min-height: 44px; padding: 10px 12px; text-align: left; border-radius: 8px; font-size: 14px; cursor: pointer; text-decoration: none; }
.file-actions-panel :is(button,a):hover { background: rgba(255,255,255,.08); }
.file-actions-panel .file-actions-danger { color: #ffaaa3; }
.file-actions-warning { padding: 8px 12px; font-size: 14px; }
</style>
