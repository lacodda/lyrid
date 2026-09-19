import { useSyncExternalStore } from 'react'

import i18n from '@/i18n'

/*
 * Which language the interface is in.
 *
 * `system` follows the browser, an explicit choice pins. Stored per browser
 * rather than on the account: a language is a property of the machine someone
 * is sitting at, not of who they are — the same person reads English at work
 * and Russian at home, and an account that carried the choice between them
 * would be wrong on one of the two.
 */
export type Language = 'system' | 'en' | 'ru'

export const LANGUAGES: readonly Language[] = ['system', 'en', 'ru']

/** The locales that actually exist as files — what `system` may resolve to. */
const SUPPORTED = new Set(['en', 'ru'])
const STORAGE_KEY = 'lyrid.language'
const FALLBACK = 'en'

export function storedLanguage(): Language {
  // Storage can be refused outright — a private window, or a browser told to
  // block site data — and it throws rather than returning null when it is.
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    return LANGUAGES.includes(raw as Language) ? (raw as Language) : 'system'
  } catch {
    return 'system'
  }
}

/**
 * The locale a choice resolves to.
 *
 * `navigator.language` is a full tag like `ru-RU`; only the base language is
 * meaningful here, since lyrid carries no regional variants.
 */
export function resolveLanguage(choice: Language): string {
  if (choice !== 'system') return choice

  for (const tag of navigator.languages.length > 0 ? navigator.languages : [navigator.language]) {
    const base = tag.split('-')[0]?.toLowerCase()
    if (base !== undefined && SUPPORTED.has(base)) return base
  }
  return FALLBACK
}

/**
 * Called once at startup, before React renders.
 *
 * Without this the first paint would be in whatever i18n was initialised with,
 * and the page would visibly change language a moment later.
 */
export function initLanguage(): void {
  void i18n.changeLanguage(resolveLanguage(storedLanguage()))

  // `<html lang>` is what hyphenation and screen readers go by, so it has to
  // follow the interface rather than stay at whatever index.html declared.
  i18n.on('languageChanged', language => {
    document.documentElement.lang = language
  })
  document.documentElement.lang = i18n.resolvedLanguage ?? FALLBACK
}

// i18next is the source of truth for "what language is showing"; the stored
// choice is a separate question ('system' is a choice, not a language).
const subscribe = (onChange: () => void) => {
  i18n.on('languageChanged', onChange)
  return () => i18n.off('languageChanged', onChange)
}

export function useLanguage(): {
  language: Language
  resolved: string
  setLanguage: (language: Language) => void
} {
  // Re-render on every language change, wherever it was triggered from.
  const resolved = useSyncExternalStore(
    subscribe,
    () => i18n.resolvedLanguage ?? FALLBACK,
    () => FALLBACK
  )

  const setLanguage = (next: Language) => {
    try {
      window.localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // Nothing to do and nothing to say: the choice still holds for this visit.
    }
    void i18n.changeLanguage(resolveLanguage(next))
  }

  return { language: storedLanguage(), resolved, setLanguage }
}

/** The picker cycles through the three states in a fixed order. */
export function nextLanguage(current: Language): Language {
  const index = LANGUAGES.indexOf(current)
  return LANGUAGES[(index + 1) % LANGUAGES.length] ?? 'system'
}
