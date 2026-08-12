import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useControllerStore = defineStore('controller', () => {
  const isActive = ref(false)
  const gamepadCount = ref(0)

  function setActive(active: boolean) {
    isActive.value = active
  }

  function setGamepadCount(count: number) {
    gamepadCount.value = count
    isActive.value = count > 0
  }

  return {
    isActive,
    gamepadCount,
    setActive,
    setGamepadCount,
  }
})
