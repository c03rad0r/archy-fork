import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PasswordRevealInput from '../PasswordRevealInput.vue'

// #145: the reveal toggle exists so a fresh-install user typing a WiFi key
// from across the room can see what they typed. The contract: masked by
// default, one tap reveals, v-model and enter behave like a plain input.

describe('PasswordRevealInput', () => {
  it('masks by default and reveals on toggle', async () => {
    const wrapper = mount(PasswordRevealInput, {
      props: { modelValue: 'hunter2', placeholder: 'WiFi password' },
    })
    const input = wrapper.find('input')
    expect(input.attributes('type')).toBe('password')

    await wrapper.find('button').trigger('click')
    expect(input.attributes('type')).toBe('text')

    await wrapper.find('button').trigger('click')
    expect(input.attributes('type')).toBe('password')
  })

  it('syncs v-model through update:modelValue', async () => {
    const wrapper = mount(PasswordRevealInput, { props: { modelValue: '' } })
    await wrapper.find('input').setValue('s3cret')
    const emitted = wrapper.emitted('update:modelValue') as string[][]
    expect(emitted[emitted.length - 1]).toEqual(['s3cret'])
  })

  it('emits enter on Enter keyup — the WiFi modal submits from the keyboard', async () => {
    const wrapper = mount(PasswordRevealInput, { props: { modelValue: 'pw' } })
    await wrapper.find('input').trigger('keyup.enter')
    expect(wrapper.emitted('enter')).toHaveLength(1)
  })

  it('passes placeholder and disabled through to the input', () => {
    const wrapper = mount(PasswordRevealInput, {
      props: { modelValue: '', placeholder: 'WiFi password', disabled: true },
    })
    const input = wrapper.find('input')
    expect(input.attributes('placeholder')).toBe('WiFi password')
    expect(input.attributes('disabled')).toBeDefined()
  })
})
