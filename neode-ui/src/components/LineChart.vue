<template>
  <canvas
    ref="canvasRef"
    class="monitoring-chart"
    :width="width"
    :height="height"
  ></canvas>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'

export interface ChartDataset {
  label: string
  data: number[]
  color: string
}

const props = withDefaults(
  defineProps<{
    datasets: ChartDataset[]
    labels?: string[]
    width?: number
    height?: number
    yMax?: number
    yLabel?: string
    showGrid?: boolean
  }>(),
  {
    width: 400,
    height: 180,
    showGrid: true,
  },
)

const canvasRef = ref<HTMLCanvasElement | null>(null)

function draw() {
  const canvas = canvasRef.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const dpr = window.devicePixelRatio || 1
  canvas.width = props.width * dpr
  canvas.height = props.height * dpr
  canvas.style.width = `${props.width}px`
  canvas.style.height = `${props.height}px`
  ctx.scale(dpr, dpr)

  const w = props.width
  const h = props.height
  const pad = { top: 10, right: 12, bottom: 24, left: 44 }
  const plotW = w - pad.left - pad.right
  const plotH = h - pad.top - pad.bottom

  // Clear
  ctx.clearRect(0, 0, w, h)

  if (!props.datasets.length || !props.datasets[0]?.data.length) {
    ctx.fillStyle = 'rgba(255,255,255,0.3)'
    ctx.font = '12px system-ui'
    ctx.textAlign = 'center'
    ctx.fillText('No data yet', w / 2, h / 2)
    return
  }

  // Compute y range
  let yMax = props.yMax ?? 0
  if (!yMax) {
    for (const ds of props.datasets) {
      for (const v of ds.data) {
        if (v > yMax) yMax = v
      }
    }
    yMax = yMax * 1.1 || 1
  }

  const maxPoints = Math.max(...props.datasets.map((d) => d.data.length))

  // Grid lines
  if (props.showGrid) {
    ctx.strokeStyle = 'rgba(255,255,255,0.06)'
    ctx.lineWidth = 1
    const gridCount = 4
    for (let i = 0; i <= gridCount; i++) {
      const y = pad.top + (plotH / gridCount) * i
      ctx.beginPath()
      ctx.moveTo(pad.left, y)
      ctx.lineTo(pad.left + plotW, y)
      ctx.stroke()
    }

    // Y-axis labels
    ctx.fillStyle = 'rgba(255,255,255,0.4)'
    ctx.font = '10px system-ui'
    ctx.textAlign = 'right'
    for (let i = 0; i <= gridCount; i++) {
      const y = pad.top + (plotH / gridCount) * i
      const val = yMax - (yMax / gridCount) * i
      ctx.fillText(formatValue(val), pad.left - 6, y + 3)
    }
  }

  // Draw each dataset
  for (const ds of props.datasets) {
    if (!ds.data.length) continue

    ctx.strokeStyle = ds.color
    ctx.lineWidth = 1.5
    ctx.lineJoin = 'round'
    ctx.lineCap = 'round'

    ctx.beginPath()
    for (let i = 0; i < ds.data.length; i++) {
      const x = pad.left + (i / Math.max(maxPoints - 1, 1)) * plotW
      const y = pad.top + plotH - (ds.data[i]! / yMax) * plotH
      if (i === 0) {
        ctx.moveTo(x, y)
      } else {
        ctx.lineTo(x, y)
      }
    }
    ctx.stroke()

    // Area fill
    ctx.globalAlpha = 0.08
    ctx.fillStyle = ds.color
    ctx.lineTo(pad.left + ((ds.data.length - 1) / Math.max(maxPoints - 1, 1)) * plotW, pad.top + plotH)
    ctx.lineTo(pad.left, pad.top + plotH)
    ctx.closePath()
    ctx.fill()
    ctx.globalAlpha = 1.0
  }

  // X-axis labels (first, middle, last)
  if (props.labels && props.labels.length > 0) {
    ctx.fillStyle = 'rgba(255,255,255,0.4)'
    ctx.font = '10px system-ui'
    ctx.textAlign = 'center'
    const indices = [0, Math.floor(props.labels.length / 2), props.labels.length - 1]
    for (const idx of indices) {
      if (idx >= 0 && idx < props.labels.length) {
        const x = pad.left + (idx / Math.max(props.labels.length - 1, 1)) * plotW
        ctx.fillText(props.labels[idx]!, pad.left + plotW + pad.right > w ? x : x, h - 6)
      }
    }
  }
}

function formatValue(val: number): string {
  if (val >= 1_000_000_000) return `${(val / 1_000_000_000).toFixed(1)}G`
  if (val >= 1_000_000) return `${(val / 1_000_000).toFixed(1)}M`
  if (val >= 1_000) return `${(val / 1_000).toFixed(1)}K`
  return val.toFixed(val < 10 ? 1 : 0)
}

watch(
  () => [props.datasets, props.labels, props.width, props.height],
  () => draw(),
  { deep: true },
)

onMounted(() => {
  draw()
  window.addEventListener('resize', draw)
})

onUnmounted(() => {
  window.removeEventListener('resize', draw)
})
</script>
