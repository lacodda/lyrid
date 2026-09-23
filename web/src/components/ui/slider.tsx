import { Slider as Base } from '@base-ui/react/slider'
import { cn } from 'dowel-ui'

/*
 * Slider - a value picked by position, and a range picked by two.
 *
 * The case for it over a NumberField is that the number does not matter much:
 * a volume, an opacity, a weight in a search filter. Where the exact figure
 * does matter, a slider is a worse field with more pixels - it cannot be
 * typed into, it cannot be pasted into, and it has no state for "empty".
 *
 * Range is the same component with an array. That is Base UI's arrangement
 * and it is the right one: a range slider is not a second control but the
 * same track with two thumbs, and splitting them would double the styling
 * and let the two drift apart. `value={[10, 40]}` is a range; `value={30}` is
 * a single.
 *
 * The parts are separate for a reason worth knowing. The Control is the whole
 * hit area - much taller than the visible track, so a pointer does not have
 * to find four pixels - while the Track is what is drawn, and the Indicator
 * is the filled part behind the thumb. Making the drawn track the hit area is
 * the commonest way a slider ends up hard to grab.
 */

export interface SliderProps {
  /** A number for one thumb, an array for a range. */
  value?: number | readonly number[]
  defaultValue?: number | readonly number[]
  /* `readonly` because that is what Base UI hands over, and narrowing it here
   * only moves the cast into every caller. */
  onValueChange?: (value: number | readonly number[]) => void
  /** Fires once when the drag ends, for the expensive thing a product does
   * not want to run on every pixel of movement. */
  onValueCommitted?: (value: number | readonly number[]) => void
  min?: number
  max?: number
  step?: number
  /** How close the thumbs of a range may come, in steps. */
  minStepsBetweenValues?: number
  /** How the value reads when it is shown: `Intl.NumberFormat` options. */
  format?: Intl.NumberFormatOptions
  orientation?: 'horizontal' | 'vertical'
  /** Show the current value beside the track. Off by default: on a row of
   * settings the numbers are noise, and where the figure matters a
   * NumberField is the better control. */
  showValue?: boolean
  /** What each thumb is called, by index.
   *
   * The thumb is the control - a hidden `<input type="range">` - and
   * `aria-label` on the root names the *group* around it, which leaves every
   * thumb unnamed. axe reports it for a single slider as loudly as for a
   * range. Without this prop the group's own label is used for each thumb,
   * which is right for one and merely adequate for two: a range wants
   * "Lowest price" and "Highest price", not the same word twice. */
  getThumbLabel?: (index: number) => string
  disabled?: boolean
  name?: string
  'aria-label'?: string
  className?: string
}

export function Slider({ showValue = false, getThumbLabel, className, ...props }: SliderProps) {
  const vertical = props.orientation === 'vertical'

  /* Every thumb needs a name of its own, so the group's label is the fallback
   * rather than nothing. A slider whose only name sits on the group is one
   * where a reader lands on the control and is told a number. */
  const groupLabel = props['aria-label']
  const nameThumb = getThumbLabel ?? (groupLabel === undefined ? undefined : () => groupLabel)

  /* One thumb per value. Base UI addresses them by index, so a range needs as
   * many as the array is long - a single Thumb on a range renders one handle
   * that moves the first value and leaves the second unreachable. Read from
   * whichever of the two props is present, because the component is useful
   * both controlled and not. */
  const current = props.value ?? props.defaultValue ?? 0
  const thumbCount = Array.isArray(current) ? current.length : 1

  return (
    <Base.Root
      {...props}
      className={cn(
        'flex items-center gap-3',
        vertical && 'h-40 flex-col',
        !vertical && 'w-full',
        className,
      )}
    >
      <Base.Control
        className={cn(
          // The hit area, deliberately larger than what is drawn: a four-pixel
          // target is a four-pixel target however pretty the track is.
          'flex touch-none items-center',
          vertical ? 'h-full w-5 justify-center' : 'h-5 w-full flex-1',
        )}
      >
        <Base.Track
          className={cn(
            'relative rounded-full bg-soft',
            vertical ? 'h-full w-1.5' : 'h-1.5 w-full',
          )}
        >
          <Base.Indicator className={cn('rounded-full bg-accent', vertical ? 'w-full' : 'h-full')} />
          {Array.from({ length: thumbCount }, (_, index) => (
            <Base.Thumb
              key={index}
              index={index}
              getAriaLabel={nameThumb}
              className={cn(
                'size-4 rounded-full bg-accent shadow-lift',
                'transition-[box-shadow,transform] duration-quick ease-out',
                'hover:scale-110',
                'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
                'data-[dragging]:scale-110',
              )}
            />
          ))}
        </Base.Track>
      </Base.Control>

      {showValue && (
        <Base.Value className="shrink-0 text-xs tabular-nums text-dim" />
      )}
    </Base.Root>
  )
}
