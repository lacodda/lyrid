import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { LANGUAGES, nextLanguage, useLanguage, type Language } from '@/lib/language'

/**
 * Which language the interface is in, as one button.
 *
 * A button that cycles rather than a select, because there are three states and
 * the list will not grow much: a dropdown for three options is a menu to open
 * before the choice can be seen. It shows what is showing now, and pressing it
 * moves to the next one — the shape kilna's theme button already has.
 *
 * `system` names itself rather than naming what it resolved to. "system" and
 * "English" are different answers: the first says the choice has not been made,
 * and replacing it with the second would make an unmade choice look made.
 */
export function LanguagePicker() {
  const { t } = useTranslation()
  const { language, setLanguage } = useLanguage()

  return (
    <Button
      size="sm"
      className="glass"
      // The button's own label says what it is; the text inside says the state.
      // Without this it announces as just "system", which is not a control.
      aria-label={`${t('language.label')}: ${label(language, t)}`}
      title={t('language.label')}
      onClick={() => setLanguage(nextLanguage(language))}
    >
      {label(language, t)}
    </Button>
  )
}

function label(language: Language, t: ReturnType<typeof useTranslation>['t']): string {
  // The keys are the language names, so the list and the catalogue cannot
  // drift: a language added to `LANGUAGES` without a name fails the locale gate
  // rather than rendering its own key on screen.
  return LANGUAGES.includes(language) ? t(`language.${language}`) : t('language.system')
}
