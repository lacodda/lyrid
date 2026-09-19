import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

import en from './locales/en.json'
import ru from './locales/ru.json'

/*
 * The interface's language.
 *
 * The same shape kilna uses, deliberately: the line already settled how this
 * works and holds it with a gate, so a second mechanism in the same words
 * would be the divergence the rule against it exists to prevent.
 *
 * English is the source language, never a fallback for a missing translation.
 * `tools/check-locales.mjs` holds every locale to the same shape at build time,
 * so there is nothing to fall back *to*: a gap fails the build instead of
 * reaching a person as a stray English line in a Russian window.
 *
 * What is *not* in here: an artist's name, a genre, a Wikipedia extract. The
 * canon is data, in whatever language its source wrote it, and translating it
 * is not this layer's business — this layer is the words lyrid says itself.
 */
export const defaultNS = 'translation'
export const resources = {
  en: { translation: en },
  ru: { translation: ru },
} as const

void i18n.use(initReactI18next).init({
  resources,
  // The real language is chosen in `lib/language.ts` before the first paint;
  // this is only what exists until then.
  lng: 'en',
  fallbackLng: false,
  interpolation: { escapeValue: false },
})

export default i18n
