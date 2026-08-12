<template>
  <div class="interactive-table my-4 rounded-lg bg-white/5 border border-white/5 overflow-hidden">
    <!-- Toolbar -->
    <div class="flex items-center gap-2 px-3 py-2 border-b border-white/5">
      <input
        v-model="filterQuery"
        type="text"
        class="flex-1 bg-white/5 border border-white/10 rounded px-2 py-1 text-base text-white/80 outline-none focus:border-accent/40 placeholder:text-white/25"
        placeholder="Filter rows..."
      />
      <button
        class="text-xs px-2 py-1 rounded bg-white/5 border border-white/10 text-white/50 hover:text-white/80 hover:bg-white/10 transition-colors"
        title="Export CSV"
        @click="exportCsv"
      >
        CSV
      </button>
    </div>

    <!-- Table -->
    <div class="overflow-x-auto">
      <table class="w-full text-xs">
        <thead>
          <tr class="border-b border-white/10">
            <th
              v-for="(header, col) in headers"
              :key="col"
              class="px-3 py-2 text-left text-white/50 font-medium cursor-pointer hover:text-white/70 select-none whitespace-nowrap"
              @click="toggleSort(col)"
            >
              <span class="inline-flex items-center gap-1">
                {{ header }}
                <span v-if="sortCol === col" class="text-accent">
                  {{ sortDir === 'asc' ? '↑' : '↓' }}
                </span>
              </span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="(row, i) in displayRows"
            :key="i"
            class="border-b border-white/5 hover:bg-white/5 transition-colors"
          >
            <td
              v-for="(cell, j) in row"
              :key="j"
              class="px-3 py-1.5 text-white/70 whitespace-nowrap"
            >
              {{ cell }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="filteredRows.length === 0" class="px-3 py-4 text-center text-xs text-white/30">
      No matching rows
    </div>

    <div v-if="filteredRows.length > 0" class="px-3 py-1.5 text-xs text-white/25 border-t border-white/5">
      {{ filteredRows.length }} row{{ filteredRows.length === 1 ? '' : 's' }}
      <span v-if="filterQuery"> (filtered from {{ rows.length }})</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

const props = defineProps<{
  headers: string[]
  rows: string[][]
}>()

const filterQuery = ref('')
const sortCol = ref(-1)
const sortDir = ref<'asc' | 'desc'>('asc')

function toggleSort(col: number) {
  if (sortCol.value === col) {
    sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc'
  } else {
    sortCol.value = col
    sortDir.value = 'asc'
  }
}

const filteredRows = computed(() => {
  const q = filterQuery.value.toLowerCase()
  if (!q) return props.rows
  return props.rows.filter((row) =>
    row.some((cell) => cell.toLowerCase().includes(q))
  )
})

const sortedRows = computed(() => {
  if (sortCol.value < 0) return filteredRows.value
  const col = sortCol.value
  const dir = sortDir.value === 'asc' ? 1 : -1
  return [...filteredRows.value].sort((a, b) => {
    const aVal = a[col] || ''
    const bVal = b[col] || ''
    // Try numeric sort
    const aNum = parseFloat(aVal)
    const bNum = parseFloat(bVal)
    if (!isNaN(aNum) && !isNaN(bNum)) return (aNum - bNum) * dir
    return aVal.localeCompare(bVal) * dir
  })
})

const displayRows = computed(() => sortedRows.value)

function exportCsv() {
  const header = props.headers.map(escapeCsvCell).join(',')
  const body = props.rows.map((row) => row.map(escapeCsvCell).join(',')).join('\n')
  const csv = header + '\n' + body
  const blob = new Blob([csv], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'table.csv'
  a.click()
  URL.revokeObjectURL(url)
}

function escapeCsvCell(cell: string): string {
  if (cell.includes(',') || cell.includes('"') || cell.includes('\n')) {
    return `"${cell.replace(/"/g, '""')}"`
  }
  return cell
}
</script>
