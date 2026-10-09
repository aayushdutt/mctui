// Sidebar: herdr's model applied to a launcher. Numbered instances down a
// slim left sidebar with herdr-style attention states; the selected one's
// page on the right, calm, becoming the live session while its game runs.

import { defineVariation } from '../../kit'
import { home } from './home'
import { mods } from './mods'
import { play } from './play'
import { wizard } from './wizard'

export default defineVariation({
  id: 'sidebar',
  overlays: ['help', 'confirm'],
  name: 'Sidebar',
  round: 2,
  order: 20,
  blurb:
    'herdr’s model for a launcher: a slim sidebar of numbered instances (1–9 jump) that only speaks up when something is true — running 18m, crashed, 42% — and the selected instance’s page beside it, which becomes the live session while its game runs. It trades the at-a-glance table (no versions or last-played column to compare) for calm, and a long list scrolls rather than fitting.',
  notes: [
    'The sidebar is the app’s spine and never leaves: every screen (mods, the log, a new instance) is a page beside it. Entries are two lines, herdr-style: `1 Survival 1.21` over a dim `1.21.4 · Fabric`, a blank row between entries. The selection tints the name row only, so line spacing never stripes it; the number takes the accent while the sidebar has focus.',
    'Attention, not decoration: the right edge of a name row is empty unless something is true — `● 18m` running (a live timer), `✗ crashed`, a spinner and `42%` downloading, `queued`, `▲ crashed` last time, `new`. The right edge is a scan column, so you never hunt for the stuck one. Too narrow for the word? It shrinks to its glyph before the name gives way.',
    '1–9 jump straight to an instance (on the game log, digits pick the level filter instead). Entries past 9 have no number because they have no key. `/` filters the sidebar in place, numbers kept.',
    'The page is content-first: name, `version · loader · played 2h ago`, the one primary action as a pill, then worlds (tab focuses them; ↵ plays straight into one), mods trouble-first, resource packs. Worlds and mods sit side by side when there’s room. Vanilla instances have no mods section at all.',
    'The page becomes the live session while its game runs: `● running 18m · 1 error · 3 warnings` and the log tail, following. ↵ lets the log take the page (play screen: level filter, thread column when wide). If a different instance is running, its page stays itself and one quiet line at the foot says so, with the key that shows it.',
    'Crash: a one-row red band (`✗ Crashed just now · exit code -1`), the cause as a sentence, the technical line muted, Try again ↵, and the mod list names the culprit and the fix. Downloading: spinner, sub-cell bar, sizes, and x to cancel. Expired sign-in: the account line at the sidebar’s foot turns amber, and the pill says what ↵ will do (Sign in & play).',
    'Help is a page (the sidebar stays), delete asks on one red row at the foot of the page. `s` stops the game, so settings are on `,`. No palette or menu: every action already has a key, so `:` and `x` just open the keys page.',
  ],
  screens: { home, mods, play, wizard },
})
