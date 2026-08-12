<template>
  <div class="event-card rounded-xl bg-white/5 border border-white/10 px-4 py-3">
    <div class="flex items-start gap-3">
      <!-- Date chip -->
      <div class="shrink-0 w-14 text-center rounded-lg bg-accent/10 border border-accent/20 py-1.5">
        <p class="text-xs text-accent/70 uppercase">{{ monthLabel }}</p>
        <p class="text-lg font-bold text-accent leading-tight">{{ dayLabel }}</p>
      </div>

      <div class="flex-1 min-w-0">
        <h3 class="text-sm font-semibold text-white/90 truncate">{{ event.title }}</h3>
        <p v-if="event.location" class="text-xs text-white/50 mt-0.5 truncate">{{ event.location }}</p>
        <p v-if="event.description" class="text-xs text-white/40 mt-1 line-clamp-2">{{ event.description }}</p>

        <!-- Countdown -->
        <p v-if="countdownText" class="text-xs mt-1.5" :class="isPast ? 'text-white/30' : 'text-accent/70'">
          {{ countdownText }}
        </p>

        <!-- Actions -->
        <div class="flex gap-2 mt-2">
          <button
            class="text-xs px-2 py-1 rounded bg-white/5 border border-white/10 text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
            title="Download ICS file"
            @click.stop="downloadIcs"
          >
            Add to Calendar
          </button>
          <a
            :href="googleCalendarUrl"
            target="_blank"
            rel="noopener noreferrer"
            class="text-xs px-2 py-1 rounded bg-white/5 border border-white/10 text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
            @click.stop
          >
            Google Calendar
          </a>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount } from 'vue'
import type { EventData } from '@/composables/contentExtraction'

const props = defineProps<{ event: EventData }>()

const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  timer = setInterval(() => { now.value = Date.now() }, 1000)
})
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})

const eventDate = computed(() => {
  if (!props.event.date) return null
  const d = new Date(props.event.date)
  return isNaN(d.getTime()) ? null : d
})

const monthLabel = computed(() => {
  if (!eventDate.value) return '?'
  return eventDate.value.toLocaleString('en', { month: 'short' }).toUpperCase()
})

const dayLabel = computed(() => {
  if (!eventDate.value) return '?'
  return eventDate.value.getDate()
})

const isPast = computed(() => {
  if (!eventDate.value) return false
  return eventDate.value.getTime() < now.value
})

const countdownText = computed(() => {
  if (!eventDate.value) return ''
  const diff = eventDate.value.getTime() - now.value
  if (diff < 0) return 'Event has passed'

  const days = Math.floor(diff / (1000 * 60 * 60 * 24))
  const hours = Math.floor((diff % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60))
  const minutes = Math.floor((diff % (1000 * 60 * 60)) / (1000 * 60))

  if (days > 0) return `${days}d ${hours}h remaining`
  if (hours > 0) return `${hours}h ${minutes}m remaining`
  return `${minutes}m remaining`
})

function formatIcsDate(d: Date): string {
  return d.toISOString().replace(/[-:]/g, '').replace(/\.\d{3}/, '')
}

function downloadIcs() {
  const d = eventDate.value
  if (!d) return

  const end = new Date(d.getTime() + 60 * 60 * 1000) // 1 hour default
  const ics = [
    'BEGIN:VCALENDAR',
    'VERSION:2.0',
    'BEGIN:VEVENT',
    `DTSTART:${formatIcsDate(d)}`,
    `DTEND:${formatIcsDate(end)}`,
    `SUMMARY:${props.event.title}`,
    props.event.location ? `LOCATION:${props.event.location}` : '',
    props.event.description ? `DESCRIPTION:${props.event.description}` : '',
    props.event.url ? `URL:${props.event.url}` : '',
    'END:VEVENT',
    'END:VCALENDAR',
  ].filter(Boolean).join('\r\n')

  const blob = new Blob([ics], { type: 'text/calendar' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `${props.event.title.replace(/[^a-zA-Z0-9]/g, '_')}.ics`
  a.click()
  URL.revokeObjectURL(url)
}

const googleCalendarUrl = computed(() => {
  const d = eventDate.value
  if (!d) return '#'

  const start = formatIcsDate(d).replace('Z', '')
  const end = formatIcsDate(new Date(d.getTime() + 60 * 60 * 1000)).replace('Z', '')
  const params = new URLSearchParams({
    action: 'TEMPLATE',
    text: props.event.title,
    dates: `${start}/${end}`,
  })
  if (props.event.location) params.set('location', props.event.location)
  if (props.event.description) params.set('details', props.event.description)
  return `https://calendar.google.com/calendar/render?${params.toString()}`
})
</script>
