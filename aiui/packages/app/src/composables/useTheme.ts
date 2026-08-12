import { ref, computed } from 'vue'

type ThemeName = 'dark' | 'light'

const currentTheme = ref<ThemeName>('dark')

export function useTheme() {
  const isDark = computed(() => currentTheme.value === 'dark')

  const setTheme = (theme: ThemeName) => {
    currentTheme.value = theme
    localStorage.setItem('aiui-theme', theme)
    document.documentElement.classList.toggle('dark', theme === 'dark')
    document.documentElement.classList.toggle('light', theme === 'light')
  }

  const toggleTheme = () => {
    setTheme(isDark.value ? 'light' : 'dark')
  }

  const initTheme = () => {
    const saved = localStorage.getItem('aiui-theme') as ThemeName | null
    if (saved) {
      setTheme(saved)
    } else if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
      setTheme('dark')
    } else {
      setTheme('light')
    }
  }

  return { currentTheme, isDark, setTheme, toggleTheme, initTheme }
}
