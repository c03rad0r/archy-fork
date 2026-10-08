import { describe, expect, it } from 'vitest'
import { computed, ref, watch, nextTick } from 'vue'
import { canonicalAppUrl } from '../appSessionConfig'

describe('stable app iframe URL', () => {
  it('does not replace the iframe source when runtime discovery adds only a root slash', async () => {
    const reported = ref('http://node.test:7778')
    const frameUrl = computed(() => canonicalAppUrl(reported.value))
    let changes = 0
    const stop = watch(frameUrl, () => { changes++ })
    expect(frameUrl.value).toBe('http://node.test:7778/')
    reported.value = 'http://node.test:7778/'
    await nextTick()
    expect(changes).toBe(0)
    reported.value = 'http://node.test:7778/project/demo'
    await nextTick()
    expect(changes).toBe(1)
    stop()
  })
  it('preserves deep paths, query strings and fragments', () => {
    expect(canonicalAppUrl('https://node.test:7778/project/demo?view=preview#player'))
      .toBe('https://node.test:7778/project/demo?view=preview#player')
  })
  it('keeps an absent app URL absent instead of opening the dashboard inside itself', () => {
    expect(canonicalAppUrl('')).toBe('')
  })
})
