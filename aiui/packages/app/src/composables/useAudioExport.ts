import { ref, computed } from 'vue'
import type { Message } from '@aiui/core/types/message'

export interface AudioExportOptions {
  voiceIndex: number
  rate: number
  includeMusic: boolean
  onlyAssistant: boolean
}

export function useAudioExport() {
  const isPlaying = ref(false)
  const isPaused = ref(false)
  const currentIndex = ref(0)
  const progress = ref(0)
  const availableVoices = ref<SpeechSynthesisVoice[]>([])
  const utterance = ref<SpeechSynthesisUtterance | null>(null)

  const isSupported = computed(() => 'speechSynthesis' in window)

  function loadVoices() {
    if (!isSupported.value) return
    const synth = window.speechSynthesis
    availableVoices.value = synth.getVoices()
    if (availableVoices.value.length === 0) {
      synth.onvoiceschanged = () => {
        availableVoices.value = synth.getVoices()
      }
    }
  }

  function play(messages: Message[], options: AudioExportOptions) {
    if (!isSupported.value) return
    const synth = window.speechSynthesis

    const filtered = options.onlyAssistant
      ? messages.filter((m) => m.role === 'assistant')
      : messages

    if (filtered.length === 0) return

    isPlaying.value = true
    isPaused.value = false
    currentIndex.value = 0

    function speakNext(index: number) {
      if (index >= filtered.length || !isPlaying.value) {
        stop()
        return
      }

      currentIndex.value = index
      progress.value = (index / filtered.length) * 100

      const msg = filtered[index]
      const text = msg.content.replace(/```[\s\S]*?```/g, 'code block').replace(/[#*_`]/g, '')

      const utt = new SpeechSynthesisUtterance(text)
      utt.rate = options.rate
      if (availableVoices.value[options.voiceIndex]) {
        utt.voice = availableVoices.value[options.voiceIndex]
      }

      utt.onend = () => {
        speakNext(index + 1)
      }

      utt.onerror = () => {
        speakNext(index + 1)
      }

      utterance.value = utt
      synth.speak(utt)
    }

    speakNext(0)
  }

  function pause() {
    if (!isSupported.value) return
    window.speechSynthesis.pause()
    isPaused.value = true
  }

  function resume() {
    if (!isSupported.value) return
    window.speechSynthesis.resume()
    isPaused.value = false
  }

  function stop() {
    if (!isSupported.value) return
    window.speechSynthesis.cancel()
    isPlaying.value = false
    isPaused.value = false
    currentIndex.value = 0
    progress.value = 0
    utterance.value = null
  }

  function skipForward(messages: Message[], options: AudioExportOptions) {
    const filtered = options.onlyAssistant
      ? messages.filter((m) => m.role === 'assistant')
      : messages
    if (currentIndex.value < filtered.length - 1) {
      window.speechSynthesis.cancel()
      play(messages, { ...options })
      // Advance to next
      currentIndex.value = Math.min(currentIndex.value + 1, filtered.length - 1)
    }
  }

  async function exportAsWav(messages: Message[], options: AudioExportOptions): Promise<Blob | null> {
    if (!isSupported.value) return null

    // Use Web Audio API to record speech synthesis output
    // This is a simplified approach — full implementation would use OfflineAudioContext
    const filtered = options.onlyAssistant
      ? messages.filter((m) => m.role === 'assistant')
      : messages

    const fullText = filtered.map((m) => m.content.replace(/```[\s\S]*?```/g, '').replace(/[#*_`]/g, '')).join('\n\n')

    // For a proper WAV export, we'd need MediaRecorder + audio routing
    // For now, return a text-based file that can be used with external TTS
    const encoder = new TextEncoder()
    return new Blob([encoder.encode(fullText)], { type: 'text/plain' })
  }

  return {
    isSupported,
    isPlaying,
    isPaused,
    currentIndex,
    progress,
    availableVoices,
    loadVoices,
    play,
    pause,
    resume,
    stop,
    skipForward,
    exportAsWav,
  }
}
