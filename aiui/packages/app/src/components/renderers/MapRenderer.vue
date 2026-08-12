<template>
  <div class="map-renderer h-full flex flex-col">
    <!-- Toolbar -->
    <div class="flex items-center gap-2 px-4 py-2 bg-black/60 backdrop-blur-md border-b border-white/5 shrink-0">
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
        title="Back"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <span class="flex-1 text-xs text-white/40">{{ places.length }} place{{ places.length === 1 ? '' : 's' }}</span>
    </div>

    <!-- Map + place list -->
    <div class="flex-1 flex overflow-hidden">
      <!-- Map container -->
      <div ref="mapContainer" class="flex-1 min-h-0" />

      <!-- Place list sidebar (desktop only) -->
      <aside
        v-if="places.length > 1"
        class="hidden md:flex flex-col w-56 border-l border-white/5 overflow-y-auto scrollbar-hide"
      >
        <button
          v-for="(place, i) in places"
          :key="place.id"
          class="text-left px-3 py-2 border-b border-white/5 transition-colors hover:bg-white/5"
          :class="selectedIdx === i ? 'bg-accent/10' : ''"
          @click="selectPlace(i)"
        >
          <p class="text-xs text-white/80 truncate">{{ place.name }}</p>
          <p v-if="place.address || place.city" class="text-xs text-white/40 truncate">
            {{ place.address || place.city }}
          </p>
        </button>
      </aside>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, shallowRef } from 'vue'
import type { Place } from '@aiui/core/types/content'

const props = defineProps<{
  places: Place[]
}>()

defineEmits<{ back: [] }>()

const mapContainer = ref<HTMLElement | null>(null)
const selectedIdx = ref(-1)

type LeafletMap = import('leaflet').Map
const mapInstance = shallowRef<LeafletMap | null>(null)

async function initMap() {
  if (!mapContainer.value || props.places.length === 0) return

  const L = await import('leaflet')
  // Import leaflet CSS
  await import('leaflet/dist/leaflet.css')

  // Calculate bounds from places with coordinates
  const withCoords = props.places.filter((p) => p.lat != null && p.lng != null)
  if (withCoords.length === 0) return

  const map = L.map(mapContainer.value, {
    zoomControl: true,
    attributionControl: true,
  })

  L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
    attribution: '&copy; OpenStreetMap contributors',
    maxZoom: 19,
  }).addTo(map)

  // Custom orange marker icon
  const orangeIcon = L.divIcon({
    html: `<div style="background:#F7931A;width:12px;height:12px;border-radius:50%;border:2px solid rgba(255,255,255,0.8);box-shadow:0 1px 4px rgba(0,0,0,0.4)"></div>`,
    className: '',
    iconSize: [12, 12],
    iconAnchor: [6, 6],
  })

  const markers: import('leaflet').Marker[] = []
  const bounds = L.latLngBounds([])

  for (const place of withCoords) {
    const latlng = L.latLng(place.lat!, place.lng!)
    bounds.extend(latlng)
    const marker = L.marker(latlng, { icon: orangeIcon }).addTo(map)

    // Popup
    let popupHtml = `<div style="font-family:system-ui;font-size:12px"><strong>${place.name}</strong>`
    if (place.address) popupHtml += `<br><span style="color:#888">${place.address}</span>`
    if (place.rating) popupHtml += `<br>Rating: ${place.rating}/5`
    popupHtml += '</div>'
    marker.bindPopup(popupHtml)
    markers.push(marker)
  }

  if (withCoords.length === 1) {
    map.setView([withCoords[0].lat!, withCoords[0].lng!], 14)
  } else {
    map.fitBounds(bounds, { padding: [40, 40] })
  }

  mapInstance.value = map

  // Invalidate size after animation
  setTimeout(() => map.invalidateSize(), 200)
}

function selectPlace(idx: number) {
  selectedIdx.value = idx
  const place = props.places[idx]
  if (place.lat != null && place.lng != null && mapInstance.value) {
    mapInstance.value.setView([place.lat, place.lng], 16, { animate: true })
  }
}

onMounted(() => {
  initMap()
})

watch(() => props.places, () => {
  mapInstance.value?.remove()
  mapInstance.value = null
  initMap()
})

onBeforeUnmount(() => {
  mapInstance.value?.remove()
})
</script>
