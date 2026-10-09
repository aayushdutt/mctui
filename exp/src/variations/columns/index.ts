// Columns: Miller columns (ranger, yazi, lf, macOS column view) over
// Instances → contents → items → detail. The same four keys reach everything.

import { defineVariation, type Ctx } from '../../kit'
import { browse } from './browse'
import { wizard } from './wizard'

function home(c: Ctx) {
  browse(c)
}

/** Home, drilled into the first instance's Mods. */
function mods(c: Ctx) {
  const inst = c.data.instances[0]
  if (!inst) return browse(c)
  browse(c, { kind: 'mods', inst })
}

/** Home, drilled into the Game log of the game you just started (or the one that crashed). */
function play(c: Ctx) {
  const { data } = c
  const name = data.running?.instance ?? data.crash?.instance
  const inst = data.instances.find((i) => i.name === name) ?? data.instances[0]
  if (!inst) return browse(c)
  const running =
    data.running ??
    (data.crash
      ? undefined
      : { instance: inst.name, elapsed: '18m', since: '14:02', world: inst.worlds[0]?.name ?? 'New World', warnings: 3, errors: 1, memory: '2.1 of 4 GB' })
  browse({ ...c, data: { ...data, running } }, { kind: 'log', inst })
}

export default defineVariation({
  id: 'columns',
  name: 'Columns',
  round: 2,
  order: 20,
  blurb:
    'Miller columns (ranger, yazi, macOS column view): Instances → what’s in the selected one → a preview of the row you’re on. ←→ go out and in, ↑↓ move, ↵ plays; Mods and the game log are just deeper columns. It trades a dashboard’s at-a-glance breadth for one path at a time, and asks a player to think of their instances like folders.',
  notes: [
    'Three fixed slots, separated by whitespace and a dim title each (no rules, no boxes). The focused column’s title takes the accent and its row the accent band; columns left of focus dim and keep their row on a neutral band (the path); the row a preview follows is bold. Everything is one row tall.',
    'Levels slide through the slots like ranger: Instances → contents (Game log, Worlds, Mods, Resource packs, Settings) → items → detail. → from Mods is the mods screen; → from Game log is the play screen. The header is the path as a breadcrumb, clickable.',
    'State, not furniture: the instances column trails one word (running, crashed, ⠋ 42%, queued, new, else when it was last played). While a game runs or after a crash, its Game log is pinned to the top of the contents, so the preview beside it is the live log or the reason it crashed, with no key pressed. One status line above the footer says what’s happening; it’s blank when nothing is.',
    'Play: the log is a file, so opening it gives it the room of two slots, the contents column stays as the way back. At 80×24 the log takes the whole width.',
    'Mods: installed mods A→Z, version right-aligned with a two-cell state column (↑ update, ✗ broken, off); `+ Add from Modrinth` closes the list like `+ New instance` does. `/` searches installed and Modrinth in one grouped list. The detail says what ↵ does precisely (“updates to 0.112, which fixes Sodium”, “installs it for 1.21.4 · Fabric”); d remove is in the footer.',
    'New instance is columns too: Version → Loader → Name. Old versions offer no Fabric; defaults are selected in every column so ↵ ↵ ↵ makes something sensible; `.` shows snapshots, the way file managers show hidden files.',
    'Overlays stay anchored: ?, : and x open a short sheet above the footer, d asks in the footer. 80×24 drops to two columns (focused + next); wider terminals widen the preview, rows grow a column where there’s room (world mode and size, mod category), and list rows stop at 60 cells so values stay near their names.',
    '↵ plays from anywhere on the path (on a world it plays that world); in a list whose rows have their own verb, ↵ is that verb (update, install, change) and p still plays. In the wizard ↵ steps to the next column.',
    'Lab limits: ← can’t leave the drilled Mods and Game log screens (esc does; home has the whole path with ←→), and ↵ on “+ Add from Modrinth” can’t start typing (press /).',
  ],
  screens: { home, mods, play, wizard },
})
