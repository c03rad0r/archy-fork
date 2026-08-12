import { defineStore } from 'pinia'
import { ref } from 'vue'
import { playNavSound } from '@/composables/useNavSounds'

export const useCLIStore = defineStore('cli', () => {
  const isOpen = ref(false)

  function open() {
    isOpen.value = true
    playNavSound('action')
  }

  function close() {
    isOpen.value = false
  }

  function toggle() {
    const wasOpen = isOpen.value
    isOpen.value = !wasOpen
    if (!wasOpen) playNavSound('action')
  }

  return {
    isOpen,
    open,
    close,
    toggle,
  }
})
