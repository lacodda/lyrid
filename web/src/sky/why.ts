/**
 * An edge turned into words: why two stars are near each other.
 *
 * One function for the card's neighbours and for the comparison, so the same
 * pair is never explained two ways on two screens. The sentences are the
 * translator's; this only decides which of them apply.
 */

import type { useTranslation } from 'react-i18next'

import type { Why } from '@/api'

type Translate = ReturnType<typeof useTranslation>['t']

/**
 * The reasons, strongest first: influence is a claim about history and says
 * the most, shared genres say what the two sound like, and co-listening —
 * true of every neighbour by definition — comes last and only when it is the
 * sole reason, so a list of neighbours does not repeat it ten times.
 */
export function reasons(why: Why, t: Translate): string[] {
  const out: string[] = []
  if (why.influence === 'shaped_by') out.push(t('why.shapedBy'))
  if (why.influence === 'went_on_to_shape') out.push(t('why.wentOnToShape'))
  if (why.influence === 'mutual') out.push(t('why.mutual'))
  if (why.genres.length > 0) out.push(t('why.genres', { list: why.genres.join(', ') }))
  if (out.length === 0 && why.co_listening !== null) out.push(t('why.coListening'))
  return out
}
