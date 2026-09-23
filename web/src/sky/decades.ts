/**
 * Time on the sky: the decade lens and the time machine.
 *
 * Both read one number per star — the year its act began, carried in every
 * tile record since format 2 — and both are drawn by the shader. The palette
 * lives here, once, and the shader receives it as a uniform array: the legend
 * under the lens and the colours on the sky are the same list, so they cannot
 * drift apart.
 */

/**
 * The eras the lens colours, by their first year. Everything before the first
 * boundary shares the first colour; everything after the last shares the last.
 *
 * Seven, not thirteen decades. Time is ordered, so the lens is an ordered
 * ramp — one hue family, light to dark — and an ordered ramp only reads when
 * neighbouring steps differ visibly in lightness. Measured with the line's
 * palette validator against the sky's own background: thirteen steps are 0.02
 * apart, far under the 0.06 an eye needs, and seven are the most that clear it.
 * The decades cut are the ones where the canon is thin; from the sixties on,
 * where most of it lives, every decade keeps its own step.
 */
export const ERAS: readonly number[] = [0, 1960, 1970, 1980, 1990, 2000, 2010]

/**
 * One colour per era, oldest first: amber to pale azure in OKLab, lightness
 * rising 0.50 to 0.95 — the older, the deeper. Passes the validator's ordinal
 * checks on the sky's background (#07080d): monotone lightness, every step at
 * least 0.06 apart, the darkest at 3.2:1 against the void.
 *
 * Linear RGB would be the shader's native space; these are sRGB, as the
 * validator reads them and as the legend's CSS draws them, and the field's own
 * star colour is written in the same space.
 */
export const ERA_COLOURS: readonly (readonly [number, number, number])[] = [
  [0.567, 0.317, 0.079], // #915114 — before 1960
  [0.691, 0.368, 0.312], // #b05e50 — 1960s
  [0.779, 0.444, 0.531], // #c77187 — 1970s
  [0.825, 0.545, 0.75], // #d28bbf — 1980s
  [0.829, 0.666, 0.953], // #d3aaf3 — 1990s
  [0.796, 0.804, 1.0], // #cbcdff — 2000s
  [0.743, 0.949, 1.0], // #bef2ff — 2010s and since
]

/**
 * The colour of a star whose year the canon does not know. Grey, and dimmer
 * than every era: the one colour that means "the canon does not say".
 */
export const UNDATED: readonly [number, number, number] = [0.36, 0.38, 0.42]

/** Which era a year falls into; -1 when it is undated. */
export function eraIndex(year: number): number {
  if (year <= 0) return -1
  let index = 0
  for (let i = 1; i < ERAS.length; i++) {
    if (year >= (ERAS[i] ?? Infinity)) index = i
  }
  return index
}

/**
 * The palette flattened for `gl.uniform3fv`, eras then undated, and the era
 * boundaries for `gl.uniform1fv`. The shader walks the boundaries with the
 * same rule as `eraIndex`; the test beside this file pins the two together.
 */
export function paletteUniform(): Float32Array {
  return new Float32Array([...ERA_COLOURS.flat(), ...UNDATED])
}

export function boundariesUniform(): Float32Array {
  return new Float32Array(ERAS)
}

/** A colour as CSS, for the legend. */
export function css([r, g, b]: readonly [number, number, number]): string {
  return `rgb(${String(Math.round(r * 255))} ${String(Math.round(g * 255))} ${String(Math.round(b * 255))})`
}

/** The slider's range: the first decade the canon holds in any number, to now. */
export function timeRange(now = new Date()): { min: number; max: number } {
  return { min: 1900, max: now.getFullYear() }
}

/**
 * How bright a star is at a moment of the time machine, as a multiplier.
 *
 * Zero before its act began: the star is not there yet. A flare in the few
 * years after it does — the "star lights up in its founding year" of the bank's
 * idea 13 — settling to its ordinary brightness. An undated star is drawn faint
 * rather than hidden or shown in full: hiding it would erase much of the
 * canon's long tail, and showing it at full strength would claim it belonged to
 * every year.
 *
 * The shader carries the same curve; this copy is what the tests hold it to.
 */
export function timeWeight(year: number, until: number): number {
  if (year <= 0) return UNDATED_WEIGHT
  if (year > until) return 0
  const age = until - year
  return 1 + FLARE * Math.exp(-age / FLARE_YEARS)
}

/** How faint an undated star is under the time machine. */
export const UNDATED_WEIGHT = 0.18
/** How much brighter a star is in the year it appears. */
export const FLARE = 1.4
/** How many years the flare takes to fade to a third. */
export const FLARE_YEARS = 2.5
