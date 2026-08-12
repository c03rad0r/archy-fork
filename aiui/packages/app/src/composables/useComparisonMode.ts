import { ref, computed } from 'vue'

const comparisonEnabled = ref(false)
const model1 = ref({ provider: 'claude' as string, model: 'claude-haiku-4.5' })
const model2 = ref({ provider: 'openrouter' as string, model: 'meta-llama/llama-4-maverick' })
const response1 = ref('')
const response2 = ref('')
const isStreaming1 = ref(false)
const isStreaming2 = ref(false)
const error1 = ref<string | null>(null)
const error2 = ref<string | null>(null)
let abortController: AbortController | null = null

export function useComparisonMode() {
  const isComparing = computed(() => comparisonEnabled.value)
  const isAnyStreaming = computed(() => isStreaming1.value || isStreaming2.value)

  function toggleComparison() {
    comparisonEnabled.value = !comparisonEnabled.value
  }

  function setModel(slot: 1 | 2, provider: string, modelId: string) {
    const target = slot === 1 ? model1 : model2
    target.value = { provider, model: modelId }
  }

  function clearResponses() {
    response1.value = ''
    response2.value = ''
    error1.value = null
    error2.value = null
  }

  function stopComparison() {
    if (abortController) {
      abortController.abort()
      abortController = null
    }
    isStreaming1.value = false
    isStreaming2.value = false
  }

  async function streamBothModels(
    streamFn: (
      provider: string,
      model: string,
      messages: { role: string; content: string }[],
      onToken: (text: string) => void,
      onError: (err: string) => void,
      signal: AbortSignal,
    ) => Promise<void>,
    messages: { role: string; content: string }[],
  ) {
    clearResponses()
    abortController = new AbortController()
    const signal = abortController.signal

    isStreaming1.value = true
    isStreaming2.value = true

    const p1 = streamFn(
      model1.value.provider,
      model1.value.model,
      messages,
      (token) => { response1.value += token },
      (err) => { error1.value = err },
      signal,
    ).finally(() => { isStreaming1.value = false })

    const p2 = streamFn(
      model2.value.provider,
      model2.value.model,
      messages,
      (token) => { response2.value += token },
      (err) => { error2.value = err },
      signal,
    ).finally(() => { isStreaming2.value = false })

    await Promise.allSettled([p1, p2])
    abortController = null
  }

  return {
    comparisonEnabled,
    isComparing,
    isAnyStreaming,
    model1,
    model2,
    response1,
    response2,
    isStreaming1,
    isStreaming2,
    error1,
    error2,
    toggleComparison,
    setModel,
    clearResponses,
    stopComparison,
    streamBothModels,
  }
}
