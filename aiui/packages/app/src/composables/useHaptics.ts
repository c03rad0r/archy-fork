import { useSettingsStore } from '@/stores/settings'

export function useHaptics() {
  const isSupported = 'vibrate' in navigator

  function vibrate(pattern: number | number[]) {
    if (!isSupported) return
    try {
      const settings = useSettingsStore()
      if (settings.settings.notificationsEnabled === false) return // reuse notifications toggle
      navigator.vibrate(pattern)
    } catch { /* ignore */ }
  }

  function messageSend() { vibrate(10) }
  function favoriteToggle() { vibrate(15) }
  function error() { vibrate([50, 50, 50]) }
  function pullRefresh() { vibrate(20) }
  function longPress() { vibrate(20) }

  return {
    isSupported,
    vibrate,
    messageSend,
    favoriteToggle,
    error,
    pullRefresh,
    longPress,
  }
}
