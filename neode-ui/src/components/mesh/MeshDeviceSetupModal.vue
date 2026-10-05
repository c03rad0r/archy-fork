<template>
  <BaseModal
    :show="show"
    :title="step === 1 ? 'Mesh Radio Detected' : step === 2 ? 'Set Recommended' : 'Flash Firmware'"
    max-width="max-w-lg"
    content-class="max-h-[90vh] overflow-y-auto"
    @close="dismiss"
  >
    <!-- Step 1: detection graphic + what's currently on the radio -->
    <div v-if="step === 1" class="text-center">
      <div class="mx-auto my-2 w-40 h-40 relative">
        <!-- pulsing signal waves behind the board image -->
        <svg viewBox="0 0 160 160" class="absolute inset-0 w-full h-full" aria-hidden="true">
          <g class="mesh-detect-waves" stroke="rgba(251,146,60,0.5)" fill="none" stroke-width="2.5" stroke-linecap="round">
            <path d="M130 44 a30 30 0 0 1 0 28" />
            <path d="M138 36 a42 42 0 0 1 0 44" />
            <path d="M146 28 a54 54 0 0 1 0 60" />
          </g>
        </svg>
        <!-- actual board image (vendored from the Meshtastic web flasher) -->
        <img
          :src="deviceImage.image"
          :alt="deviceImage.label"
          class="relative w-full h-full object-contain drop-shadow-[0_8px_24px_rgba(251,146,60,0.25)]"
          @error="imageFailed = true"
          v-if="!imageFailed"
        />
        <div v-else class="relative w-full h-full flex items-center justify-center text-5xl">📡</div>
      </div>
      <p class="text-white text-base font-medium">{{ deviceImage.label }}</p>
      <p class="text-white/60 text-xs mt-1">
        {{ deviceImage.exact ? 'Detected' : 'Detected LoRa radio' }} on
        <span class="font-mono text-orange-300">{{ devicePath }}</span>
      </p>

      <!-- What's currently flashed / configured on it -->
      <div class="mt-4 text-left rounded-xl bg-white/[0.05] border border-white/10 p-3">
        <div v-if="probing" class="py-1">
          <div class="flex items-center justify-between text-white/60 text-sm mb-1.5">
            <span>{{ probeStage }}</span>
            <span class="text-white/40 text-xs tabular-nums">{{ Math.round(probeProgress) }}%</span>
          </div>
          <div class="h-1.5 rounded-full bg-white/10 overflow-hidden">
            <div
              class="h-full rounded-full bg-orange-400/80 transition-[width] duration-500 ease-linear"
              :style="{ width: probeProgress + '%' }"
            ></div>
          </div>
        </div>
        <template v-else-if="probe">
          <div class="flex items-center gap-2">
            <span class="px-2 py-0.5 rounded-md text-[11px] font-semibold"
              :class="{
                'bg-emerald-400/15 text-emerald-300': probe.kind === 'meshcore',
                'bg-sky-400/15 text-sky-300': probe.kind === 'meshtastic',
                'bg-violet-400/15 text-violet-300': probe.kind === 'reticulum',
              }">{{ kindLabel }}</span>
            <span v-if="probe.firmware_version" class="text-white/50 text-xs truncate">{{ probe.firmware_version }}</span>
          </div>
          <dl class="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
            <template v-if="probe.advert_name">
              <dt class="text-white/40">Name</dt><dd class="text-white/80">{{ probe.advert_name }}</dd>
            </template>
            <template v-if="probe.node_id != null">
              <dt class="text-white/40">Node ID</dt><dd class="text-white/80 font-mono">{{ probe.node_id }}</dd>
            </template>
            <template v-if="probe.region">
              <dt class="text-white/40">Region</dt><dd class="text-white/80">{{ probe.region }}</dd>
            </template>
            <template v-if="probe.modem_preset">
              <dt class="text-white/40">Preset</dt><dd class="text-white/80">{{ probe.modem_preset }}</dd>
            </template>
            <template v-if="probe.primary_channel">
              <dt class="text-white/40">Channel</dt><dd class="text-white/80">{{ probe.primary_channel }}</dd>
            </template>
            <template v-if="probe.secondary_channel">
              <dt class="text-white/40">2nd channel</dt><dd class="text-white/80">{{ probe.secondary_channel }}</dd>
            </template>
          </dl>
        </template>
        <p v-else class="text-white/50 text-xs py-1">
          Couldn't identify the firmware on this radio{{ probeError ? ` (${probeError})` : '' }} —
          you can still set it up (auto-detect will keep trying) or use it as-is.
        </p>
      </div>

      <!-- INTEGRATION POINT (other developer, in progress): the web flasher
           for all three firmwares (MeshCore / Meshtastic / RNode) belongs
           HERE as a third action on this screen — e.g. "Flash different
           firmware" — driven by the probe result above. Per the operator:
           the flasher must live in this detection UI. -->
      <div class="flex gap-2 mt-5">
        <button
          class="flex-1 glass-button px-4 py-2 rounded-lg text-sm disabled:opacity-50"
          :disabled="!!connecting"
          @click="keepAsIs"
        >
          {{ connecting === 'keep' ? 'Connecting…' : 'Keep As Is' }}
        </button>
        <button
          class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
          :disabled="!!connecting"
          @click="step = 2"
        >
          Set Recommended
        </button>
      </div>
      <p class="text-white/40 text-[11px] mt-3">
        "Keep As Is" uses the radio exactly as it is — nothing on it is changed,
        and you can hot-swap radios any time.
      </p>
      <button
        class="w-full text-center text-white/40 hover:text-white/70 text-[11px] mt-3 underline underline-offset-2"
        :disabled="!!connecting"
        @click="openFlashStep"
      >
        Flash Firmware…
      </button>
      <p v-if="error" class="text-xs text-red-400 mt-2">{{ error }}</p>
    </div>

    <!-- Step 3: erase + reflash — destructive, opt-in only -->
    <div v-else-if="step === 'flash'">
      <!-- Once a job exists (started via startFlash), ALWAYS show the
           progress/result view below — including on failure. The old
           condition (`!active && stage !== 'done'`) was also true for a
           FAILED job (active:false, stage:'failed'), which silently sent
           the user back to this picker instead of showing the error. -->
      <template v-if="!flashJob">
        <p class="text-white/60 text-xs mb-3">
          Downloads the latest firmware from upstream and writes it to
          <span class="font-mono text-orange-300">{{ devicePath }}</span>.
        </p>

        <div class="space-y-4">
          <div>
            <label class="block text-sm text-white/80 mb-1">Firmware family</label>
            <select v-model="flashFamily" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option value="">Choose…</option>
              <option value="meshcore">MeshCore</option>
              <option value="meshtastic">Meshtastic</option>
              <option value="reticulum">Reticulum RNode</option>
            </select>
          </div>
          <div>
            <label class="block text-sm text-white/80 mb-1">Board</label>
            <select v-model="flashBoard" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option value="">Choose…</option>
              <option value="heltec-v3">Heltec LoRa 32 V3</option>
              <option value="heltec-v4">Heltec LoRa 32 V4</option>
            </select>
            <p class="text-[11px] text-amber-400/80 mt-1">
              Select the model printed on your board. USB adapters are shared across models.
            </p>
          </div>
        </div>

        <div class="rounded-xl bg-red-500/10 border border-red-500/30 p-3 mt-4">
          <label class="flex items-start gap-2 text-xs text-red-300">
            <input type="checkbox" v-model="flashConfirmed" class="mt-0.5" />
            <span>
              This <strong>erases the entire chip</strong>, including any existing
              keys, identity, and contacts. This cannot be undone.
            </span>
          </label>
        </div>

        <p v-if="error" class="text-xs text-red-400 mt-3">{{ error }}</p>

        <div class="flex gap-2 mt-6">
          <button class="glass-button px-4 py-2 rounded-lg text-sm" @click="step = 1">Back</button>
          <button
            class="flex-1 glass-button px-4 py-2 rounded-lg text-sm font-medium bg-red-500/80 hover:bg-red-500 text-white disabled:opacity-50"
            :disabled="!flashFamily || !flashBoard || !flashConfirmed || starting"
            @click="startFlash"
          >
            {{ starting ? 'Starting…' : 'Erase & Flash Now' }}
          </button>
        </div>
      </template>

      <!-- Progress -->
      <template v-else>
        <div class="text-center py-2">
          <p class="text-white text-sm font-medium">{{ flashStageLabel }}</p>
          <div class="mt-3 h-2 rounded-full bg-white/10 overflow-hidden">
            <div
              class="h-full bg-orange-400 transition-all"
              :style="{ width: (flashJob?.percent ?? (flashJob?.stage === 'done' ? 100 : 8)) + '%' }"
            ></div>
          </div>
          <p v-if="flashJob?.error" class="text-xs text-red-400 mt-3">{{ flashJob.error }}</p>
        </div>
        <div class="mt-3 rounded-xl bg-black/30 border border-white/10 p-2 h-32 overflow-y-auto font-mono text-[10px] text-white/50 leading-relaxed">
          <div v-for="(line, i) in (flashJob?.log_tail ?? []).slice(-40)" :key="i">{{ line }}</div>
        </div>
        <div class="flex gap-2 mt-4">
          <button
            v-if="flashJob?.stage === 'downloading' && flashJob?.active"
            class="glass-button px-4 py-2 rounded-lg text-sm"
            @click="cancelFlash"
          >
            Cancel
          </button>
          <button
            v-if="!flashJob?.active"
            class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium"
            @click="closeFlashStep"
          >
            Done
          </button>
        </div>
      </template>
    </div>

    <!-- Step 2: our latest parameters, shown before anything is written -->
    <div v-else>
      <p class="text-white/60 text-xs mb-3">
        These are the recommended Archipelago settings — nothing is written to
        the radio until you confirm.
      </p>

      <!-- Summary of what will be applied -->
      <div class="rounded-xl bg-white/[0.05] border border-white/10 p-3 mb-4">
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt class="text-white/40">Channel</dt>
          <dd class="text-white/80">{{ form.channel || 'archipelago' }} <span class="text-white/40">(+ public default kept for interop)</span></dd>
          <dt class="text-white/40">Name</dt>
          <dd class="text-white/80">{{ form.name || '(radio keeps its name)' }}</dd>
          <dt class="text-white/40">Region</dt>
          <dd class="text-white/80">{{ form.region || "(radio keeps its region)" }}</dd>
          <template v-if="effectiveKind === 'meshcore' && rfPreset">
            <dt class="text-white/40">RF params</dt>
            <dd class="text-white/80">{{ rfPreset.freqMhz }} MHz · {{ rfPreset.bwKhz }} kHz · SF{{ rfPreset.sf }} · CR4/{{ rfPreset.cr }}</dd>
          </template>
        </dl>
      </div>

      <div class="space-y-4">
        <!-- Region — the meaning adapts to the firmware: Meshtastic takes a
             region enum directly; MeshCore/RNode take raw RF params owned by
             the firmware/daemon, so the region only drives displayed guidance. -->
        <div>
          <label class="block text-sm text-white/80 mb-1">LoRa region / frequency plan</label>
          <select v-model="form.region" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
            <option value="">Keep the radio's current region</option>
            <option v-for="r in LORA_REGIONS" :key="r.code" :value="r.code">{{ r.label }}</option>
          </select>
          <p v-if="suggestedRegion && form.region === suggestedRegion.code" class="text-[11px] text-green-400/80 mt-1">
            Suggested from your node's location
          </p>
          <p v-if="selectedRegion && selectedRegion.dutyCyclePct < 100" class="text-[11px] text-amber-400/80 mt-1">
            {{ selectedRegion.code }} limits airtime to {{ selectedRegion.dutyCyclePct }}% duty cycle ({{ selectedRegion.band }} MHz) — the radio enforces this automatically.
          </p>
          <p v-if="effectiveKind === 'meshcore' && rfPreset" class="text-[11px] text-sky-300/80 mt-1">
            MeshCore RF params for {{ selectedRegion?.code }} are applied to the radio automatically on connect.
          </p>
          <p v-if="effectiveKind === 'reticulum' && rnodePlan" class="text-[11px] text-sky-300/80 mt-1">
            RNode plan for {{ form.region }}: {{ (rnodePlan.frequency / 1e6).toFixed(4) }} MHz, {{ rnodePlan.bandwidth / 1000 }} kHz, SF{{ rnodePlan.spreading_factor }}, CR4/{{ rnodePlan.coding_rate }}, {{ rnodePlan.txpower }} dBm — applied on connect, and the radio confirms it.
          </p>
          <p v-else-if="effectiveKind === 'reticulum'" class="text-[11px] text-sky-300/80 mt-1">
            Pick a region to apply its recommended RNode RF plan on connect — editable any time in Mesh → Device settings.
          </p>
        </div>

        <!-- Node name -->
        <div>
          <label class="block text-sm text-white/80 mb-1">Name on the mesh</label>
          <input v-model="form.name" maxlength="24" placeholder="e.g. basement-node"
            class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
        </div>

        <!-- Channel (Meshtastic secondary channel / MeshCore group channel;
             Reticulum has no channel concept — IFAC lives in the daemon config) -->
        <div v-if="effectiveKind !== 'reticulum'">
          <label class="block text-sm text-white/80 mb-1">Channel</label>
          <input v-model="form.channel" maxlength="11"
            class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
          <p class="text-[11px] text-white/40 mt-1">
            Archipelago nodes find each other on the "archipelago" channel; the public default channel stays active for interop.
          </p>
        </div>
      </div>

      <p v-if="error" class="text-xs text-red-400 mt-3">{{ error }}</p>

      <div class="flex gap-2 mt-6">
        <button class="glass-button px-4 py-2 rounded-lg text-sm" @click="step = 1">Back</button>
        <button
          class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
          :disabled="!!connecting"
          @click="applySetup"
        >
          {{ connecting === 'setup' ? 'Applying…' : 'Apply Settings & Connect' }}
        </button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import BaseModal from '@/components/BaseModal.vue'
import { useMeshStore, type MeshDeviceProbe, type MeshConfigureParams, type FlashFirmwareFamily, type FlashBoard, type FlashJobStatus } from '@/stores/mesh'
import { useAppStore } from '@/stores/app'
import { LORA_REGIONS, regionByCode, suggestRegionFromLatLon, meshcorePlanFor, RNODE_REGION_PLANS } from '@/utils/loraRegions'
import { resolveMeshDeviceImage } from '@/utils/meshDeviceImages'

const mesh = useMeshStore()
const appStore = useAppStore()
const router = useRouter()

const step = ref<1 | 2 | 'flash'>(1)
const connecting = ref<false | 'keep' | 'setup'>(false)
const error = ref('')
const probing = ref(false)
const probe = ref<MeshDeviceProbe | null>(null)
const probeError = ref('')

// Time-driven probe progress: the probe RPC is a single opaque call that can
// take ~5-30s (boot settle + up to three firmware handshakes), so the bar
// advances on a clock toward 92% and snaps to 100% when the result lands.
const probeProgress = ref(0)
const probeStage = ref('Waiting for the radio to boot…')
let probeTicker: ReturnType<typeof setInterval> | null = null
function startProbeProgress() {
  stopProbeProgress()
  probeProgress.value = 0
  probeStage.value = 'Waiting for the radio to boot…'
  const startedAt = Date.now()
  probeTicker = setInterval(() => {
    const elapsed = (Date.now() - startedAt) / 1000
    // ~92% at 30s, decelerating — never looks stuck, never lies "done".
    probeProgress.value = Math.min(92, 100 * (1 - Math.exp(-elapsed / 11)))
    if (elapsed >= 4) probeStage.value = 'Detecting firmware…'
    if (elapsed >= 18) probeStage.value = 'Still checking (radios can be slow to answer)…'
  }, 400)
}
function stopProbeProgress(done = false) {
  if (probeTicker) { clearInterval(probeTicker); probeTicker = null }
  if (done) probeProgress.value = 100
}

const devicePath = computed(() => mesh.flashFlowPath ?? mesh.undismissedDetectedDevices[0] ?? '')
const show = computed(() => !!devicePath.value)
const imageFailed = ref(false)
const deviceImage = computed(() =>
  resolveMeshDeviceImage(
    mesh.status?.detected_device_info?.find(d => d.path === devicePath.value)
  )
)

const kindLabel = computed(() => {
  switch (probe.value?.kind) {
    case 'meshcore': return 'MeshCore'
    case 'meshtastic': return 'Meshtastic'
    case 'reticulum': return 'Reticulum RNode'
    default: return ''
  }
})

const suggestedRegion = computed(() => {
  const info = appStore.serverInfo as { lat?: number | null; lon?: number | null } | undefined
  if (info?.lat != null && info?.lon != null) {
    return suggestRegionFromLatLon(info.lat, info.lon)
  }
  return undefined
})

const form = ref({
  region: '',
  name: '',
  channel: 'archipelago',
})

const selectedRegion = computed(() => regionByCode(form.value.region))
/** The chosen region's recommended RNode RF plan (undefined = none chosen). */
const rnodePlan = computed(() =>
  form.value.region ? RNODE_REGION_PLANS[form.value.region] : undefined,
)
// The firmware whose options we surface: the probe result wins, else the
// last connected type, else meshtastic-style (where presets apply).
const effectiveKind = computed(() => {
  if (probe.value) return probe.value.kind
  const t = (mesh.status?.device_type ?? '').toLowerCase()
  return t === 'meshcore' || t === 'meshtastic' || t === 'reticulum' ? t : 'auto'
})
// The node's persisted RF params (e.g. the validated Portugal preset) ARE
// "our latest parameters" — the generic per-region community plan is only
// the fallback for nodes that never configured RF params.
const rfPreset = computed(() => {
  const cfg = mesh.status?.lora_radio_params
  if (cfg) {
    return {
      freqMhz: cfg.freq_khz / 1000,
      bwKhz: cfg.bw_hz / 1000,
      sf: cfg.sf,
      cr: cfg.cr,
    }
  }
  return meshcorePlanFor(form.value.region)
})

// (Re)probe + (re)apply presets each time a new device surfaces the modal
watch([show, devicePath], async ([visible]) => {
  if (!visible) {
    stopFlashPoll()
    return
  }
  if (mesh.flashFlowPath) {
    // Manual "Flash LoRa" entry: jump straight to the flash step. Skip the
    // read-only probe — the port is usually the live session's, and a second
    // opener on the tty corrupts the running connection; status already
    // knows the firmware kind for the connected radio.
    probe.value = null
    probeError.value = ''
    probing.value = false
    stopProbeProgress()
    imageFailed.value = false
    openFlashStep()
    const t = (mesh.status?.device_type ?? '').toLowerCase()
    if (t === 'meshcore' || t === 'meshtastic' || t === 'reticulum') {
      flashFamily.value = t as FlashFirmwareFamily
    }
    return
  }
  step.value = 1
  error.value = ''
  imageFailed.value = false
  form.value.region = suggestedRegion.value?.code ?? mesh.status?.lora_region ?? ''
  form.value.name = mesh.status?.self_advert_name ?? appStore.serverName ?? ''
  form.value.channel = mesh.status?.channel_name || 'archipelago'
  // Read-only probe of what's on the stick — drives the details card.
  probe.value = null
  probeError.value = ''
  probing.value = true
  startProbeProgress()
  const path = devicePath.value
  try {
    const res = await mesh.probeDevice(path)
    if (devicePath.value === path) probe.value = res
  } catch (e) {
    if (devicePath.value === path) {
      probeError.value = e instanceof Error ? e.message : String(e)
    }
  } finally {
    stopProbeProgress(true)
    if (devicePath.value === path) probing.value = false
  }
}, { immediate: false })

function dismiss() {
  if (mesh.flashFlowPath) {
    mesh.closeFlashFlow()
    return
  }
  if (devicePath.value) mesh.dismissDetectedDevice(devicePath.value)
}

/** Use the radio exactly as flashed: no config writes, firmware pinned to
 *  what the probe saw (auto if the probe failed), hot-swappable from here. */
async function keepAsIs() {
  connecting.value = 'keep'
  error.value = ''
  try {
    const path = devicePath.value
    await mesh.configure({
      enabled: true,
      device_path: path,
      device_kind: probe.value?.kind ?? 'auto',
      manage_radio: false,
    })
    mesh.dismissDetectedDevice(path)
    // The Mesh view lives under the dashboard shell — a bare /mesh 404s.
    void router.push('/dashboard/mesh')
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to connect the mesh radio'
  } finally {
    connecting.value = false
  }
}

/** Apply the Archipelago parameters shown on this screen, then connect. */
async function applySetup() {
  connecting.value = 'setup'
  error.value = ''
  try {
    const path = devicePath.value
    const params: MeshConfigureParams = {
      enabled: true,
      device_path: path,
      channel_name: form.value.channel.trim() || 'archipelago',
      ...(form.value.name.trim() ? { advert_name: form.value.name.trim() } : {}),
      ...(form.value.region ? { lora_region: form.value.region } : {}),
      device_kind: probe.value?.kind ?? 'auto',
      manage_radio: true,
    }
    // MeshCore radios get their RF params written directly (freq/bw/sf/cr in
    // firmware units) — same conversion as the Mesh settings panel.
    if (effectiveKind.value === 'meshcore' && rfPreset.value) {
      params.lora_radio_params = {
        freq_khz: Math.round(rfPreset.value.freqMhz * 1000),
        bw_hz: Math.round(rfPreset.value.bwKhz * 1000),
        sf: rfPreset.value.sf,
        cr: rfPreset.value.cr,
      }
    }
    await mesh.configure(params)
    // RNode radios: the region's recommended RF plan is applied through the
    // Reticulum daemon's persisted settings (mesh.rnode-config-apply), the
    // same round-trip the Device panel uses — the radio confirms the values
    // itself after the daemon restarts with them. Best-effort here: a
    // failure must not abort the connect the user just asked for.
    if (effectiveKind.value === 'reticulum' && rnodePlan.value) {
      mesh.suppressDeviceDetect()
      void mesh
        .applyRnodeConfig({ enabled: true, port: null, ...rnodePlan.value })
        .catch(() => {})
    }
    mesh.dismissDetectedDevice(path)
    void router.push('/dashboard/mesh')
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to configure the mesh radio'
  } finally {
    connecting.value = false
  }
}

// ─── Step 3: erase + reflash ─────────────────────────────────────────────
const flashFamily = ref<FlashFirmwareFamily | ''>('')
const flashBoard = ref<FlashBoard | ''>('')
const flashConfirmed = ref(false)
const starting = ref(false)
const flashJob = ref<FlashJobStatus | null>(null)
let flashPollTimer: ReturnType<typeof setInterval> | null = null


const flashStageLabel = computed(() => {
  switch (flashJob.value?.stage) {
    case 'downloading': return 'Downloading firmware…'
    case 'preparing': return 'Preparing radio…'
    case 'erasing': return 'Erasing chip…'
    case 'writing': return 'Writing firmware…'
    case 'autoinstalling': return 'Installing (rnodeconf)…'
    case 'done': return 'Flash complete'
    case 'failed': return 'Flash failed'
    default: return ''
  }
})

function openFlashStep() {
  flashFamily.value = (probe.value?.kind as FlashFirmwareFamily) ?? ''
  flashBoard.value = ''
  flashConfirmed.value = false
  flashJob.value = null
  error.value = ''
  step.value = 'flash'
}

function stopFlashPoll() {
  if (flashPollTimer) {
    clearInterval(flashPollTimer)
    flashPollTimer = null
  }
}

async function pollFlashStatus() {
  try {
    const status = await mesh.flashStatus()
    flashJob.value = status
    if (!status.active) {
      stopFlashPoll()
      if (status.done && !status.error) {
        // Mirrors the unplug/replug hot-swap flow: re-probe so the details
        // card reflects whatever firmware is actually on the board now.
        const path = devicePath.value
        probing.value = true
        try {
          probe.value = await mesh.probeDevice(path)
        } catch {
          probe.value = null
        } finally {
          probing.value = false
        }
      }
    }
  } catch {
    stopFlashPoll()
  }
}

async function startFlash() {
  if (!flashFamily.value || !flashBoard.value || !flashConfirmed.value) return
  starting.value = true
  error.value = ''
  try {
    await mesh.flashDevice(devicePath.value, flashFamily.value, flashBoard.value)
    flashJob.value = { active: true, stage: 'downloading', log_tail: [] }
    stopFlashPoll()
    flashPollTimer = setInterval(pollFlashStatus, 1500)
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to start flashing'
  } finally {
    starting.value = false
  }
}

async function cancelFlash() {
  try {
    await mesh.flashCancel()
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to cancel'
  }
}

onBeforeUnmount(() => {
  stopFlashPoll()
  stopProbeProgress()
})

function closeFlashStep() {
  stopFlashPoll()
  if (mesh.flashFlowPath) {
    // Manual entry has no detection step to go back to — close the modal.
    mesh.closeFlashFlow()
    return
  }
  step.value = 1
}
</script>

<style scoped>
.mesh-detect-waves path {
  animation: mesh-wave-pulse 2.2s ease-out infinite;
  opacity: 0;
}
.mesh-detect-waves path:nth-child(2) { animation-delay: 0.35s; }
.mesh-detect-waves path:nth-child(3) { animation-delay: 0.7s; }
@keyframes mesh-wave-pulse {
  0% { opacity: 0; }
  25% { opacity: 1; }
  100% { opacity: 0; }
}
</style>
