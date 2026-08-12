import { ref } from 'vue'
import {
  parseFramesReducer,
  areFramesComplete,
  framesToData,
  totalNumberOfFrames,
  currentNumberOfFrames,
} from 'qrloop'

/**
 * Collects animated-QR frames (qrloop format, as used by k484 for large
 * Fedimint tokens) and reassembles them into the original token string.
 */
export function useAnimatedQRDecoder() {
  const framesState = ref<ReturnType<typeof parseFramesReducer> | null>(null)
  const isComplete = ref(false)
  const decodedData = ref<string | null>(null)
  const uniqueFrames = ref<Set<string>>(new Set())

  /** Feed one scanned frame; returns true once the full payload is decoded. */
  function addFrame(frame: string): boolean {
    if (isComplete.value) return true
    if (uniqueFrames.value.has(frame)) return false
    uniqueFrames.value.add(frame)

    try {
      framesState.value = parseFramesReducer(framesState.value, frame)
      if (areFramesComplete(framesState.value)) {
        const dataBuffer = framesToData(framesState.value)
        // Tokens travel as URL-safe base64
        decodedData.value = dataBuffer
          .toString('base64')
          .replace(/\+/g, '-')
          .replace(/\//g, '_')
        isComplete.value = true
        return true
      }
      return false
    } catch {
      // A frame that qrloop rejects may just be a different QR format
      return false
    }
  }

  function progressText(): string {
    if (!framesState.value) return ''
    const total = totalNumberOfFrames(framesState.value)
    const current = currentNumberOfFrames(framesState.value)
    return `${current}/${total} frames`
  }

  function reset() {
    framesState.value = null
    uniqueFrames.value.clear()
    isComplete.value = false
    decodedData.value = null
  }

  return { isComplete, decodedData, addFrame, reset, progressText }
}
