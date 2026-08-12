import { describe, it, expect, beforeEach, vi } from 'vitest'
import { useTheme } from './useTheme'

describe('useTheme', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.classList.remove('dark', 'light')
    // Reset module-level state by setting to default
    const { setTheme } = useTheme()
    setTheme('dark')
    vi.restoreAllMocks()
  })

  it('returns expected API', () => {
    const theme = useTheme()
    expect(theme.currentTheme).toBeDefined()
    expect(theme.isDark).toBeDefined()
    expect(theme.setTheme).toBeTypeOf('function')
    expect(theme.toggleTheme).toBeTypeOf('function')
    expect(theme.initTheme).toBeTypeOf('function')
  })

  it('defaults to dark theme', () => {
    const { isDark, currentTheme } = useTheme()
    expect(currentTheme.value).toBe('dark')
    expect(isDark.value).toBe(true)
  })

  it('setTheme switches to light', () => {
    const { setTheme, isDark, currentTheme } = useTheme()
    setTheme('light')

    expect(currentTheme.value).toBe('light')
    expect(isDark.value).toBe(false)
    expect(localStorage.getItem('aiui-theme')).toBe('light')
    expect(document.documentElement.classList.contains('light')).toBe(true)
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })

  it('setTheme switches to dark', () => {
    const { setTheme } = useTheme()
    setTheme('light')
    setTheme('dark')

    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(document.documentElement.classList.contains('light')).toBe(false)
    expect(localStorage.getItem('aiui-theme')).toBe('dark')
  })

  it('toggleTheme flips between dark and light', () => {
    const { toggleTheme, currentTheme } = useTheme()

    expect(currentTheme.value).toBe('dark')
    toggleTheme()
    expect(currentTheme.value).toBe('light')
    toggleTheme()
    expect(currentTheme.value).toBe('dark')
  })

  it('initTheme restores saved preference from localStorage', () => {
    localStorage.setItem('aiui-theme', 'light')

    const { initTheme, currentTheme } = useTheme()
    initTheme()

    expect(currentTheme.value).toBe('light')
  })

  it('initTheme falls back to prefers-color-scheme when no saved preference', () => {
    localStorage.clear()

    const matchMediaSpy = vi.spyOn(window, 'matchMedia').mockReturnValue({
      matches: false,
    } as MediaQueryList)

    const { initTheme, currentTheme } = useTheme()
    initTheme()

    expect(matchMediaSpy).toHaveBeenCalledWith('(prefers-color-scheme: dark)')
    expect(currentTheme.value).toBe('light')
  })

  it('initTheme uses dark when system prefers dark', () => {
    localStorage.clear()

    vi.spyOn(window, 'matchMedia').mockReturnValue({
      matches: true,
    } as MediaQueryList)

    const { initTheme, currentTheme } = useTheme()
    initTheme()

    expect(currentTheme.value).toBe('dark')
  })

  it('shares state across multiple useTheme calls', () => {
    const theme1 = useTheme()
    const theme2 = useTheme()

    theme1.setTheme('light')
    expect(theme2.currentTheme.value).toBe('light')
    expect(theme2.isDark.value).toBe(false)
  })
})
