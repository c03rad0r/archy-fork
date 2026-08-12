import { ref, computed, watch } from 'vue'
import { defineStore } from 'pinia'
import type { ContentTab } from '@/composables/contentFiltering'
import { storeApiKey, getApiKey, deleteApiKey } from '@/utils/key-vault'
import { hasSessionKey } from '@/utils/crypto'

const STORAGE_KEY = 'aiui-settings'

export interface AppSettings {
  // M15.1 — Accent colour
  accentColor: string
  // M15.2 — Glass intensity
  glassIntensity: 'subtle' | 'default' | 'strong'
  // M15.3 — Font size
  fontSize: 'compact' | 'default' | 'large'
  // M15.4 — Content type visibility
  hiddenContentTabs: ContentTab[]
  // M15.5 — Keyboard shortcuts
  shortcuts: Record<string, string>
  // M15.6 — Push notifications
  notificationsEnabled: boolean
  // M15.7 — Auto-archive
  autoArchiveDays: number // 0 = never, 7/30/90
  // M15.10 — Default conversation settings
  defaultModel: string
  defaultPersonaId: string
  defaultWebSearch: boolean
  defaultShowTokens: boolean
  // API key management — the boolean persists; the KEY NEVER does.
  // `claudeApiKey` lives only in the memory-only ref exposed by the store
  // below and, when a passphrase session is active, in the encrypted
  // key-vault. It is never written to localStorage again (S2).
  useOwnApiKey: boolean
}

const DEFAULT_SETTINGS: AppSettings = {
  accentColor: '#F7931A',
  glassIntensity: 'default',
  fontSize: 'default',
  hiddenContentTabs: [],
  shortcuts: {
    'send-message': 'Enter',
    'new-line': 'Shift+Enter',
    'search': 'Cmd+F',
    'new-chat': 'Cmd+N',
    'settings': 'Cmd+,',
    'close-panel': 'Escape',
  },
  notificationsEnabled: false,
  autoArchiveDays: 0,
  defaultModel: '',
  defaultPersonaId: '',
  defaultWebSearch: false,
  defaultShowTokens: false,
  useOwnApiKey: false,
}

/** Captured by the one-time plaintext migration inside `loadSettings`. */
let legacyClaudeKey = ''

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings>(loadSettings())

  // The Claude API key — memory-only. Persisted ENCRYPTED in the key-vault
  // when a passphrase session is active; otherwise it lasts this session
  // only. Seeded from the one-time plaintext migration above.
  const claudeApiKey = ref(legacyClaudeKey)

  /** Set/clear the key. Returns how it is held: 'encrypted' (vault) or 'session' (memory-only). */
  async function setClaudeApiKey(key: string): Promise<'encrypted' | 'session'> {
    claudeApiKey.value = key
    if (!key) {
      await deleteApiKey('claude').catch(() => {})
      return 'session'
    }
    if (hasSessionKey()) {
      try {
        await storeApiKey('claude', key)
        return 'encrypted'
      } catch { /* fall through to memory-only */ }
    }
    return 'session'
  }

  /** Load the key from the vault (after a passphrase unlock), or migrate a
   * just-scrubbed legacy key INTO the vault now that a session key exists. */
  async function initClaudeKey(): Promise<void> {
    if (claudeApiKey.value) {
      await persistClaudeKeyToVault()
      return
    }
    const k = await getApiKey('claude').catch(() => null)
    if (k) claudeApiKey.value = k
  }

  /** Persist the in-memory key to the vault once a session key exists
   *  (called after a passphrase unlock). No-op without one. */
  async function persistClaudeKeyToVault(): Promise<void> {
    if (claudeApiKey.value && hasSessionKey()) {
      try { await storeApiKey('claude', claudeApiKey.value) } catch { /* stays memory-only */ }
    }
  }

  function loadSettings(): AppSettings {
    try {
      const stored = localStorage.getItem(STORAGE_KEY)
      if (stored) {
        const parsed = JSON.parse(stored)
        // One-time migration (S2): this object used to carry the Claude API
        // key in plaintext. Lift it into the memory-only path and re-persist
        // the scrubbed object immediately, so the key never sits at rest in
        // localStorage again.
        if (typeof parsed.claudeApiKey === 'string' && parsed.claudeApiKey) {
          legacyClaudeKey = parsed.claudeApiKey
          delete parsed.claudeApiKey
          localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...DEFAULT_SETTINGS, ...parsed }))
        }
        return { ...DEFAULT_SETTINGS, ...parsed }
      }
    } catch { /* ignore */ }
    return { ...DEFAULT_SETTINGS }
  }

  function save() {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings.value))
  }

  // Apply CSS variables when settings change
  function applyCssVars() {
    const root = document.documentElement

    // Accent colour
    root.style.setProperty('--color-accent', settings.value.accentColor)

    // Glass intensity
    const glass = {
      subtle: { blur: '12px', opacity: '0.25' },
      default: { blur: '18px', opacity: '0.35' },
      strong: { blur: '28px', opacity: '0.50' },
    }[settings.value.glassIntensity]
    root.style.setProperty('--glass-blur', glass.blur)
    root.style.setProperty('--glass-opacity', glass.opacity)

    // Font size
    const sizes = { compact: '13px', default: '15px', large: '17px' }
    root.style.setProperty('--font-size-base', sizes[settings.value.fontSize])
  }

  // Watch for changes and persist + apply
  watch(settings, () => {
    save()
    applyCssVars()
  }, { deep: true })

  // Apply on init
  applyCssVars()

  const accentColor = computed({
    get: () => settings.value.accentColor,
    set: (v) => { settings.value.accentColor = v },
  })

  const glassIntensity = computed({
    get: () => settings.value.glassIntensity,
    set: (v) => { settings.value.glassIntensity = v },
  })

  const fontSize = computed({
    get: () => settings.value.fontSize,
    set: (v) => { settings.value.fontSize = v },
  })

  const hiddenContentTabs = computed({
    get: () => settings.value.hiddenContentTabs,
    set: (v) => { settings.value.hiddenContentTabs = v },
  })

  const notificationsEnabled = computed({
    get: () => settings.value.notificationsEnabled,
    set: (v) => { settings.value.notificationsEnabled = v },
  })

  const autoArchiveDays = computed({
    get: () => settings.value.autoArchiveDays,
    set: (v) => { settings.value.autoArchiveDays = v },
  })

  function isTabVisible(tab: ContentTab): boolean {
    return !settings.value.hiddenContentTabs.includes(tab)
  }

  function toggleTabVisibility(tab: ContentTab) {
    const idx = settings.value.hiddenContentTabs.indexOf(tab)
    if (idx >= 0) {
      settings.value.hiddenContentTabs.splice(idx, 1)
    } else {
      settings.value.hiddenContentTabs.push(tab)
    }
  }

  function setShortcut(action: string, binding: string) {
    settings.value.shortcuts[action] = binding
  }

  function resetSettings() {
    settings.value = { ...DEFAULT_SETTINGS }
  }

  return {
    settings,
    claudeApiKey,
    setClaudeApiKey,
    initClaudeKey,
    persistClaudeKeyToVault,
    accentColor,
    glassIntensity,
    fontSize,
    hiddenContentTabs,
    notificationsEnabled,
    autoArchiveDays,
    isTabVisible,
    toggleTabVisibility,
    setShortcut,
    resetSettings,
    applyCssVars,
  }
})
