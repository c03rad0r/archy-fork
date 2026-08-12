<script setup lang="ts">
import { ref, onMounted, onUnmounted, onActivated, onDeactivated, nextTick, watch, computed } from 'vue'
import { useMeshStore } from '@/stores/mesh'
import type { NodePosition } from '@/stores/mesh'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'

const mesh = useMeshStore()

const mapContainer = ref<HTMLElement | null>(null)
let map: L.Map | null = null
const markersLayer = ref<L.LayerGroup | null>(null)
const linesLayer = ref<L.LayerGroup | null>(null)

// Whether we have any position data to show
const hasPositions = computed(() => mesh.nodePositions.size > 0 || mesh.federatedPositions.size > 0)

// Location sharing state
const sharingLocation = ref(false)
const locationSource = ref<'browser' | 'device'>('browser')
const locationError = ref('')
const hasDeviceGps = computed(() => mesh.deadmanStatus?.has_gps ?? false)
const locationPermissionDenied = computed(() => locationError.value === 'Location permission denied')
let geoWatchId: number | null = null

function toggleLocationSharing() {
  if (sharingLocation.value) {
    stopSharing()
  } else {
    startSharing()
  }
}

function switchSource(source: 'browser' | 'device') {
  locationSource.value = source
  if (sharingLocation.value) {
    stopSharing()
    startSharing()
  }
}

function startSharing() {
  locationError.value = ''

  if (locationSource.value === 'browser') {
    if (!navigator.geolocation) {
      locationError.value = 'Geolocation not supported'
      return
    }
    geoWatchId = navigator.geolocation.watchPosition(
      (pos) => {
        mesh.updateSelfPosition(pos.coords.latitude, pos.coords.longitude, 'This Node')
        sharingLocation.value = true
        locationError.value = ''
      },
      (err) => {
        locationError.value = err.code === 1 ? 'Location permission denied' : err.message
        sharingLocation.value = false
      },
      { enableHighAccuracy: true, timeout: 15000, maximumAge: 30000 },
    )
    sharingLocation.value = true
  } else {
    // Device GPS — position data comes from deadman/mesh GPS module
    sharingLocation.value = true
  }
}

function stopSharing() {
  if (geoWatchId !== null) {
    navigator.geolocation.clearWatch(geoWatchId)
    geoWatchId = null
  }
  sharingLocation.value = false
  mesh.nodePositions.delete(-1)
}

function createMarkerIcon(type: 'self' | 'online' | 'offline'): L.DivIcon {
  const colorMap = {
    self: { bg: '#fb923c', border: '#f59e0b', shadow: 'rgba(251,146,60,0.5)' },
    online: { bg: '#4ade80', border: '#22c55e', shadow: 'rgba(74,222,128,0.4)' },
    offline: { bg: '#6b7280', border: '#4b5563', shadow: 'rgba(107,114,128,0.3)' },
  }
  const c = colorMap[type]
  const size = type === 'self' ? 16 : 12
  const pulse = type === 'self'
    ? `<div style="position:absolute;top:50%;left:50%;transform:translate(-50%,-50%);width:${size + 12}px;height:${size + 12}px;border-radius:50%;background:${c.shadow};animation:mesh-map-pulse 2s infinite;"></div>`
    : ''

  return L.divIcon({
    className: 'mesh-map-marker-wrapper',
    iconSize: [size + 12, size + 12],
    iconAnchor: [(size + 12) / 2, (size + 12) / 2],
    popupAnchor: [0, -(size / 2 + 6)],
    html: `
      ${pulse}
      <div style="
        width:${size}px;height:${size}px;
        border-radius:50%;
        background:${c.bg};
        border:2px solid ${c.border};
        box-shadow:0 0 8px ${c.shadow};
        position:absolute;top:50%;left:50%;
        transform:translate(-50%,-50%);
        z-index:2;
      "></div>
    `,
  })
}

/// Marker for an Archipelago node (federation peer) — the little Archy logo
/// in a glowing badge, distinct from the plain colored dots used for raw
/// LoRa mesh-radio peers. Trusted nodes get the warm orange ring/glow the
/// rest of the map already uses for "this is us/ours"; Observer nodes get a
/// cooler blue so the trust boundary is visible at a glance.
function createFederatedMarkerIcon(trusted: boolean): L.DivIcon {
  const ring = trusted ? '#fb923c' : '#38bdf8'
  const glow = trusted ? 'rgba(251,146,60,0.55)' : 'rgba(56,189,248,0.5)'
  const size = 30
  return L.divIcon({
    className: 'mesh-map-marker-wrapper',
    iconSize: [size, size],
    iconAnchor: [size / 2, size / 2],
    popupAnchor: [0, -(size / 2 + 2)],
    html: `
      <div style="
        position:absolute; top:50%; left:50%; transform:translate(-50%,-50%);
        width:${size + 14}px; height:${size + 14}px; border-radius:50%;
        background:${glow}; animation:mesh-map-pulse 2.4s infinite;
      "></div>
      <div style="
        width:${size}px; height:${size}px; border-radius:9px;
        background:#0b0d14; border:2px solid ${ring};
        box-shadow:0 0 10px ${glow}, 0 2px 6px rgba(0,0,0,0.5);
        position:absolute; top:50%; left:50%; transform:translate(-50%,-50%);
        display:flex; align-items:center; justify-content:center;
        z-index:3; overflow:hidden;
      ">
        <img src="/assets/icon/apple-touch-icon-180x180-v2.png" width="${size - 6}" height="${size - 6}"
          style="border-radius:6px;object-fit:cover;" alt="" />
      </div>
    `,
  })
}

function buildFederatedPopupContent(name: string, onion: string, trusted: boolean): string {
  const badge = trusted
    ? '<span style="display:inline-block;background:rgba(251,146,60,0.2);color:#fb923c;font-size:0.65rem;padding:1px 6px;border-radius:4px;margin-left:6px;font-weight:600;">TRUSTED</span>'
    : '<span style="display:inline-block;background:rgba(56,189,248,0.2);color:#38bdf8;font-size:0.65rem;padding:1px 6px;border-radius:4px;margin-left:6px;font-weight:600;">OBSERVER</span>'
  const onionShort = onion.length > 20 ? `${onion.slice(0, 10)}...${onion.slice(-6)}` : onion
  return `
    <div style="font-family:'Avenir Next',sans-serif;min-width:170px;">
      <div style="display:flex;align-items:center;gap:6px;font-weight:600;font-size:0.9rem;color:#fff;margin-bottom:4px;">
        📡 ${name}${badge}
      </div>
      <div style="font-size:0.72rem;color:rgba(255,255,255,0.5);font-family:monospace;margin-bottom:6px;word-break:break-all;">
        ${onionShort}
      </div>
      <div style="font-size:0.78rem;color:rgba(255,255,255,0.6);">Archipelago node</div>
    </div>
  `
}

function getSignalBars(rssi: number | null): string {
  if (rssi === null) return 'Unknown'
  if (rssi >= -70) return 'Strong'
  if (rssi >= -90) return 'Good'
  if (rssi >= -110) return 'Weak'
  return 'Very Weak'
}

function formatLastHeard(timestamp: string): string {
  const now = Date.now()
  const then = new Date(timestamp).getTime()
  const diffSecs = Math.floor((now - then) / 1000)
  if (diffSecs < 60) return 'Just now'
  if (diffSecs < 3600) return `${Math.floor(diffSecs / 60)}m ago`
  if (diffSecs < 86400) return `${Math.floor(diffSecs / 3600)}h ago`
  return `${Math.floor(diffSecs / 86400)}d ago`
}

function truncatePubkey(pubkey: string | null): string {
  if (!pubkey) return 'No pubkey'
  if (pubkey.length <= 16) return pubkey
  return `${pubkey.slice(0, 8)}...${pubkey.slice(-8)}`
}

function buildPopupContent(
  name: string,
  pubkey: string | null,
  rssi: number | null,
  lastHeard: string,
  hops: number,
  isSelf: boolean,
): string {
  const signal = getSignalBars(rssi)
  const heard = formatLastHeard(lastHeard)
  const truncPk = truncatePubkey(pubkey)
  const selfBadge = isSelf
    ? '<span style="display:inline-block;background:rgba(251,146,60,0.2);color:#fb923c;font-size:0.65rem;padding:1px 6px;border-radius:4px;margin-left:6px;font-weight:600;">THIS NODE</span>'
    : ''

  return `
    <div style="font-family:'Avenir Next',sans-serif;min-width:160px;">
      <div style="font-weight:600;font-size:0.9rem;color:#fff;margin-bottom:4px;">
        ${name}${selfBadge}
      </div>
      <div style="font-size:0.72rem;color:rgba(255,255,255,0.5);font-family:monospace;margin-bottom:8px;word-break:break-all;">
        ${truncPk}
      </div>
      <div style="display:flex;flex-direction:column;gap:3px;font-size:0.78rem;">
        ${!isSelf ? `<div style="color:rgba(255,255,255,0.7);">Signal: <span style="color:${rssi !== null && rssi >= -90 ? '#4ade80' : '#fbbf24'};">${signal}</span>${rssi !== null ? ` (${rssi} dBm)` : ''}</div>` : ''}
        ${!isSelf ? `<div style="color:rgba(255,255,255,0.7);">Hops: <span style="color:rgba(255,255,255,0.9);">${hops}</span></div>` : ''}
        <div style="color:rgba(255,255,255,0.7);">Last heard: <span style="color:rgba(255,255,255,0.9);">${heard}</span></div>
      </div>
    </div>
  `
}

function initMap() {
  if (!mapContainer.value || map) return

  const el = mapContainer.value
  const rect = el.getBoundingClientRect()

  // If container has no height yet, retry
  if (rect.height < 10) {
    setTimeout(initMap, 150)
    return
  }

  map = L.map(el, {
    zoomControl: true,
    attributionControl: true,
    center: [30, 0],
    zoom: 3,
  })

  L.tileLayer('https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png', {
    attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> &copy; <a href="https://carto.com/">CARTO</a>',
    subdomains: 'abcd',
    maxZoom: 19,
    detectRetina: true,
  }).addTo(map)

  markersLayer.value = L.layerGroup().addTo(map)
  linesLayer.value = L.layerGroup().addTo(map)

  // Give Leaflet a frame to measure, then invalidate
  requestAnimationFrame(() => {
    if (map) {
      try { map.invalidateSize() } catch { /* destroyed */ }
    }
  })

  // Style attribution for dark theme
  const attrib = el.querySelector('.leaflet-control-attribution')
  if (attrib instanceof HTMLElement) {
    attrib.style.background = 'rgba(0,0,0,0.6)'
    attrib.style.color = 'rgba(255,255,255,0.4)'
    attrib.style.fontSize = '0.65rem'
  }

  updateMarkers()
}

function updateMarkers() {
  if (!map || !markersLayer.value || !linesLayer.value) return

  markersLayer.value.clearLayers()
  linesLayer.value.clearLayers()

  const positions = mesh.nodePositions
  const fedPositions = mesh.federatedPositions
  if (positions.size === 0 && fedPositions.size === 0) return

  const bounds: L.LatLngExpression[] = []
  const selfPos = positions.get(-1)

  // Find which contact_ids are in the peers list (for online status)
  const peerMap = new Map(mesh.peers.map(p => [p.contact_id, p]))

  positions.forEach((pos: NodePosition, contactId: number) => {
    const isSelf = contactId === -1
    const peer = peerMap.get(contactId)
    const isOnline = isSelf || !!peer

    const marker = L.marker([pos.lat, pos.lng], {
      icon: createMarkerIcon(isSelf ? 'self' : isOnline ? 'online' : 'offline'),
    })

    const name = isSelf
      ? (mesh.status?.self_advert_name ?? 'This Node')
      : (peer?.advert_name ?? pos.label ?? `Node ${contactId}`)
    const pubkey = isSelf ? null : (peer?.pubkey_hex ?? null)
    const rssi = peer?.rssi ?? null
    const lastHeard = isSelf ? new Date().toISOString() : (peer?.last_heard ?? pos.timestamp)
    const hops = peer?.hops ?? 0

    marker.bindPopup(buildPopupContent(name, pubkey, rssi, lastHeard, hops, isSelf), {
      className: 'mesh-map-popup',
      closeButton: true,
      maxWidth: 250,
    })

    markersLayer.value!.addLayer(marker)
    bounds.push([pos.lat, pos.lng])

    // Draw dashed line from self to each connected peer
    if (!isSelf && selfPos) {
      const line = L.polyline(
        [[selfPos.lat, selfPos.lng], [pos.lat, pos.lng]],
        {
          color: isOnline ? 'rgba(74,222,128,0.4)' : 'rgba(107,114,128,0.3)',
          weight: 1.5,
          dashArray: '6, 8',
          opacity: 0.7,
        },
      )
      linesLayer.value!.addLayer(line)
    }
  })

  // Archipelago nodes (federation peers who opted into location sharing) —
  // own logo marker, no signal/hops (that's a radio-peer concept).
  fedPositions.forEach((fed) => {
    const marker = L.marker([fed.lat, fed.lng], {
      icon: createFederatedMarkerIcon(fed.trusted),
      zIndexOffset: 500,
    })
    marker.bindPopup(buildFederatedPopupContent(fed.name ?? 'Archipelago Node', fed.onion, fed.trusted), {
      className: 'mesh-map-popup',
      closeButton: true,
      maxWidth: 250,
    })
    markersLayer.value!.addLayer(marker)
    bounds.push([fed.lat, fed.lng])
  })

  // Fit map to show all markers
  if (bounds.length > 1) {
    map.fitBounds(L.latLngBounds(bounds), { padding: [40, 40], maxZoom: 14 })
  } else if (bounds.length === 1 && bounds[0]) {
    map.setView(bounds[0], 12)
  }
}

function handleResize() {
  if (map) {
    map.invalidateSize()
  }
}

// Watch for changes in node positions and peers
watch(
  () => [mesh.nodePositions.size, mesh.peers.length, mesh.federatedPositions.size],
  () => {
    updateMarkers()
  },
)

let resizeObserver: ResizeObserver | null = null

// Only-while-visible: the window resize listener and ResizeObserver follow
// the Mesh tab's activate/deactivate lifecycle now that this component
// survives a tab switch under KeepAlive (T-02-03/D-03 — Mesh.vue joined
// KEEP_ALIVE_PATHS in 02-04). The Leaflet instance itself is never destroyed
// or recreated by any of this: initMap()'s own `if (!mapContainer.value ||
// map) return` guard already makes construction idempotent, so exactly one
// map is built per session no matter how many times armMapVisibility() runs.
// This only quiesces/repairs the listener + observer and re-tiles the map
// around it, so a map laid out while off screen doesn't paint at a stale or
// partial size on return.
//
// Called from BOTH onMounted and onActivated: onActivated is a documented
// no-op outside a <KeepAlive> boundary (see Mesh.vue's own armMeshLive for
// the same idiom), so a bare mount (a unit test, or any future non-KeepAlive
// usage) must not silently skip this setup.
function armMapVisibility() {
  window.removeEventListener('resize', handleResize)
  window.addEventListener('resize', handleResize)

  if (mapContainer.value) {
    if (!resizeObserver) {
      resizeObserver = new ResizeObserver((entries) => {
        const entry = entries[0]
        if (!entry) return
        const { height } = entry.contentRect
        if (!map && height > 10) {
          initMap()
        } else if (map) {
          try { map.invalidateSize() } catch { /* destroyed */ }
        }
      })
    }
    resizeObserver.observe(mapContainer.value)
  }

  // Fallback init for the very first mount (idempotent no-op once `map` is
  // already set — mirrors initMap's own not-yet-laid-out retry loop). Guard
  // against `map` already existing so later reactivations don't schedule a
  // throwaway no-op timer (WR-06) — this is genuinely first-mount-only.
  if (!map) setTimeout(initMap, 300)

  // A map laid out while the tab was off screen may have settled at a
  // zero/partial size; re-tile it at its real size now that it's back on
  // screen instead of showing an unsized or partially tiled canvas.
  if (map) {
    void nextTick(() => {
      try { map?.invalidateSize() } catch { /* destroyed */ }
    })
  }
}
function disarmMapVisibility() {
  window.removeEventListener('resize', handleResize)
  if (resizeObserver) resizeObserver.disconnect()
}

// Vue fires onActivated immediately after onMounted on a KeepAlive-wrapped
// component's first mount, which would otherwise double the fallback
// setTimeout(initMap, 300) call on every fresh page load (harmless since
// initMap is idempotent, but redundant). This flag lets onMounted's call
// count as the first activation's arm, so onActivated only re-arms on a
// genuine later reactivation, matching Mesh.vue's own meshFreshMount idiom.
let mapMountFresh = true
// Set when a live geolocation watch is torn down by onDeactivated so
// onActivated can transparently resume it (WR-02) — the browser location
// watch must not keep firing (battery drain, background location indicator)
// while the Mesh tab is off screen, matching the only-while-visible pattern
// already applied to the resize listener/ResizeObserver in this file.
let wasSharingBeforeDeactivate = false
onActivated(() => {
  if (mapMountFresh) { mapMountFresh = false; return }
  armMapVisibility()
  if (wasSharingBeforeDeactivate) {
    wasSharingBeforeDeactivate = false
    startSharing()
  }
})
onMounted(() => armMapVisibility())
onDeactivated(() => {
  disarmMapVisibility()
  if (sharingLocation.value) {
    wasSharingBeforeDeactivate = true
    stopSharing()
  }
})

onUnmounted(() => {
  disarmMapVisibility()
  stopSharing()
  if (map) {
    map.remove()
    map = null
  }
  markersLayer.value = null
  linesLayer.value = null
})
</script>

<template>
  <div class="mesh-map-outer">
    <div ref="mapContainer" class="mesh-map-inner"></div>

    <!-- Floating hint when no positions -->
    <div v-if="!hasPositions && !sharingLocation" class="mesh-map-hint">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" />
        <circle cx="12" cy="10" r="3" />
      </svg>
      <span>{{ locationPermissionDenied ? 'Local location is off. Other device positions will appear when received.' : 'Waiting for mesh device positions.' }}</span>
    </div>

    <!-- Location sharing overlay -->
    <div class="mesh-map-location-bar">
      <div class="mesh-map-location-row">
        <svg class="mesh-map-location-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" />
          <circle cx="12" cy="10" r="3" />
        </svg>
        <span class="mesh-map-location-label">Share Location</span>
        <button
          class="mesh-map-toggle"
          :class="{ active: sharingLocation }"
          role="switch"
          :aria-checked="sharingLocation"
          @click="toggleLocationSharing"
        >
          <span class="mesh-map-toggle-knob" />
        </button>
      </div>

      <!-- Source selector (visible when sharing and device GPS available) -->
      <div v-if="sharingLocation && hasDeviceGps" class="mesh-map-source-row">
        <button
          class="mesh-map-source-btn"
          :class="{ active: locationSource === 'browser' }"
          @click="switchSource('browser')"
        >This Machine</button>
        <button
          class="mesh-map-source-btn"
          :class="{ active: locationSource === 'device' }"
          @click="switchSource('device')"
        >Mesh Radio GPS</button>
      </div>

      <div v-if="locationError" class="mesh-map-location-error">
        {{ locationPermissionDenied ? 'Location permission denied. Peer locations can still appear on the map.' : locationError }}
      </div>
    </div>
  </div>
</template>

<style>
/* Must be unscoped — Leaflet creates DOM nodes dynamically */

/* CRITICAL: Override Tailwind's img { max-width: 100% } which breaks Leaflet tiles */
.mesh-map-inner img {
  max-width: none !important;
  max-height: none !important;
}

.mesh-map-outer {
  width: 100%;
  height: 100%;
  min-height: 420px;
  position: relative;
  overflow: hidden;
}

.mesh-map-inner {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: #1a1a2e;
}

/* Leaflet adds .leaflet-container to the element itself (not a child) */
.mesh-map-inner.leaflet-container {
  background: #1a1a2e;
}

.mesh-map-hint {
  position: absolute;
  bottom: 64px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 400;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(12px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  color: rgba(255, 255, 255, 0.6);
  font-size: 0.78rem;
  white-space: nowrap;
  pointer-events: none;
}

/* ─── Location sharing overlay ─── */
.mesh-map-location-bar {
  position: absolute;
  bottom: 8px;
  left: 8px;
  right: 8px;
  z-index: 500;
  background: rgba(0, 0, 0, 0.75);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 10px;
  padding: 8px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.mesh-map-location-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.mesh-map-location-icon {
  color: rgba(255, 255, 255, 0.5);
  flex-shrink: 0;
}

.mesh-map-location-label {
  font-size: 0.78rem;
  color: rgba(255, 255, 255, 0.7);
  font-weight: 500;
  flex: 1;
}

/* Toggle switch */
.mesh-map-toggle {
  width: 36px;
  height: 20px;
  /* The global mobile rule forces buttons to min-height:44px, which stretches
     this switch and pushes the knob off-centre. Pin it back to the pill size. */
  min-height: 20px !important;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  background: rgba(255, 255, 255, 0.1);
  cursor: pointer;
  position: relative;
  padding: 0;
  flex-shrink: 0;
  transition: all 0.25s ease;
}

.mesh-map-toggle.active {
  background: rgba(251, 146, 60, 0.35);
  border-color: rgba(251, 146, 60, 0.5);
}

.mesh-map-toggle-knob {
  display: block;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.6);
  position: absolute;
  top: 2px;
  left: 2px;
  transition: all 0.25s ease;
}

.mesh-map-toggle.active .mesh-map-toggle-knob {
  left: 18px;
  background: #fb923c;
}

/* Source selector */
.mesh-map-source-row {
  display: flex;
  gap: 4px;
  background: rgba(0, 0, 0, 0.3);
  border-radius: 6px;
  padding: 2px;
}

.mesh-map-source-btn {
  flex: 1;
  padding: 4px 8px;
  font-size: 0.7rem;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.5);
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.mesh-map-source-btn:hover {
  color: rgba(255, 255, 255, 0.7);
}

.mesh-map-source-btn.active {
  color: #fff;
  background: rgba(255, 255, 255, 0.1);
}

.mesh-map-location-error {
  font-size: 0.7rem;
  color: #ef4444;
}
</style>

<style>
/* Global styles for Leaflet popup theming - must not be scoped */
.mesh-map-popup .leaflet-popup-content-wrapper {
  background: rgba(0, 0, 0, 0.85);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 10px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  color: #fff;
}

.mesh-map-popup .leaflet-popup-tip {
  background: rgba(0, 0, 0, 0.85);
  border: 1px solid rgba(255, 255, 255, 0.15);
}

.mesh-map-popup .leaflet-popup-close-button {
  color: rgba(255, 255, 255, 0.5);
  font-size: 18px;
  padding: 4px 8px;
}

.mesh-map-popup .leaflet-popup-close-button:hover {
  color: rgba(255, 255, 255, 0.9);
}

/* Marker wrapper reset */
.mesh-map-marker-wrapper {
  background: none !important;
  border: none !important;
}

/* Pulse animation for self marker */
@keyframes mesh-map-pulse {
  0% { transform: translate(-50%, -50%) scale(1); opacity: 0.6; }
  50% { transform: translate(-50%, -50%) scale(1.8); opacity: 0; }
  100% { transform: translate(-50%, -50%) scale(1); opacity: 0; }
}

/* Dark theme for Leaflet zoom controls */
.mesh-map-inner .leaflet-control-zoom a {
  background: rgba(0, 0, 0, 0.7) !important;
  color: rgba(255, 255, 255, 0.8) !important;
  border-color: rgba(255, 255, 255, 0.1) !important;
}

.mesh-map-inner .leaflet-control-zoom a:hover {
  background: rgba(0, 0, 0, 0.85) !important;
  color: #fff !important;
}
</style>
