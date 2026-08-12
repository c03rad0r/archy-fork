import { ref, computed } from 'vue'
import en from './en.json'
import es from './es.json'
import fr from './fr.json'

type Messages = typeof en

const STORAGE_KEY = 'aiui-locale'

const locales: Record<string, Messages> = { en, es, fr }

const currentLocale = ref(loadLocale())

function loadLocale(): string {
  const saved = localStorage.getItem(STORAGE_KEY)
  if (saved && locales[saved]) return saved

  // Auto-detect from browser
  const browserLang = navigator.language.split('-')[0]
  if (locales[browserLang]) return browserLang

  return 'en'
}

export function useI18n() {
  const locale = computed({
    get: () => currentLocale.value,
    set: (v: string) => {
      currentLocale.value = v
      localStorage.setItem(STORAGE_KEY, v)
      // Set dir attribute for RTL
      document.documentElement.dir = isRtl(v) ? 'rtl' : 'ltr'
    },
  })

  const messages = computed(() => locales[currentLocale.value] ?? en)

  function t(path: string): string {
    const keys = path.split('.')
    let result: unknown = messages.value
    for (const key of keys) {
      if (result && typeof result === 'object') {
        result = (result as Record<string, unknown>)[key]
      } else {
        return path
      }
    }
    return typeof result === 'string' ? result : path
  }

  const availableLocales = Object.keys(locales)

  return {
    locale,
    messages,
    t,
    availableLocales,
  }
}

function isRtl(locale: string): boolean {
  return ['ar', 'he', 'fa', 'ur'].includes(locale)
}
