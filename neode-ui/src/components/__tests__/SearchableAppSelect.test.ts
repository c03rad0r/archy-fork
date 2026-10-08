import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import SearchableAppSelect from '../SearchableAppSelect.vue'
const options = [{ id: 'homeassistant', name: 'Home Assistant' }, { id: 'immich', name: 'Immich' }]
describe('searchable app picker', () => {
  it('renders actual touchable options, filters them and selects only an offered app', async () => {
    const wrapper = mount(SearchableAppSelect, { props: { modelValue: '', options } })
    await wrapper.get('button').trigger('click')
    expect(wrapper.findAll('[role="option"]')).toHaveLength(2)
    await wrapper.get('input').setValue('imm')
    expect(wrapper.findAll('[role="option"]')).toHaveLength(1)
    await wrapper.get('[role="option"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')).toEqual([['immich']])
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
  })
  it('supports keyboard choice and escape without selecting arbitrary search text', async () => {
    const wrapper = mount(SearchableAppSelect, { props: { modelValue: '', options } })
    await wrapper.get('button').trigger('keydown', { key: 'ArrowDown' })
    await wrapper.get('input').setValue('home')
    await wrapper.get('input').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('update:modelValue')).toEqual([['homeassistant']])
    await wrapper.get('button').trigger('click')
    await wrapper.get('input').setValue('unknown')
    await wrapper.get('input').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('update:modelValue')).toHaveLength(1)
    await wrapper.get('input').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
  })
})
