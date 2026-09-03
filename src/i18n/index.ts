import { createI18n } from 'vue-i18n'
import zh from './locales/zh'
import en from './locales/en'

export type MessageSchema = typeof zh

const stored = localStorage.getItem('lm.locale')
const browserLang = navigator.language?.startsWith('zh') ? 'zh' : 'en'
const defaultLocale = stored || browserLang

export const i18n = createI18n<[MessageSchema], 'zh' | 'en'>({
  legacy: false,
  locale: defaultLocale,
  fallbackLocale: 'en',
  messages: {
    zh,
    en,
  },
})

export function setLocale(locale: 'zh' | 'en') {
  ;(i18n.global.locale as unknown as { value: 'zh' | 'en' }).value = locale
  localStorage.setItem('lm.locale', locale)
  document.querySelector('html')?.setAttribute('lang', locale)
}
