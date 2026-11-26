# 31. Sheets on a narrow screen

Date: 2026-11-25

## Status

Accepted.

## Context

The interface is a sky that fills the window with chrome floating over it: a
column on the left (the observer's instruments, the nearest stars, the account,
the halo, sharing), a column on the right (the card of the picked star, what is
marked, the route, the compass, the player), and the search box above it.

That is two columns of 22 rem, and a phone is one column wide. Since v0.12 the
card simply lay over the left column there; v0.13 hid the instruments and the
compass at that width so they would not bury the sky, and v0.14 hid the idle
player the same way. A phone therefore had no era lens, no time machine, no
compass, no radio to start, and a card that covered the account -- and no way
to zoom at all, because zoom was the wheel.

Two things constrain any arrangement:

- **The player is a YouTube embed.** Its terms want a viewport of at least 200
  by 200 that stays in view while it plays, and an `iframe` moved to another
  place in the document loads again -- the song starts over.
- **The sky is where the next star is picked.** Anything modal over it stops
  the exploration the product is.

## Decision

Below Tailwind's `md` (48 rem) the same pieces are arranged another way.
`web/src/sky/narrow.ts` holds the boundary and the arithmetic.

**The card is a bottom sheet, and so is the left column.** The card rises from
the bottom edge when a star is picked. Everything the left column held, with
the compass, is behind one button in the corner -- "Menu" -- and rises as a
second sheet. One sheet at a time: picking a star puts the menu away.

**A sheet is dowel's drawer without the modal half.** `web/src/sky/Sheet.tsx`
is built from the same Base UI drawer parts as the line's `DrawerPopup` and
wears its look (`drawerPopupVariants`, bottom side), but with `modal={false}`,
no scrim, and a viewport that takes presses only on the sheet itself. The
line's `DrawerPopup` frames the whole window and dims it, which is right for a
form and wrong over a map: the sky above the card stays live, so the next star
can be picked with the card open, and the instruments in the menu change a sky
the person can see while changing them. A sheet is closed by its button, a
swipe down, or Escape -- not by a press on the sky, which is a pan.

**The player sits under the search box, and a sheet stops short of it.** The
video is above and what is being read is below, the arrangement a phone's
video applications taught everyone. A sheet's height is at most 60% of the
window and never reaches the bottom of the pieces at the top (`sheetHeight`),
so the player is never covered by the card whose button started it. The floor
of 224 px wins only on a window too short for both.

**The right column is one set of elements at both widths.** The player, the
panel of what is marked and the route are the same elements, moved from the
bottom-right corner to under the search box by classes. Not a second
arrangement rendered for phones: turning a phone on its side crosses the
boundary, and a second arrangement would be a second player.

**A flight ends above the sheet.** A star flown to lands in the middle of the
window, which on a phone is under its own card. On the narrow layout the
camera is aimed so the star lands in the strip between the top pieces and the
sheet (`aboveSheet`), and a star picked by a press -- which on a wide window
does not move the view at all -- moves it just that far.

**Two fingers zoom.** A pinch keeps the sky under the middle of the two
fingers under it (`web/src/sky/touch.ts`), which is pan and zoom in one rule,
bounded as the wheel is.

What the expedition asks next -- the first hint in the fog and the contract in
hand -- stays on the sky above the menu button instead of going into the menu:
they are calls to act, not tools, and behind a button a new explorer would open
a dark sky with nothing said about it.

## Consequences

- A phone has everything a wide window has. Nothing is hidden by width any
  more; `hidden md:flex` is gone from the sky's chrome.
- With the player playing and a card open, a phone shows no sky: video above,
  card below. Closing either gives it back. That is a phone's size, not a
  defect of the arrangement.
- `Sheet.tsx` is lyrid's own assembly of the drawer. When dowel's drawer learns
  to be non-modal, the sheet becomes that and this file goes.
- A phone on its side is wider than the boundary and gets the wide layout in a
  window some 390 px tall, where the columns are cramped. Short-and-wide is not
  arranged for yet.
- The sheet's height is measured from the elements (a `ResizeObserver` on the
  window and the top column), not known from constants: the player is two
  heights, and a route can join it.
