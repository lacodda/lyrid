import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { Button } from '@/components/ui/button'
import { SectionLabel } from '@/components/ui/panel'
import { HALO_SHAPES, type HaloShape } from './renderer'

/**
 * A scratch panel for choosing how the marked star is drawn.
 *
 * Deliberately temporary: this is here to settle a look by eye, and it belongs
 * in settings once there are settings to put it in. It is kept small and
 * self-contained so moving it later costs nothing — no state above it, no
 * styling anything else depends on.
 */

/** The mark's own azure, and the default: the marker belongs to the product. */
export const HALO_DEFAULT = { name: 'azure', rgb: [0.55, 0.78, 1.0] as [number, number, number] }

/** The colours offered, as linear RGB with a name to press. */
export const HALO_COLOURS: { name: string; rgb: [number, number, number] }[] = [
  HALO_DEFAULT,
  { name: 'gold', rgb: [1.0, 0.84, 0.45] },
  { name: 'rose', rgb: [1.0, 0.55, 0.7] },
  { name: 'mint', rgb: [0.5, 1.0, 0.78] },
  { name: 'violet', rgb: [0.72, 0.6, 1.0] },
  { name: 'white', rgb: [0.95, 0.96, 1.0] },
]

interface Props {
  shape: HaloShape
  colour: [number, number, number]
  onShape: (shape: HaloShape) => void
  onColour: (colour: [number, number, number]) => void
}

export function HaloPicker({ shape, colour, onShape, onColour }: Props) {
  const { t } = useTranslation()

  return (
    // Dashed on purpose: this is scratch, and the border says so. A solid
    // panel would be a claim that the arrangement has been decided.
    <div className="glass flex flex-col gap-1 border-dashed p-2">
      <SectionLabel>{t('halo.label')}</SectionLabel>

      <div className="flex flex-wrap gap-1">
        {HALO_SHAPES.map(option => (
          <Button
            key={option}
            size="sm"
            variant={option === shape ? 'soft' : 'ghost'}
            // The shapes are a closed set in the renderer, so their names are
            // keys rather than free text.
            onClick={() => onShape(option)}
          >
            {t(`halo.shape.${option}`)}
          </Button>
        ))}
      </div>

      <div className="flex flex-wrap gap-1">
        {HALO_COLOURS.map(option => (
          <button
            key={option.name}
            type="button"
            className={cn(
              'size-5 cursor-pointer rounded-full border border-line',
              sameColour(option.rgb, colour) && 'border-accent ring-1 ring-accent'
            )}
            style={{ background: css(option.rgb) }}
            title={t(`halo.colour.${option.name}`)}
            aria-label={t(`halo.colour.${option.name}`)}
            aria-pressed={sameColour(option.rgb, colour)}
            onClick={() => onColour(option.rgb)}
          />
        ))}
      </div>
    </div>
  )
}

function sameColour(a: [number, number, number], b: [number, number, number]): boolean {
  return a[0] === b[0] && a[1] === b[1] && a[2] === b[2]
}

function css([r, g, b]: [number, number, number]): string {
  const byte = (value: number) => Math.round(value * 255)
  return `rgb(${String(byte(r))}, ${String(byte(g))}, ${String(byte(b))})`
}
