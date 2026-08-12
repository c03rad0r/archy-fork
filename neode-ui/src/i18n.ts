import { createI18n } from 'vue-i18n'
import en from './locales/en.json'

export type MessageSchema = typeof en

export const SUPPORTED_LOCALES = [
  { code: 'en', name: 'English', flag: '🇬🇧' },
  { code: 'es', name: 'Español', flag: '🇪🇸' },
] as const

export type SupportedLocale = typeof SUPPORTED_LOCALES[number]['code']

const savedLocale = (typeof localStorage !== 'undefined'
  ? localStorage.getItem('neode_locale')
  : null) as SupportedLocale | null

const i18n = createI18n({
  legacy: false,
  locale: savedLocale || 'en',
  fallbackLocale: 'en',
  messages: {
    en,
  },
})

/** Lazy-load a locale's messages and switch to it. */
export async function setLocale(locale: SupportedLocale) {
  if (locale === 'en') {
    i18n.global.locale = 'en' as never
    localStorage.setItem('neode_locale', 'en')
    return
  }

  if (!(i18n.global.availableLocales as string[]).includes(locale)) {
    const messages = await import(`./locales/${locale}.json`)
    ;(i18n.global as ReturnType<typeof createI18n>['global']).setLocaleMessage(locale, messages.default)
  }

  i18n.global.locale = locale as never
  localStorage.setItem('neode_locale', locale)
}

// Load saved locale on startup
if (savedLocale && savedLocale !== 'en') {
  setLocale(savedLocale)
}

export default i18n
