<template>
  <div v-if="request" class="absolute inset-0 z-40 flex items-center justify-center bg-black/70 p-3 backdrop-blur-md sm:p-6">
    <section ref="modal" role="dialog" aria-modal="true" :aria-labelledby="titleId"
      :aria-busy="loading || phase === 'preparing'" class="glass-card flex max-h-[90%] w-full max-w-xl flex-col overflow-hidden rounded-2xl p-4 sm:p-6">
      <header class="flex items-start gap-3">
        <div class="min-w-0 flex-1"><p class="text-xs text-white/50">IndeeHub · Cloud video</p>
          <h2 :id="titleId" class="mt-1 text-xl font-semibold text-white">{{ phase === 'resolve' ? 'Recover your video registration' : phase === 'select' ? 'Choose your project video' : phase === 'signing' ? 'Approve your producer signature' : 'Preparing your video' }}</h2></div>
        <button type="button" aria-label="Close video registration" class="min-h-11 min-w-11 rounded-lg text-white/70 hover:bg-white/10" @click="emit('cancel')">✕</button>
      </header>
      <dl class="my-4 grid grid-cols-2 gap-3 rounded-xl border border-white/10 p-3 text-sm">
        <div class="col-span-2 min-w-0"><dt class="text-white/45">Project</dt><dd class="break-words text-white">{{ request.intent.projectId }}</dd></div>
        <div><dt class="text-white/45">Price</dt><dd class="text-white">{{ request.intent.priceSats }} sats</dd></div>
        <div><dt class="text-white/45">Access after first play</dt><dd class="text-white">{{ duration }}</dd></div>
      </dl>
      <p v-if="error || localError" role="alert" class="mb-3 rounded-lg border border-red-400/30 p-3 text-sm text-red-200">{{ error || localError }}</p>
      <div v-if="phase === 'resolve'" class="space-y-4 py-3 text-sm text-white/70">
        <p>The node will recover this request's completed video and receipt. If it expired before completion, the node will retire the request so you can start another.</p>
        <button type="button" class="glass-button min-h-11 w-full rounded-lg border-orange-400/30 px-4 py-3 text-orange-200" @click="emit('resolve')">Recover or retire expired request</button>
      </div>
      <template v-else-if="phase === 'select'">
        <div class="mb-2 flex items-center gap-2"><button type="button" :disabled="directory === '/' || loading" class="min-h-11 rounded-lg px-3 text-sm text-white/75 disabled:opacity-40" @click="up">Up</button><p class="min-w-0 flex-1 break-all text-xs text-white/50">Cloud{{ directory }}</p><button type="button" :disabled="loading" class="min-h-11 rounded-lg px-3 text-sm text-white/75" @click="load(directory)">Refresh</button></div>
        <div class="min-h-24 overflow-y-auto rounded-xl border border-white/10">
          <p v-if="loading" role="status" class="p-4 text-sm text-white/55">Loading Cloud files…</p>
          <p v-else-if="!visible.length" class="p-4 text-sm text-white/55">No supported videos here. Choose an MP4, WebM or MOV file.</p>
          <button v-for="file in visible" :key="file.path" type="button" :disabled="loading"
            :aria-pressed="!file.isDir && selected?.path === file.path" class="flex min-h-12 w-full items-center gap-3 border-b border-white/5 px-3 py-2 text-left text-sm hover:bg-white/10"
            :class="selected?.path === file.path ? 'bg-orange-400/15 text-orange-200' : 'text-white/80'" @click="choose(file)">
            <span aria-hidden="true">{{ file.isDir ? '▸' : '▷' }}</span><span class="min-w-0 flex-1 break-words">{{ file.name }}</span>
            <span v-if="!file.isDir" class="shrink-0 text-xs text-white/40">{{ formatSize(file.size) }}</span>
          </button>
        </div>
        <p class="my-3 text-xs leading-relaxed text-white/50">The node keeps a fixed copy for this project's rental terms. This step prepares the video; publishing remains a separate action.</p>
        <footer class="mt-auto flex flex-col-reverse gap-2 pt-2 sm:flex-row"><button type="button" class="glass-button min-h-11 flex-1 rounded-lg px-4" @click="emit('cancel')">Cancel</button><button type="button" :disabled="!selected || loading" class="glass-button min-h-11 flex-1 rounded-lg border-orange-400/30 px-4 text-orange-200 disabled:opacity-40" @click="approve">Use this video</button></footer>
      </template>
      <div v-else role="status" class="py-5 text-sm leading-relaxed text-white/65">
        <p class="break-all">{{ request.selection?.relative_path }}</p>
        <p class="mt-3">{{ request.resolution ? (phase === 'signing' ? 'Sign the request to recover its original result or retire it if it expired before completion.' : 'Checking the original operation. Keep this request until its result is confirmed.') : (phase === 'signing' ? 'Sign the exact project, video and terms with your active producer identity. Your signing key stays in its signer.' : 'Creating the fixed video copy and durable registration. You can reopen the same operation if the connection is interrupted.') }}</p>
      </div>
    </section>
  </div>
</template>
<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { fileBrowserClient, type FileBrowserItem } from '@/api/filebrowser-client'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import type { RegistrationRequest, CloudSelection } from '@/composables/useMediaRegistrationBridge'
const props = defineProps<{ request: RegistrationRequest | null; phase: 'select' | 'resolve' | 'signing' | 'preparing'; error: string }>()
const emit = defineEmits<{ approve: [selection: CloudSelection]; cancel: []; resolve: [] }>()
const modal = ref<HTMLElement | null>(null), directory = ref('/'), files = ref<FileBrowserItem[]>([])
const selected = ref<FileBrowserItem | null>(null), loading = ref(false), localError = ref('')
const titleId = `media-registration-${crypto.randomUUID()}`
let generation = 0
const visible = computed(() => files.value.filter(item => item.isDir || /\.(mp4|m4v|webm|mov)$/i.test(item.name)))
const duration = computed(() => { const seconds = props.request?.intent.viewingSeconds ?? 0; return seconds % 3600 === 0 ? `${seconds / 3600} hours` : `${Math.ceil(seconds / 60)} minutes` })
useModalKeyboard(modal, computed(() => Boolean(props.request)), () => emit('cancel'))
async function load(path: string) {
  const run = ++generation; loading.value = true; localError.value = ''; selected.value = null
  try {
    if (!await fileBrowserClient.login()) throw new Error('Cloud files could not be opened. Check your node connection and try again.')
    const result = await fileBrowserClient.listDirectory(path)
    if (run !== generation) return
    files.value = result; directory.value = path
  } catch (error) { if (run === generation) localError.value = error instanceof Error ? error.message : 'Cloud files could not be loaded.' }
  finally { if (run === generation) loading.value = false }
}
function up() { const pieces = directory.value.split('/').filter(Boolean); pieces.pop(); void load('/' + pieces.join('/')) }
function choose(file: FileBrowserItem) { if (file.isDir) void load(file.path); else selected.value = file }
function approve() { if (!selected.value) return; emit('approve', { relative_path: selected.value.path.replace(/^\/+/, ''), payment_methods: ['cashu'] }) }
function formatSize(bytes: number) { return bytes >= 1024 ** 3 ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : `${(bytes / 1024 ** 2).toFixed(1)} MB` }
watch(() => props.request?.intent.requestId, id => { ++generation; files.value = []; selected.value = null; localError.value = ''; if (id && props.phase === 'select') void load('/'); else loading.value = false }, { immediate: true })
</script>
