// Ledger: the table model of htop, k9s and `ls -l`. Every screen is one
// aligned table over a short drawer about the selected row.

import { defineVariation } from '../../kit'
import { home } from './home'
import { mods } from './mods'
import { play } from './play'
import { wizard } from './wizard'

export default defineVariation({
  id: 'ledger',
  name: 'Ledger',
  round: 2,
  order: 20,
  blurb:
    'Home is one aligned table of instances (htop, k9s, `ls -l`) with a STATE column you scan like a process list, and a short drawer under a rule about the selected row. It scales to 15+ instances and reads at a glance; it trades away the roomy, friendly detail pane — everything is a row first.',
  notes: [
    'One table, one row per instance: quiet header, numbers right-aligned, NAME as wide as the longest name. Columns drop by priority as the terminal narrows (WORLDS, then PLAYED at 80 cols) and JAVA/MEMORY appear at 124+. Spare width opens the gaps evenly instead of leaving one hole.',
    'STATE is the process-list column: `ready` is faint, so only what matters lights up — `● running 18m` (and LAST says `now`), `▲ crashed`, `✗ crashed`, `new`, `queued`, and a sub-cell download bar (`███▍  42%`). A `↑` beside MODS marks pending updates. No glyphs anywhere else.',
    'Sorted by LAST by default, so the instance you played last is on top and selected: open, ↵, play. `s` cycles the sort (also ←→, or click a header); the sorted header is bright with ↓/↑.',
    '`/` turns the footer into a prompt (`/fa  7 of 14`) — fuzzy over name, then loader, then version, matched letters lit in whichever column matched. The table is sized for every instance, so filtering never moves the drawer.',
    'The drawer hangs one rule under the table; the rule carries the selected name, its state and `↓ 4 more`. Labels are right-aligned like `systemctl status`: attention first (crash: what broke in one sentence + the keys that fix it; expired sign-in; updates), then worlds, mods, java. Running: session, problems, and the last log lines, tail-style.',
    'Overlays never float: help, delete, stop and the actions menu take the drawer’s place under the same rule; `:` is a command line in the footer. The table stays readable above.',
    'Mods is the same table: installed first (outdated on top, the crash’s culprit in red, disabled rows dimmed); `/` swaps it to Modrinth results (NAME, AUTHOR, CATEGORY, DOWNLOADS, `✓ installed`). The log is a TIME/LEVEL/MESSAGE table that follows the tail, the level filter on its header row, the crash cause on the one tinted row. The wizard is three tables whose defaults are pre-selected, with a drawer showing what ↵ ↵ ↵ will make.',
    'Extra content (ledger/data.ts): three more installed mods for Survival (and Fabric API 0.112.1, which the crash scenario says Sodium needs), eight more Modrinth hits, a crash log tail.',
  ],
  screens: { home, mods, wizard, play },
})
