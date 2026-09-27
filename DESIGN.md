# mctui design

How mctui looks and behaves. Every screen is built from the components in
`crates/mctui/src/ui/`; if something here can't be expressed with them, add a
component rather than drawing it by hand.

## Principles

- **Quiet chrome, loud content.** No boxes, no borders, no rules between panes.
  Structure comes from whitespace, alignment and a few single-row tints.
- **One accent.** The accent means *you are here, this is the main action, this
  is a key*. Green, amber and red only ever mean state. Everything else is a
  step on the grey ramp.
- **Every row is one line.** Background tints (selection, chips, buttons,
  fields, toasts) fill exactly one row: terminals with line spacing split
  anything taller into stripes, and never paint vertical strokes (`│ ┃`).
- **Say it once.** A title isn't repeated as a label, a count isn't repeated in
  a hint, and a footer only shows keys that work right now.
- **Keyboard and mouse are equals.** Anything you can press you can click; what
  the pointer is over lights up; the wheel scrolls what's under it.

## Colour

A theme is a palette resolved against the terminal's real background
(`theme.rs`), so tints sit right on any background.

| Role | Used for |
|---|---|
| `accent` | focused titles, keys, the primary button, filter matches |
| `strong` | titles, selected text, values in stat tiles |
| `text` | body text, names in lists |
| `subtle` / `dim` | secondary copy, metadata, labels |
| `muted` | counts, hints, section labels |
| `faint` | decoration, "↓ 3 more", empty-state copy |
| `surface` | keycaps, inactive chips, fields, hover band |
| `surface_hi` | a hovered chip or button |
| `selection` | the focused selection band, the active chip in a focused pane |
| `success` / `warning` / `error` (+ `_bg` tints) | state, and only state |

## Layout

- The page has 1 row of margin top and bottom and 2 columns left and right.
- **Header** (row 1): the `mctui` wordmark, then the page's breadcrumbs
  (`mctui › Survival 1.21 › Mods`; home shows no crumb). The right side holds
  context (the signed-in account).
- One blank row, then the **body**, then any **notices** and the **toast**,
  one blank row, and the **footer** of keycaps.
- Panes sit side by side with a 4-column gutter and no divider. Each pane
  starts with a **section label**, then a blank row, then content inset 2
  columns (bands run the full pane width; text sits inside them).
- Below 76 columns, two-pane screens collapse to one pane plus a summary line.
- Vertical rhythm: one blank row between groups, never two.

## Components

| Component | Look | Notes |
|---|---|---|
| Section label | `INSTANCES   6` — uppercase, muted, bold; count muted | accent when its pane has focus |
| List row | one line: name left, one accessory right (time, size, state) | blank line between rows; `↑ 2 more` / `↓ 3 more` when scrolled |
| Selection | `selection` band across the row, text strong + bold | only in the focused pane |
| Hover | `surface` band | never moves keyboard focus onto a destructive choice |
| Button | ` ▶ Play  ↵ ` on the accent (primary) or ` Play offline  o ` on `surface` | the key sits inside, quieter; one primary per view |
| Chip | ` Mods 3 ` on `surface`; active = `selection` | tabs, filters, status (`● Ready` on `success_bg`) |
| Keycap | ` ↵ ` on `surface`, bold, then the label dim | every footer hint, clickable |
| Field | a `surface` band with the text and the cursor | focused = `selection` band; placeholder muted |
| Toggle | ` On ` chip (success) / ` Off ` chip (surface) | space or click flips it |
| Choice | ` ‹ catppuccin › ` chip | ←→ or click cycles |
| Stat tile | value (strong, bold) over a label (muted) | tiles share the width, wrap when narrow |
| Key/value | label column (dim, 16 wide) then value (text) | forms and details |
| Progress | `━━━━━━━──────  45%` accent over `rule` | one row; never goes backwards |
| Spinner | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` in the accent, 80 ms a frame | only while work is running |
| Toast | one tinted row above the footer: `✓ Deleted “X”.` | 4 s, newest replaces oldest |
| Card | cleared area, bold title over a rule, body, closing rule | help, palette, menus, dialogs; the screen behind dims |
| Dialog | card + message + the two choices as buttons | opens on the safe choice; danger = red title and red Delete |

Glyphs: `●` state · `○` pending · `✓` done · `✗` failed · `▲` warning ·
`↓` not downloaded · `▶` play · `↵` enter. No emoji, no Nerd Font icons.

## Interaction grammar

- `↑↓` / `jk` move within the focused pane and stop at the ends. `←→` / `hl`
  and `tab` / `shift+tab` move between panes, tabs and fields.
- `↵` is the one affirmative action and never removes anything. `space`
  toggles. `d` / `⌫` remove, always with a confirmation. `r` retries.
- `esc` steps back one layer: overlay → field or filter → running task
  (cancelled in place) → screen. Forms ask before discarding edits.
- `?` help and `:` palette work on every screen; `/` filters lists. `q` quits
  from home only; `ctrl+c` quits anywhere, asking again while a game runs.
- Text fields keep letters, `?`, `:` and `q` for typing, with readline editing
  (`ctrl+a/e/k/u/w/y`, `alt+b/f`).
- Mouse: click selects and focuses, double-click activates a row, right-click
  opens the row's actions at the pointer, the wheel scrolls what's under it,
  clicking outside a card closes it.

## Copy

Sentence case. Say what happened and what to do next: "Couldn't reach
Modrinth — check your connection, then press r." Name things the way players
do (instance names, mod titles), never file names or internal ids.
