import { describe, it, expect, beforeEach } from 'vitest'
import { isTextField, typeKeyIntoField } from '../remote-relay'

/**
 * Companion-cursor text entry. Synthetic KeyboardEvents do NOT mutate input
 * values in the browser, so the relay edits `.value` at the caret directly and
 * fires an `input` event. These tests lock in that behaviour so a regression
 * (like the old "type goes to document, nothing happens" bug) is caught before
 * release rather than by a user with a companion controller.
 */
describe('isTextField', () => {
  it('accepts text-like inputs and textareas', () => {
    const text = document.createElement('input')
    text.type = 'text'
    const search = document.createElement('input')
    search.type = 'search'
    const area = document.createElement('textarea')
    expect(isTextField(text)).toBe(true)
    expect(isTextField(search)).toBe(true)
    expect(isTextField(area)).toBe(true)
  })

  it('rejects non-text controls and null', () => {
    const checkbox = document.createElement('input')
    checkbox.type = 'checkbox'
    expect(isTextField(checkbox)).toBe(false)
    expect(isTextField(document.createElement('button'))).toBe(false)
    expect(isTextField(document.createElement('div'))).toBe(false)
    expect(isTextField(null)).toBe(false)
  })
})

describe('typeKeyIntoField', () => {
  let input: HTMLInputElement
  let inputEvents: number

  beforeEach(() => {
    input = document.createElement('input')
    input.type = 'search'
    document.body.appendChild(input)
    inputEvents = 0
    input.addEventListener('input', () => { inputEvents++ })
  })

  it('inserts printable characters at the caret and fires input', () => {
    typeKeyIntoField(input, 'b')
    typeKeyIntoField(input, 't')
    typeKeyIntoField(input, 'c')
    expect(input.value).toBe('btc')
    expect(input.selectionStart).toBe(3)
    expect(inputEvents).toBe(3)
  })

  it('inserts a character in the middle of existing text', () => {
    input.value = 'bc'
    input.selectionStart = input.selectionEnd = 1
    typeKeyIntoField(input, 't')
    expect(input.value).toBe('btc')
    expect(input.selectionStart).toBe(2)
  })

  it('backspace deletes the char before the caret', () => {
    input.value = 'btc'
    input.selectionStart = input.selectionEnd = 3
    typeKeyIntoField(input, 'Backspace')
    expect(input.value).toBe('bt')
    expect(input.selectionStart).toBe(2)
  })

  it('backspace removes the active selection', () => {
    input.value = 'bitcoin'
    input.selectionStart = 0
    input.selectionEnd = 3
    typeKeyIntoField(input, 'Backspace')
    expect(input.value).toBe('coin')
    expect(input.selectionStart).toBe(0)
  })

  it('arrow keys move the caret without changing the value', () => {
    input.value = 'abc'
    input.selectionStart = input.selectionEnd = 3
    typeKeyIntoField(input, 'ArrowLeft')
    expect(input.selectionStart).toBe(2)
    expect(input.value).toBe('abc')
  })

  it('Enter on a single-line input is left for the app to handle', () => {
    input.value = 'query'
    input.selectionStart = input.selectionEnd = 5
    const consumed = typeKeyIntoField(input, 'Enter')
    expect(consumed).toBe(false)
    expect(input.value).toBe('query')
  })

  it('Enter inserts a newline in a textarea', () => {
    const area = document.createElement('textarea')
    area.value = 'a'
    area.selectionStart = area.selectionEnd = 1
    expect(typeKeyIntoField(area, 'Enter')).toBe(true)
    expect(area.value).toBe('a\n')
  })

  it('non-text keys are not consumed as editing', () => {
    input.value = 'x'
    input.selectionStart = input.selectionEnd = 1
    expect(typeKeyIntoField(input, 'Escape')).toBe(false)
    expect(typeKeyIntoField(input, 'Tab')).toBe(false)
    expect(input.value).toBe('x')
  })
})
