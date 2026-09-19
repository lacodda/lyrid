# 13. The interface is dowel, pinned dark, in two languages

Date: 2026-09-19

## Status

Accepted.

## Context

For eleven versions the interface was 853 lines of hand-written CSS and seven
hand-written components. That was the right shape while there were two screens
and the question was whether the renderer could draw two million stars at all.
It is the wrong shape now: the plan holds twenty more versions of interface
work — the fog of war, the observer's instruments, scrobbling, the radio — and
every one of them would start by writing another button.

Three things had to be settled together, because settling them apart means
rewriting the same five screens three times.

### Whose components

**Keep writing them here.** No dependency, no registry, nothing to follow. But
the line already went through this with kasl-server, and what it found was not
missing components: it was a hand-written theme that pinned dark ink on the
accent in both themes and measured 3.49:1 on the light one. Nobody notices that
by looking. A shared system checks it in CI.

**Stock shadcn/ui.** Its theme names colours by their role in a page —
`card`, `popover`, `muted` — where this product's chrome floats over a star
field and has no page. Every screen would translate between two vocabularies.

**dowel**, the line's own system: taken. Eighty-nine primitives in the
vocabulary the products already converged on, each one shipping with its own
gate — axe, the keyboard, a pointer-target floor, a picture in both themes. The
accent for lyrid already existed in its registry, derived from the mark's azure.

### Dark or both

dowel's theme follows `prefers-color-scheme` and pins with a class. Following
the system is right for the products it came from: kilna and kasl-server are
windows full of panels, and a person who reads everything else in a light theme
wants those light too.

lyrid is not that. The product is a night sky — the void behind the stars is the
design, not a background colour — and a light theme over it would be white
panels floating in space. Two readings were possible: take dowel's tokens but
not its theme switching (which means forking the theme), or use the mechanism
the theme already has for exactly this. The second: `class="dark"` on the root
element, which is a declaration and not a workaround.

### What language

English was the only language the interface spoke, hardcoded in the components.
The line's rule is that English is what ships; the question was when Russian
arrives. Now, at five screens, or later, at twenty-five.

## Decision

**The primitives come from the dowel registry, and the vocabulary from its
theme.** Twenty-two primitives are installed: the `app` set, the form fields,
and the six a screen here actually needed. `dowel doctor` reports the
installation coherent.

**Two tokens stay lyrid's own, and 853 lines of CSS go.** They are the two
things dowel has no word for, and both are named rather than written inline:

- `--void` is not `--bg`. dowel's ground is a near-black carrying a trace of the
  accent, correct for a panel on a page. The void is the colour of empty space
  between stars, and it is darker and bluer than any chrome would want to be.
- `--glass` is not `--raise`. `--raise` is opaque, because a panel in a document
  sits *on* a page. Here a panel sits on the map, and hiding the map behind it
  hides the product. It is mixed from `--raise` with `color-mix`, not written as
  its own hex, so it keeps following the accent's tint like the rest of the
  chrome. A `glass` utility carries it along with the blur and the hairline, so
  the pieces that float cannot drift apart.

A wish is filed with dowel for the second one: a surface that floats over
content is not unique to this product, and a third copy of it in the line would
be a token the line should own.

**Dark is pinned with `class="dark"` on `<html>`.**

**The interface speaks English and Russian from this version, i18next through
react-i18next — the same mechanism kilna uses, down to the locale gate.** Not a
smaller thing written here: the line has a working shape for this, and a second
translation mechanism in the same words is precisely the divergence the rule
against it exists to prevent.

English is the source language and **never a fallback**. `fallbackLng` is
`false`, and `web/tools/check-locales.mjs` runs in the lint gate: it holds every
locale to the source's shape, refuses a translation that drops or invents a
`{{placeholder}}`, and reads the `t('…')` calls in the source so a key the code
asks for and no locale has fails the build. A screen half in English is the kind
of defect nobody reports and everybody notices.

The canon is **not** in this layer. An artist's name, a genre, a Wikipedia
extract arrive in whatever language their source wrote them, and translating
them is not a question about the interface.

## Consequences

The five screens are written in tokens and primitives: no colour is named in
them, so they are correct in both themes and would follow the accent if it
moved. The charter's two-press delete became a `ConfirmDialog` — which, besides
looking like the rest of the line, announces itself to a screen reader as an
interruption rather than as a button whose label quietly changed.

The scale refuses sizes nobody argued for. Three places wanted an 8px type size
and there is no such step; they are 10px, which is the floor the line settled
on, and they are no worse for it.

The brand gate had to move. It read the header mark's drawn size out of
`styles.css`, and the size lives on the element now — a gate left pointing at
the old file would have gone on passing against a rule that is no longer there.
It reads `App.tsx` and was checked by mutation at the new location.

`dowel-ui` and Tailwind 4 are dependencies of the SPA, and the bundle grew from
what React alone cost. The primitives are copied into the repository rather than
imported, so they are this project's code and can be edited here; the cost of
that is that a fix in dowel arrives by re-running the install, not by bumping a
version.

Russian is one file. Every version from here adds to two locales instead of
writing one string, and the gate makes that not optional — which is the point of
doing it at five screens rather than at twenty-five.
