import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useControllerStore } from '../controller'

describe('useControllerStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('starts with default state', () => {
    const store = useControllerStore()
    expect(store.isActive).toBe(false)
    expect(store.gamepadCount).toBe(0)
  })

  it('setActive sets isActive to true', () => {
    const store = useControllerStore()
    store.setActive(true)
    expect(store.isActive).toBe(true)
  })

  it('setActive sets isActive to false', () => {
    const store = useControllerStore()
    store.setActive(true)
    store.setActive(false)
    expect(store.isActive).toBe(false)
  })

  it('setGamepadCount updates count and activates when > 0', () => {
    const store = useControllerStore()
    store.setGamepadCount(2)
    expect(store.gamepadCount).toBe(2)
    expect(store.isActive).toBe(true)
  })

  it('setGamepadCount deactivates when count is 0', () => {
    const store = useControllerStore()
    store.setGamepadCount(1)
    expect(store.isActive).toBe(true)
    store.setGamepadCount(0)
    expect(store.gamepadCount).toBe(0)
    expect(store.isActive).toBe(false)
  })

  it('setActive does not affect gamepadCount', () => {
    const store = useControllerStore()
    store.setGamepadCount(3)
    store.setActive(false)
    expect(store.isActive).toBe(false)
    expect(store.gamepadCount).toBe(3)
  })
})
