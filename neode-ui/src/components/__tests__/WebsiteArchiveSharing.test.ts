import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import WebsiteArchiveSharing from '../WebsiteArchiveSharing.vue'

describe('archived website sharing consent', () => {
  it('requires fresh approval after the reviewed snapshot changes', async () => {
    const wrapper = mount(WebsiteArchiveSharing, { props: { publication: { html: '<h1>Reviewed</h1>' }, archive: { sha256: 'a'.repeat(64), size: 17 } } })
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()
    expect(wrapper.get('iframe').attributes('sandbox')).toBe('')
    expect(wrapper.get('iframe').attributes('srcdoc')).toContain('<h1>Reviewed</h1>')
    await wrapper.get('input').setValue(true)
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('change')).toEqual([[true]])
    await wrapper.setProps({ publication: { html: '<h1>Different</h1>' } })
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()
    expect(wrapper.emitted('change')).toHaveLength(1)
  })
  it('keeps removal available without new approval and exposes only the selected hash', async () => {
    const hash = 'a'.repeat(64)
    const wrapper = mount(WebsiteArchiveSharing, { props: { publication: { html: 'public', public_archive: hash }, address: 'https://example.com/' } })
    expect(wrapper.text()).toContain('https://example.com/'+hash)
    expect(wrapper.find('input').exists()).toBe(false)
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('change')).toEqual([[false]])
  })
})
