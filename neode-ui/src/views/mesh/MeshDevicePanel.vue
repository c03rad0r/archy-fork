<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useMeshStore } from '@/stores/mesh'
import { LORA_REGIONS, regionByCode, meshcorePlanFor, MESHCORE_RF_PRESETS, RNODE_REGION_PLANS } from '@/utils/loraRegions'

const mesh = useMeshStore()

const rebooting = ref(false)
const rebootError = ref<string | null>(null)
const rebootMessage = ref<string | null>(null)

async function handleReboot() {
  rebooting.value = true
  rebootError.value = null
  rebootMessage.value = null
  // Same as apply: the radio goes away on purpose for ~15-20s.
  mesh.suppressDeviceDetect()
  try {
    const res = await mesh.rebootRadio()
    // The backend now waits for the device's acknowledgement and says what
    // actually happened — show it instead of silently going idle again.
    rebootMessage.value = res.message || 'Reboot command acknowledged by the radio.'
  } catch (e) {
    rebootError.value = e instanceof Error ? e.message : 'Failed to reboot radio'
  } finally {
    rebooting.value = false
  }
}

// ── RNode (Reticulum) RF settings — full round-trip with device read-back ──

const rnodeForm = ref({
  enabled: true,
  port: '',
  frequency: '',
  bandwidth: '125000',
  spreading_factor: '8',
  coding_rate: '5',
  txpower: '17',
  airtime_limit_short: '',
  airtime_limit_long: '',
})
const rnodeLive = ref<Record<string, unknown> | null>(null)
const rnodeLiveError = ref<string | null>(null)
const rnodeLoading = ref(false)
const rnodeApplying = ref(false)
const rnodeResult = ref<{ ok: boolean; confirmed: boolean; message: string } | null>(null)
let rnodeSeeded = false

const rnodeRegionPlan = computed(() => (form.value.region ? RNODE_REGION_PLANS[form.value.region] : undefined))

function setRnodeRecommendedForRegion() {
  const plan = rnodeRegionPlan.value
  if (!plan) return
  rnodeForm.value.frequency = String(plan.frequency)
  rnodeForm.value.bandwidth = String(plan.bandwidth)
  rnodeForm.value.spreading_factor = String(plan.spreading_factor)
  rnodeForm.value.coding_rate = String(plan.coding_rate)
  rnodeForm.value.txpower = String(plan.txpower)
  rnodeForm.value.airtime_limit_short = plan.airtime_limit_short != null ? String(plan.airtime_limit_short) : ''
  rnodeForm.value.airtime_limit_long = plan.airtime_limit_long != null ? String(plan.airtime_limit_long) : ''
}

async function loadRnodeConfig() {
  rnodeLoading.value = true
  try {
    const res = await mesh.getRnodeConfig()
    rnodeLive.value = res.live
    rnodeLiveError.value = res.live_error
    const s = res.settings as Record<string, unknown>
    if (!rnodeSeeded && s) {
      rnodeSeeded = true
      rnodeForm.value.enabled = s.enabled !== false
      rnodeForm.value.port = (s.port as string) ?? ''
      rnodeForm.value.frequency = String(s.frequency ?? '')
      rnodeForm.value.bandwidth = String(s.bandwidth ?? '125000')
      rnodeForm.value.spreading_factor = String(s.spreading_factor ?? '8')
      rnodeForm.value.coding_rate = String(s.coding_rate ?? '5')
      rnodeForm.value.txpower = String(s.txpower ?? '17')
      rnodeForm.value.airtime_limit_short = s.airtime_limit_short != null ? String(s.airtime_limit_short) : ''
      rnodeForm.value.airtime_limit_long = s.airtime_limit_long != null ? String(s.airtime_limit_long) : ''
    }
  } catch (e) {
    rnodeLiveError.value = e instanceof Error ? e.message : 'Could not load RNode settings'
  } finally {
    rnodeLoading.value = false
  }
}

async function applyRnodeSettings() {
  rnodeApplying.value = true
  rnodeResult.value = null
  // Applying deliberately restarts the radio daemon; without this the
  // "new device detected" modal interrupts the flow mid-apply.
  mesh.suppressDeviceDetect()
  try {
    const res = await mesh.applyRnodeConfig({
      enabled: rnodeForm.value.enabled,
      port: rnodeForm.value.port.trim() || null,
      frequency: Number(rnodeForm.value.frequency),
      bandwidth: Number(rnodeForm.value.bandwidth),
      spreading_factor: Number(rnodeForm.value.spreading_factor),
      coding_rate: Number(rnodeForm.value.coding_rate),
      txpower: Number(rnodeForm.value.txpower),
      airtime_limit_short: rnodeForm.value.airtime_limit_short === '' ? null : Number(rnodeForm.value.airtime_limit_short),
      airtime_limit_long: rnodeForm.value.airtime_limit_long === '' ? null : Number(rnodeForm.value.airtime_limit_long),
    })
    rnodeResult.value = { ok: res.applied, confirmed: !!res.confirmed, message: res.message }
    if (res.live) rnodeLive.value = res.live
  } catch (e) {
    rnodeResult.value = { ok: false, confirmed: false, message: e instanceof Error ? e.message : 'Apply failed' }
  } finally {
    rnodeApplying.value = false
  }
}

async function refreshRnodeLive() {
  rnodeLoading.value = true
  try {
    const res = await mesh.getRnodeConfig()
    rnodeLive.value = res.live
    rnodeLiveError.value = res.live_error
  } catch (e) {
    rnodeLiveError.value = e instanceof Error ? e.message : 'Could not read the radio state'
  } finally {
    rnodeLoading.value = false
  }
}

function fmtMhz(v: unknown): string {
  const n = Number(v)
  return Number.isFinite(n) && n > 0 ? `${(n / 1e6).toFixed(4)} MHz` : '—'
}

// ── Editable settings (persisted via mesh.configure) ──
const form = ref({
  region: '',
  deviceKind: 'auto',
  channel: 'archipelago',
  name: '',
  broadcastIdentity: true,
  // MeshCore LoRa PHY params (human units; converted to firmware units on
  // save). All four empty = leave the radio's flashed settings untouched.
  rfFreqMhz: '',
  rfBwKhz: '',
  rfSf: '',
  rfCr: '',
})
const saving = ref(false)
const saveError = ref<string | null>(null)
const saveDone = ref(false)
let seeded = false

// Seed the form once from status (don't clobber in-progress edits on poll)
watch(
  () => mesh.status,
  (s) => {
    if (!s || seeded) return
    seeded = true
    form.value.region = s.lora_region ?? ''
    form.value.deviceKind = s.device_kind ?? 'auto'
    form.value.channel = s.channel_name || 'archipelago'
    form.value.name = s.self_advert_name ?? ''
    const rp = (s as Record<string, unknown>).lora_radio_params as
      | { freq_khz: number; bw_hz: number; sf: number; cr: number }
      | null
      | undefined
    if (rp) {
      applyingPreset = true
      form.value.rfFreqMhz = String(rp.freq_khz / 1000)
      form.value.rfBwKhz = String(rp.bw_hz / 1000)
      form.value.rfSf = String(rp.sf)
      form.value.rfCr = String(rp.cr)
      // Show the matching named preset if the stored values are one; else Custom.
      const match = MESHCORE_RF_PRESETS.find(
        (p) =>
          Math.round(p.freqMhz * 1000) === rp.freq_khz &&
          Math.round(p.bwKhz * 1000) === rp.bw_hz &&
          p.sf === rp.sf &&
          p.cr === rp.cr,
      )
      rfPreset.value = match ? match.id : 'custom'
      applyingPreset = false
    }
  },
  { immediate: true },
)

// RF preset dropdown: picking a named plan fills the four fields; hand-editing
// any field flips the dropdown to Custom so it never misrepresents the values.
const rfPreset = ref('')
let applyingPreset = false
function selectRfPreset(id: string) {
  rfPreset.value = id
  if (id === 'custom' || id === '') return
  const p = MESHCORE_RF_PRESETS.find((x) => x.id === id)
  if (!p) return
  applyingPreset = true
  form.value.rfFreqMhz = String(p.freqMhz)
  form.value.rfBwKhz = String(p.bwKhz)
  form.value.rfSf = String(p.sf)
  form.value.rfCr = String(p.cr)
  applyingPreset = false
}
watch(
  () => [form.value.rfFreqMhz, form.value.rfBwKhz, form.value.rfSf, form.value.rfCr],
  () => {
    if (!applyingPreset && rfPreset.value && rfPreset.value !== 'custom') rfPreset.value = 'custom'
  },
)

const selectedRegion = computed(() => regionByCode(form.value.region))
const deviceType = computed(() => mesh.status?.device_type ?? 'unknown')
// Firmware whose options apply: explicit pin wins, else the connected type.
const effectiveKind = computed(() => {
  if (form.value.deviceKind !== 'auto') return form.value.deviceKind
  const t = deviceType.value.toLowerCase()
  return t === 'meshcore' || t === 'meshtastic' || t === 'reticulum' ? t : 'auto'
})
const meshcorePlan = computed(() => meshcorePlanFor(form.value.region))

async function saveSettings() {
  saving.value = true
  saveError.value = null
  saveDone.value = false
  try {
    // MeshCore RF params: a named preset sends its plan; Custom sends the
    // four fields (all required — partial input is an error); "Keep the
    // radio's current settings" omits the key so the config is untouched.
    let rfParams: { freq_khz: number; bw_hz: number; sf: number; cr: number } | undefined
    if (rfPreset.value && rfPreset.value !== 'custom') {
      const p = MESHCORE_RF_PRESETS.find((x) => x.id === rfPreset.value)
      if (p) {
        rfParams = {
          freq_khz: Math.round(p.freqMhz * 1000),
          bw_hz: Math.round(p.bwKhz * 1000),
          sf: p.sf,
          cr: p.cr,
        }
      }
    } else if (rfPreset.value === 'custom') {
      const rf = [form.value.rfFreqMhz, form.value.rfBwKhz, form.value.rfSf, form.value.rfCr]
      if (!rf.every((v) => String(v).trim() !== '')) {
        throw new Error('Fill in all four RF fields (frequency, bandwidth, SF, CR)')
      }
      rfParams = {
        freq_khz: Math.round(parseFloat(form.value.rfFreqMhz) * 1000),
        bw_hz: Math.round(parseFloat(form.value.rfBwKhz) * 1000),
        sf: parseInt(form.value.rfSf, 10),
        cr: parseInt(form.value.rfCr, 10),
      }
    }
    await mesh.configure({
      lora_region: form.value.region,
      device_kind: form.value.deviceKind,
      channel_name: form.value.channel.trim() || 'archipelago',
      // Always sent: an empty string CLEARS the custom mesh name (backend
      // maps "" -> None -> fall back to the server name). The old omit-when-
      // empty made clearing impossible once a name was ever set.
      advert_name: form.value.name.trim(),
      broadcast_identity: form.value.broadcastIdentity,
      ...(rfParams ? { lora_radio_params: rfParams } : {}),
    })
    saveDone.value = true
    setTimeout(() => { saveDone.value = false }, 5000)
  } catch (e) {
    saveError.value = e instanceof Error ? e.message : 'Failed to save mesh settings'
  } finally {
    saving.value = false
  }
}

// Load the RNode settings + live state as soon as the panel knows a
// Reticulum radio is (or is pinned as) the device. Declared LAST: with
// `immediate: true` the source getter runs at setup, and `effectiveKind`
// must already exist (the SendBitcoinModal TDZ-crash lesson).
watch(
  () => effectiveKind.value,
  (kind) => {
    if (kind === 'reticulum') void loadRnodeConfig()
  },
  { immediate: true },
)
</script>

<template>
  <div class="glass-card mesh-device-panel">
    <h3 class="mesh-panel-title">Device</h3>
    <p class="mesh-panel-sub">Firmware, identity, and radio controls for the connected mesh device</p>

    <div v-if="mesh.status" class="mesh-device-panel-grid">
      <div class="mesh-stat">
        <span class="mesh-stat-label">Firmware</span>
        <span class="mesh-stat-value">{{ mesh.status.firmware_version ?? '—' }}</span>
      </div>
      <div class="mesh-stat">
        <span class="mesh-stat-label">Node ID</span>
        <span class="mesh-stat-value">{{ mesh.status.self_node_id != null ? `!${mesh.status.self_node_id.toString(16).padStart(8, '0')}` : '—' }}</span>
      </div>
      <div class="mesh-stat">
        <span class="mesh-stat-label">Type</span>
        <span class="mesh-stat-value">{{ deviceType === 'unknown' ? '—' : deviceType }}</span>
      </div>
    </div>

    <!-- Radio settings -->
    <div class="mt-5 pt-4 border-t border-white/10">
      <h4 class="text-sm font-semibold text-white mb-3">Radio Settings</h4>
      <div class="grid gap-3 sm:grid-cols-2">
        <div>
          <label class="block text-xs text-white/60 mb-1">LoRa region / frequency plan</label>
          <select v-model="form.region" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
            <option value="">Keep the radio's current region</option>
            <option v-for="r in LORA_REGIONS" :key="r.code" :value="r.code">{{ r.label }}</option>
          </select>
          <p v-if="selectedRegion && selectedRegion.dutyCyclePct < 100" class="text-[11px] text-amber-400/80 mt-1">
            {{ selectedRegion.code }}: {{ selectedRegion.dutyCyclePct }}% duty-cycle limit, {{ selectedRegion.band }} MHz, max {{ selectedRegion.maxPowerDbm }} dBm
          </p>
          <p v-if="effectiveKind === 'meshtastic' || effectiveKind === 'auto'" class="text-[11px] text-white/40 mt-1">
            Applied to fresh (region-unset) Meshtastic radios; a radio that already has a region keeps it.
          </p>
          <p v-else-if="effectiveKind === 'meshcore' && meshcorePlan" class="text-[11px] text-sky-300/80 mt-1">
            MeshCore community plan for {{ selectedRegion?.code }}: {{ meshcorePlan.freqMhz }} MHz, {{ meshcorePlan.bwKhz }} kHz, SF{{ meshcorePlan.sf }}, CR4/{{ meshcorePlan.cr }} — set the RF fields below to program the radio.
          </p>
          <p v-else-if="effectiveKind === 'meshcore'" class="text-[11px] text-sky-300/80 mt-1">
            Program the radio's RF settings with the fields below — every radio on your mesh must match{{ selectedRegion ? ` (${selectedRegion.band} MHz band)` : '' }}.
          </p>
          <p v-else-if="effectiveKind === 'reticulum'" class="text-[11px] text-sky-300/80 mt-1">
            Pick your region, then use "Set recommended for region" in the RNode section below.
          </p>
        </div>
        <div>
          <label class="block text-xs text-white/60 mb-1">Radio firmware</label>
          <select v-model="form.deviceKind" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
            <option value="auto">Auto-detect (Meshcore → Meshtastic → RNode)</option>
            <option value="meshcore">MeshCore</option>
            <option value="meshtastic">Meshtastic</option>
            <option value="reticulum">Reticulum / RNode</option>
          </select>
          <p class="text-[11px] text-white/40 mt-1">
            Pin the flashed firmware so no other protocol's probe bytes touch the port.
          </p>
        </div>
        <div v-if="effectiveKind !== 'reticulum'">
          <label class="block text-xs text-white/60 mb-1">Channel</label>
          <input v-model="form.channel" maxlength="11" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
        </div>
        <div>
          <label class="block text-xs text-white/60 mb-1">Name on the mesh</label>
          <input v-model="form.name" maxlength="24" placeholder="node name" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
        </div>
      </div>

      <!-- MeshCore LoRa PHY params — every radio on the local mesh must match
           or it hears RF energy but decodes nothing. Empty = leave the radio's
           flashed settings untouched. -->
      <div v-if="effectiveKind === 'meshcore'" class="mt-4">
        <h5 class="text-xs font-semibold text-white/80 mb-2">MeshCore RF parameters</h5>
        <div class="mb-3">
          <label class="block text-xs text-white/60 mb-1">Frequency plan</label>
          <select :value="rfPreset" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" @change="selectRfPreset(($event.target as HTMLSelectElement).value)">
            <option value="">Keep the radio's current settings</option>
            <option v-for="p in MESHCORE_RF_PRESETS" :key="p.id" :value="p.id">{{ p.label }}</option>
            <option value="custom">Custom…</option>
          </select>
        </div>
        <div v-if="rfPreset === 'custom'" class="grid gap-3 grid-cols-2 sm:grid-cols-4">
          <div>
            <label class="block text-xs text-white/60 mb-1">Frequency (MHz)</label>
            <input v-model="form.rfFreqMhz" inputmode="decimal" placeholder="869.618" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Bandwidth (kHz)</label>
            <select v-model="form.rfBwKhz" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option value="">—</option>
              <option v-for="bw in ['62.5', '125', '250', '500']" :key="bw" :value="bw">{{ bw }}</option>
            </select>
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Spreading factor</label>
            <select v-model="form.rfSf" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option value="">—</option>
              <option v-for="sf in [5, 6, 7, 8, 9, 10, 11, 12]" :key="sf" :value="String(sf)">SF {{ sf }}</option>
            </select>
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Coding rate</label>
            <select v-model="form.rfCr" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option value="">—</option>
              <option v-for="cr in [5, 6, 7, 8]" :key="cr" :value="String(cr)">4/{{ cr }}</option>
            </select>
          </div>
        </div>
        <p class="text-[11px] text-white/40 mt-1">
          Saved settings program the radio on its next connect (it reboots once to apply). Leave all four empty to keep the radio's own settings.
        </p>
      </div>
      <!-- RNode (Reticulum) RF settings: the device's CURRENT values shown
           first (radio-confirmed read-back), then every parameter editable,
           with apply → device confirmation. Actions stack in a column. -->
      <div v-if="effectiveKind === 'reticulum'" class="mt-4">
        <div class="flex items-center justify-between mb-2">
          <h5 class="text-xs font-semibold text-white/80">RNode radio — current device settings</h5>
          <button class="text-[11px] text-sky-300/80 hover:text-sky-200 disabled:opacity-50" :disabled="rnodeLoading" @click="refreshRnodeLive">
            {{ rnodeLoading ? 'Reading…' : 'Refresh' }}
          </button>
        </div>
        <div v-if="rnodeLive" class="rounded-lg bg-white/[0.04] border border-white/10 p-3 mb-3 grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs">
          <div><span class="text-white/40 block">Status</span><span :class="rnodeLive.online ? 'text-green-400' : 'text-amber-400'">{{ rnodeLive.online ? 'Online' : 'Detected, not online' }}</span></div>
          <div><span class="text-white/40 block">Port</span><span class="text-white/80">{{ rnodeLive.port || '—' }}</span></div>
          <div><span class="text-white/40 block">Frequency</span><span class="text-white/80">{{ fmtMhz(rnodeLive.r_frequency ?? rnodeLive.frequency) }}</span></div>
          <div><span class="text-white/40 block">Bandwidth</span><span class="text-white/80">{{ rnodeLive.r_bandwidth ?? rnodeLive.bandwidth ?? '—' }} Hz</span></div>
          <div><span class="text-white/40 block">Spreading</span><span class="text-white/80">SF {{ rnodeLive.r_spreadingfactor ?? rnodeLive.spreadingfactor ?? '—' }}</span></div>
          <div><span class="text-white/40 block">Coding rate</span><span class="text-white/80">4/{{ rnodeLive.r_codingrate ?? rnodeLive.codingrate ?? '—' }}</span></div>
          <div><span class="text-white/40 block">TX power</span><span class="text-white/80">{{ rnodeLive.r_txpower ?? rnodeLive.txpower ?? '—' }} dBm</span></div>
          <div><span class="text-white/40 block">Airtime limits</span><span class="text-white/80">{{ rnodeLive.r_airtime_limit_short ?? rnodeLive.airtime_limit_short ?? '—' }}% / {{ rnodeLive.r_airtime_limit_long ?? rnodeLive.airtime_limit_long ?? '—' }}%</span></div>
        </div>
        <p v-else-if="rnodeLiveError" class="text-[11px] text-amber-400/80 mb-3">{{ rnodeLiveError }}</p>

        <h5 class="text-xs font-semibold text-white/80 mb-2">RNode RF parameters</h5>
        <div class="grid gap-3 grid-cols-2 sm:grid-cols-4">
          <div>
            <label class="block text-xs text-white/60 mb-1">Frequency (Hz)</label>
            <input v-model="rnodeForm.frequency" inputmode="numeric" placeholder="869462500" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Bandwidth (Hz)</label>
            <select v-model="rnodeForm.bandwidth" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option v-for="bw in ['7800','10400','15600','20800','31250','41700','62500','125000','250000','500000']" :key="bw" :value="bw">{{ bw }}</option>
            </select>
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Spreading factor</label>
            <select v-model="rnodeForm.spreading_factor" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option v-for="sf in [5,6,7,8,9,10,11,12]" :key="sf" :value="String(sf)">SF {{ sf }}</option>
            </select>
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Coding rate</label>
            <select v-model="rnodeForm.coding_rate" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60">
              <option v-for="cr in [5,6,7,8]" :key="cr" :value="String(cr)">4/{{ cr }}</option>
            </select>
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">TX power (dBm)</label>
            <input v-model="rnodeForm.txpower" inputmode="numeric" placeholder="14" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Airtime short (%)</label>
            <input v-model="rnodeForm.airtime_limit_short" inputmode="decimal" placeholder="25" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Airtime long (%)</label>
            <input v-model="rnodeForm.airtime_limit_long" inputmode="decimal" placeholder="10" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
          </div>
          <div>
            <label class="block text-xs text-white/60 mb-1">Serial port</label>
            <input v-model="rnodeForm.port" placeholder="auto-detect" class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60" />
          </div>
        </div>
        <label class="flex items-center gap-2 mt-3 text-sm text-white/80 cursor-pointer">
          <input v-model="rnodeForm.enabled" type="checkbox" class="h-4 w-4 accent-orange-500" />
          RNode interface enabled
        </label>

        <!-- Actions: stacked in a column on purpose (operator layout request) -->
        <div class="flex flex-col gap-2 mt-4 max-w-sm">
          <button
            class="glass-button px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
            :disabled="!rnodeRegionPlan || rnodeApplying"
            @click="setRnodeRecommendedForRegion"
          >
            {{ rnodeRegionPlan ? `Set recommended for ${form.region}` : 'Pick a region above first' }}
          </button>
          <button
            class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
            :disabled="rnodeApplying"
            @click="applyRnodeSettings"
          >
            {{ rnodeApplying ? 'Applying — waiting for the radio to confirm…' : 'Apply & Confirm on Device' }}
          </button>
        </div>
        <p v-if="rnodeResult" class="text-xs mt-2" :class="rnodeResult.ok && rnodeResult.confirmed ? 'text-green-400' : rnodeResult.ok ? 'text-amber-400' : 'text-red-400'">
          <template v-if="rnodeResult.ok && rnodeResult.confirmed">✓ </template>{{ rnodeResult.message }}
        </p>
      </div>

      <label class="flex items-center gap-2 mt-3 text-sm text-white/80 cursor-pointer">
        <input v-model="form.broadcastIdentity" type="checkbox" class="h-4 w-4 accent-orange-500" />
        Periodically broadcast this node's identity on the mesh
      </label>

      <p v-if="effectiveKind === 'auto'" class="text-[11px] text-white/40 mt-3">
        Options adapt to the detected firmware: region + channel program Meshtastic radios;
        MeshCore and RNode own their RF parameters in firmware/daemon config; name and identity apply to all.
      </p>

      <div class="flex items-center gap-3 mt-4">
        <button
          class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
          :disabled="saving"
          @click="saveSettings"
        >
          {{ saving ? 'Saving…' : 'Save Settings' }}
        </button>
        <span v-if="saveDone" class="text-xs text-green-400">Saved — applying to the radio now…</span>
        <span v-if="saveError" class="text-xs text-red-400">{{ saveError }}</span>
      </div>
    </div>

    <div class="mesh-device-panel-actions">
      <button
        class="glass-button mesh-device-reboot-btn"
        :disabled="rebooting || !mesh.status?.device_connected"
        @click="handleReboot"
      >
        <span v-if="rebooting" class="mesh-spinner" aria-hidden="true"></span>
        <template v-else>Reboot Radio</template>
      </button>
      <p class="mesh-device-reboot-hint">Use this if the device stops responding to sent messages or seems stuck.</p>
      <p v-if="rebootMessage" class="text-xs text-green-400 mt-1">{{ rebootMessage }}</p>
      <p v-if="rebootError" class="mesh-device-reboot-error">{{ rebootError }}</p>
    </div>
  </div>
</template>
