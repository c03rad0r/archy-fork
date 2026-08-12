<template>
  <div class="pb-6">
    <BackButton label="Back to Settings" desktop-margin="mb-6" @click="$router.push('/dashboard/settings')" />
    <div class="mb-6">
      <h1 class="text-3xl font-bold text-white mb-2">App registries</h1>
      <p class="text-white/70">
        Container registries this node pulls app images from. The primary is tried first; if it's
        slow or unreachable, the next one in the list is tried automatically.
      </p>
    </div>

    <!-- Status message -->
    <div
      v-if="statusMessage"
      class="mb-4 p-3 rounded-lg text-sm"
      :class="statusIsError ? 'bg-red-500/20 text-red-300' : 'bg-green-500/20 text-green-300'"
    >
      {{ statusMessage }}
    </div>

    <!-- Registry list -->
    <div class="glass-card p-6 mb-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between mb-2">
        <h2 class="text-lg font-semibold text-white">Registries</h2>
        <button
          type="button"
          class="text-xs px-3 py-1.5 rounded-md bg-white/5 hover:bg-white/10 text-white/70 hover:text-white transition-colors w-full sm:w-auto"
          @click="openAddRegistry"
        >+ Add registry</button>
      </div>
      <p class="text-sm text-white/60 mb-4">
        Registries are tried in priority order on every app install. Changing the primary takes
        effect on the next install — existing containers keep running on whatever image they
        already pulled.
      </p>
      <ul v-if="registries.length" class="space-y-2">
        <li
          v-for="r in sortedRegistries"
          :key="r.url"
          class="p-3 bg-white/5 rounded-lg"
        >
          <div class="flex items-start gap-3">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-0.5 flex-wrap">
                <p class="text-sm font-medium text-white truncate">{{ r.name || r.url }}</p>
                <span
                  v-if="r.priority === 0"
                  class="text-[10px] font-mono px-2 py-0.5 rounded bg-green-500/20 text-green-300"
                >PRIMARY</span>
                <span
                  v-if="!r.tls_verify"
                  class="text-[10px] font-mono px-2 py-0.5 rounded bg-amber-500/20 text-amber-300"
                  title="TLS verification disabled — HTTP or self-signed registry"
                >HTTP</span>
              </div>
              <p class="text-xs text-white/50 font-mono break-all">{{ r.url }}</p>
            </div>
            <div class="shrink-0 flex items-center gap-1">
              <button
                type="button"
                class="w-8 h-8 flex items-center justify-center rounded-md text-white/60 hover:text-white hover:bg-white/10 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="registryTests[r.url]?.testing"
                title="Test reachability"
                @click="testRegistry(r)"
              >
                <svg v-if="registryTests[r.url]?.testing" class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                  <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" stroke-opacity="0.25"></circle>
                  <path fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
                </svg>
                <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </button>
              <button
                v-if="r.priority !== 0"
                type="button"
                class="w-8 h-8 flex items-center justify-center rounded-md text-white/60 hover:text-yellow-300 hover:bg-white/10 transition-colors"
                title="Make primary"
                @click="setPrimary(r.url)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                </svg>
              </button>
              <button
                v-if="registries.length > 1"
                type="button"
                class="w-8 h-8 flex items-center justify-center rounded-md text-white/60 hover:text-red-300 hover:bg-red-400/10 transition-colors"
                title="Remove registry"
                @click="removeRegistry(r.url)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                </svg>
              </button>
            </div>
          </div>
          <div
            v-if="registryTests[r.url] && !registryTests[r.url]?.testing"
            class="mt-2 pt-2 border-t border-white/5 text-xs"
          >
            <span v-if="registryTests[r.url]?.reachable" class="inline-flex items-center gap-1.5 text-green-300">
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
              </svg>
              Reachable (HTTP {{ registryTests[r.url]?.status }})
            </span>
            <span v-else class="inline-flex items-center gap-1.5 text-red-300">
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M6 18L18 6M6 6l12 12" />
              </svg>
              <span class="truncate">{{ registryTests[r.url]?.error || 'Unreachable' }}</span>
            </span>
          </div>
        </li>
      </ul>
    </div>

    <!-- Add-registry modal -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="addingRegistry"
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md"
          @click.self="cancelAddRegistry"
        >
          <div class="glass-card p-6 max-w-md w-full mx-4">
            <h3 class="text-lg font-semibold text-white mb-1">Add app registry</h3>
            <p class="text-sm text-white/60 mb-5">
              The URL should be of the form <span class="font-mono text-white/80">host[:port]/namespace</span>
              — for example <span class="font-mono text-white/80">ghcr.io/myorg</span> or
              <span class="font-mono text-white/80">192.0.2.10:3000/apps</span>. Registries are
              added to the end of the list; use "Make primary" to reorder.
            </p>
            <form class="space-y-3" @submit.prevent="submitRegistry">
              <div>
                <label class="block text-xs text-white/60 mb-1">Name</label>
                <input
                  v-model="registryDraft.name"
                  type="text"
                  placeholder="My private registry"
                  class="w-full px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none"
                />
              </div>
              <div>
                <label class="block text-xs text-white/60 mb-1">Registry URL</label>
                <input
                  v-model="registryDraft.url"
                  type="text"
                  autofocus
                  placeholder="host:port/namespace"
                  class="w-full px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none font-mono"
                />
              </div>
              <label class="flex items-center gap-2 cursor-pointer text-sm text-white/80">
                <input
                  v-model="registryDraft.tls_verify"
                  type="checkbox"
                  class="accent-orange-400"
                />
                Verify TLS certificate (uncheck for HTTP or self-signed)
              </label>
              <div class="flex gap-3 justify-end pt-2">
                <button
                  type="button"
                  @click="cancelAddRegistry"
                  class="glass-button rounded-lg px-4 py-2 text-sm font-medium"
                >Cancel</button>
                <button
                  type="submit"
                  class="glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 disabled:opacity-50 disabled:cursor-not-allowed"
                  :disabled="registrySaving || !registryDraft.url.trim()"
                >{{ registrySaving ? 'Adding…' : 'Add registry' }}</button>
              </div>
            </form>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, reactive } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import BackButton from '@/components/BackButton.vue'

interface Registry {
  url: string
  name: string
  tls_verify: boolean
  enabled: boolean
  priority: number
}

interface RegistryTestState {
  testing?: boolean
  reachable?: boolean
  status?: number | null
  error?: string | null
}

const registries = ref<Registry[]>([])
const sortedRegistries = computed(() =>
  [...registries.value].sort((a, b) => a.priority - b.priority)
)
const registryTests = ref<Record<string, RegistryTestState>>({})
const statusMessage = ref('')
const statusIsError = ref(false)

const addingRegistry = ref(false)
const registrySaving = ref(false)
const registryDraft = reactive({ url: '', name: '', tls_verify: true })

function showStatus(msg: string, isError = false) {
  statusMessage.value = msg
  statusIsError.value = isError
  setTimeout(() => { statusMessage.value = '' }, 8000)
}

async function loadRegistries() {
  try {
    const res = await rpcClient.call<{ registries: Registry[] }>({ method: 'registry.list' })
    registries.value = res.registries
  } catch (e) {
    if (import.meta.env.DEV) console.warn('registry.list failed', e)
  }
}

function openAddRegistry() {
  registryDraft.url = ''
  registryDraft.name = ''
  registryDraft.tls_verify = true
  addingRegistry.value = true
}
function cancelAddRegistry() {
  addingRegistry.value = false
}

async function submitRegistry() {
  const url = registryDraft.url.trim()
  if (!url) return
  registrySaving.value = true
  try {
    const res = await rpcClient.call<{ registries: Registry[] }>({
      method: 'registry.add',
      params: {
        url,
        name: registryDraft.name.trim() || url,
        tls_verify: registryDraft.tls_verify,
      },
    })
    registries.value = res.registries
    addingRegistry.value = false
    showStatus('Registry added.')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    showStatus(`Add registry failed: ${msg}`, true)
  } finally {
    registrySaving.value = false
  }
}

async function removeRegistry(url: string) {
  try {
    const res = await rpcClient.call<{ registries: Registry[] }>({
      method: 'registry.remove',
      params: { url },
    })
    registries.value = res.registries
    showStatus('Registry removed.')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    showStatus(`Remove failed: ${msg}`, true)
  }
}

async function setPrimary(url: string) {
  try {
    const res = await rpcClient.call<{ registries: Registry[] }>({
      method: 'registry.set-primary',
      params: { url },
    })
    registries.value = res.registries
    showStatus('Primary registry updated. Next install will try it first.')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    showStatus(`Set primary failed: ${msg}`, true)
  }
}

async function testRegistry(r: Registry) {
  registryTests.value = { ...registryTests.value, [r.url]: { testing: true } }
  try {
    const res = await rpcClient.call<{
      url: string
      reachable: boolean
      status: number | null
      error?: string | null
    }>({ method: 'registry.test', params: { url: r.url, tls_verify: r.tls_verify } })
    registryTests.value = {
      ...registryTests.value,
      [r.url]: {
        testing: false,
        reachable: res.reachable,
        status: res.status,
        error: res.error ?? null,
      },
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    registryTests.value = {
      ...registryTests.value,
      [r.url]: { testing: false, reachable: false, error: msg },
    }
  }
}

onMounted(() => { void loadRegistries() })
</script>
