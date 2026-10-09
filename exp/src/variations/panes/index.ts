// Panes: the classic power-user TUI (lazygit, k9s, btop). Dense framed
// panels, everything visible at once, a one-row status bar. Frames are
// gap-safe: titled horizontal rules and closing rules, no vertical strokes.

import { defineVariation } from '../../kit'
import { home } from './home'
import { mods } from './mods'
import { play } from './play'
import { wizard } from './wizard'

export default defineVariation({
  id: 'panes',
  name: 'Panes',
  order: 10,
  blurb:
    'The power-user TUI (lazygit, k9s, btop): framed panels show instances, details, accounts and a live Activity feed at once, as dense tables over a one-row status bar. It trades calm and whitespace for density: the first glance is busier and every screen looks like a dashboard.',
  notes: [
    'Frames are gap-safe: a titled top rule (`── Instances 5 ────`) and a closing rule, no vertical strokes, no multi-row fills. Side-by-side panels are split by a 2-column gutter. Tabs, filters and counts live in the rules.',
    'Focus is the frame: the focused panel’s rules and title take the accent. Tab cycles panels; the selection band is accent-tinted in the focused panel and a quiet surface band in the others, so you always see what each panel points at.',
    'Everything tabular is a table: muted header row, numbers right-aligned, one glyph column for state (▲ crashed, ○ never played, ✓ installed, ↑ update).',
    'A status bar replaces header and footer: a mode badge (HOME, FILTER, RUNNING…), context, the keys that work now (help never drops off), the account on the right.',
    'Screens: home (instances + accounts, details with Worlds/Mods/Packs tabs, Activity; Java and disk at 140×40), mods (installed, queue, filters, Modrinth table, details), play (session strip, log table with level tabs, problems that jump the log), wizard (all three steps side by side). All four overlays restyled.',
  ],
  screens: { home, mods, wizard, play },
})
