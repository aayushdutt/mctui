// Refined: today's model (instances left, the selected one's sheet right),
// rebuilt with the TUI mindset. Compact aligned rows, a composed preview with
// no tabs, state only while it's true, and craft instead of decoration.

import { defineVariation } from '../../kit'
import { home } from './home'
import { mods } from './mods'
import { play } from './play'
import { wizard } from './wizard'

export default defineVariation({
  id: 'refined',
  name: 'Refined',
  round: 2,
  order: 20,
  blurb:
    'Baseline’s model rebuilt as a terminal table: one-line instance rows in aligned columns with a state column that stays empty unless something is true, and a tab-free sheet that composes state, worlds and mods in priority order. It gives up the baseline’s roomy, button-led look for density and quiet: fewer pills, more text, and the magic is in alignment and live detail.',
  notes: [
    'Home: most recently played first, so ↵ plays the last one with zero decisions. Columns hug their content (name · version · state · last played; loader and playtime join at 140 cols). When narrow, the state word takes the time’s place. Older “last played” reads dimmer: recency as brightness.',
    'State column: running (green) · crashed (red, amber if it was last time) · new · queued · a sub-cell bar and % while downloading. The launched row becomes the running row.',
    'The sheet has no tabs: name, version · loader, one state line, then Worlds and Mods (what needs you first: needs 0.112+ / ↑ 0.6.5 / off, then the fine ones). It trims and then drops sections by priority to fit, and lists every mod when there’s height. The one affordance sits top right: ↵ play · ↵ log · ↵ try again · ↵ sign in & play.',
    'A status line above the footer exists only while a game runs, crashed, or something downloads. It drops parts by priority instead of clipping, and shows keys only when its subject isn’t the selected row. The account is a quiet name; an expired sign-in turns it into one amber pill plus `a sign in`.',
    'Crash: what broke in one sentence (fitted to the width: “0.112 or newer” becomes “0.112+” rather than wrapping), the technical cause faint, and a specific next key: `m update Fabric API`. Mods marks the blamed mod in red.',
    'Mods: installed first, grouped (to fix, to update, fine, off) a blank row apart; `/` turns the same list into a Modrinth search with lit matches and ✓ for installed. Game: state line on top, level filter as quiet words, follows the end, `/` finds. New instance: the form always shows what ↵ ↵ ↵ makes (26.3 · Vanilla · “26.3”, numbered if taken); versions list who uses them.',
    'Overlays stay small: delete is a one-row question in the status line, `x` opens a menu beside the row, `:` is a prompt in the footer row, help is a bottom sheet.',
  ],
  overlays: ['help', 'palette', 'menu', 'confirm'],
  screens: { home, mods, wizard, play },
})
