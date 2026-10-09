# mctui design lab

A local playground (not part of the build, never deployed) for brainstorming how mctui's
TUI could look. **Read [UX.md](UX.md) first**: who uses mctui, what for, and
the TUI mindset every direction starts from. Mockups draw into a **terminal cell buffer**, not HTML, so
anything designed here is something ratatui can actually draw: one glyph and
one fg/bg per cell, nothing between rows.

```sh
cd exp
pnpm install
pnpm dev            # http://localhost:5178
pnpm check          # type-check
pnpm shot <variation|all> [screen|all] [--scenario running] [--theme dark] [--size 100x30] [--ui '{"sel":2}'] [--suffix x] [--text]
                    # → shots/<variation>-<screen>.ansi + .png (no browser needed)
```

`cat shots/<name>.ansi` in a truecolor terminal shows a mockup for real. In
the browser, "Copy as ANSI" does the same via the clipboard.

## What's here

| Path | What |
|---|---|
| `src/term/buffer.ts` | `Buffer` (cells, `text`, `line`, `fill`, `box`, `hline`, `region`…) and layout (`rect`, `cols`, `rows`, `inset`, `centered`, `wrap`, `truncate`) |
| `src/term/theme.ts` | The app's six palettes, resolved exactly like `theme.rs` (`surface`, `selection`… mixed from the terminal bg), plus `mix`, `onColor` |
| `src/term/widgets.ts` | The app's components (DESIGN.md): `section`, `band`, `button`, `chip`, `tabs`, `field`, `search`, `toggle`, `choice`, `progress`, `kv`, `stats`, `keycaps`, `page`, `header`, `card`, `toast` |
| `src/term/Screen.tsx` | DOM renderer; draws box/block glyphs itself; **Line spacing** mode mimics the user's terminal |
| `src/kit.ts` | `Variation`, `Ctx`, `Ui` (interaction state), the screen list |
| `src/keys.ts` | The key/click grammar that drives mockups (↑↓ tab ←→ / ? : x d n m a s p ↵ esc) |
| `src/data/mock.ts` | Shared content: instances, worlds, mods, Modrinth hits, versions, log, accounts, settings |
| `src/data/scenarios.ts` | States to design for: idle, running (`data.running`), downloading (`data.tasks`), crashed (`data.crash`), expired sign-in, first run, many instances |
| `src/variations/<id>/index.ts` | One design direction each. Auto-discovered. |
| `src/lint.ts` | Flags vertical strokes and multi-row tints (what line spacing breaks) |
| `../target/screenshots/*.ansi` | The live app's frames (`make screenshots` in the repo root), shown under "Live app" |

## Adding a direction

1. Copy `src/variations/baseline/` to `src/variations/<id>/` and rename `id`/`name`.
2. Each screen is `(c: Ctx) => void`: draw into `c.buf` with colours from `c.t`
   and content from `c.data`. Screens: `home`, `mods`, `wizard`, `play`,
   `accounts`, `settings` — skip any you don't want; the lab shows a placeholder.
3. Make it interactive for free: `c.sel(n)` / `c.osel(n)` / `c.tabs(n)` /
   `c.panes(n)` return the current selection/tab/focus clamped to your list
   sizes; `buf.region(rect, 'sel:3')` makes rows clickable; `c.hovered(key)`
   lights things up; `c.ui.overlay` is `help` / `palette` / `menu` / `confirm`
   when the user opens one; `c.ui.query` is the `/` search text.
4. `pnpm shot <id> all` and look at the PNGs (also `--size 80x24`, `--size 140x40`,
   `--theme light`).

## The terminal we design for

- **Cells only.** Monospace grid, one glyph per cell. No emoji, no Nerd Font
  icons (not everyone has them). Box drawing, block elements (`▀▄█▌▐░▒▓`),
  braille, geometric shapes (`●○◆▲▶`) and arrows are fine.
- **Line spacing.** The user's terminal adds space between rows and paints
  neither backgrounds nor vertical strokes into it. So a background filling
  two+ rows shows stripes, and `│ ┃ ╭ ╮` columns show as dashes. The lab's
  default "Line spacing" mode (and `pnpm shot`'s PNGs) render exactly that, and
  lint counts it. A direction may still use boxes on purpose — then say so, and
  check it in both spacing modes.
- **Theme-driven colour.** Use `c.t.*` roles, never hardcoded hex, so every
  palette works (the lab switches between dark, light, gruvbox, catppuccin,
  dracula, nord). Tints come from `mix(t.bg, colour, amount)`.
- **Sizes.** 100×30 is the reference; it should still work at 80×24 and use
  the room at 140×40.
- **Keyboard and mouse are equals.** The real app is fully clickable.
