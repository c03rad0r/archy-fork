// Client-side image compression presets for the mesh chat attachment picker,
// mirroring Columba's ImageCompressionPreset (Low/Medium/High/Original) —
// resize + iteratively-quality-reduced JPEG, entirely in the browser so no
// extra round-trip is needed before the existing send pipeline takes over.

export interface ImageCompressionPreset {
  key: 'low' | 'medium' | 'high' | 'original'
  displayName: string
  description: string
  maxDimensionPx: number
  targetBytes: number
  initialQuality: number
  minQuality: number
}

export const IMAGE_COMPRESSION_PRESETS: ImageCompressionPreset[] = [
  {
    key: 'low',
    displayName: 'Low',
    description: '32KB max — best for LoRa',
    maxDimensionPx: 320,
    targetBytes: 32 * 1024,
    initialQuality: 60,
    minQuality: 30,
  },
  {
    key: 'medium',
    displayName: 'Medium',
    description: '128KB max — balanced',
    maxDimensionPx: 800,
    targetBytes: 128 * 1024,
    initialQuality: 75,
    minQuality: 40,
  },
  {
    key: 'high',
    displayName: 'High',
    description: '512KB max — good quality',
    maxDimensionPx: 2048,
    targetBytes: 512 * 1024,
    initialQuality: 90,
    minQuality: 50,
  },
  {
    key: 'original',
    displayName: 'Original',
    description: 'No compression',
    maxDimensionPx: Infinity,
    targetBytes: Infinity,
    initialQuality: 100,
    minQuality: 100,
  },
]

/** Resize + iteratively shrink JPEG quality until under the preset's target size
 * (or `minQuality` is reached, whichever comes first). `original` is a no-op. */
export async function compressImage(file: File, preset: ImageCompressionPreset): Promise<File> {
  if (preset.key === 'original') return file

  const bitmap = await createImageBitmap(file)
  const { width, height } = scaledDimensions(bitmap.width, bitmap.height, preset.maxDimensionPx)
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('Canvas 2D context unavailable')
  ctx.drawImage(bitmap, 0, 0, width, height)
  bitmap.close()

  let quality = preset.initialQuality / 100
  let blob = await canvasToJpegBlob(canvas, quality)
  while (blob.size > preset.targetBytes && quality > preset.minQuality / 100) {
    quality = Math.max(quality - 0.1, preset.minQuality / 100)
    blob = await canvasToJpegBlob(canvas, quality)
  }

  const name = file.name.replace(/\.[^./\\]+$/, '') + '.jpg'
  return new File([blob], name, { type: 'image/jpeg', lastModified: Date.now() })
}

/** Tiny low-res JPEG (default 64px / low quality) for the `thumb_bytes` field
 * on a ContentRef message — shown immediately on receipt, before the full
 * image is fetched. */
export async function makeThumbnail(file: File, maxDimensionPx = 64, quality = 0.4): Promise<Uint8Array> {
  const bitmap = await createImageBitmap(file)
  const { width, height } = scaledDimensions(bitmap.width, bitmap.height, maxDimensionPx)
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('Canvas 2D context unavailable')
  ctx.drawImage(bitmap, 0, 0, width, height)
  bitmap.close()

  const blob = await canvasToJpegBlob(canvas, quality)
  return new Uint8Array(await blob.arrayBuffer())
}

function scaledDimensions(width: number, height: number, maxDimensionPx: number): { width: number; height: number } {
  if (!Number.isFinite(maxDimensionPx) || (width <= maxDimensionPx && height <= maxDimensionPx)) {
    return { width, height }
  }
  const scale = maxDimensionPx / Math.max(width, height)
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) }
}

function canvasToJpegBlob(canvas: HTMLCanvasElement, quality: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => (blob ? resolve(blob) : reject(new Error('canvas.toBlob failed'))),
      'image/jpeg',
      quality,
    )
  })
}
