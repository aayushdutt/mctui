// Lazy: Panes rethought through lazygit's actual model. A stack of small side
// panels about the selected instance, the focused one open, and one main view
// that shows whatever the cursor is on.

import { defineVariation } from '../../kit'
import { home } from './home'
import { mods } from './mods'
import { play } from './play'
import { wizard } from './wizard'

export default defineVariation({
  id: 'lazy',
  name: 'Lazy',
  round: 2,
  order: 20,
  blurb:
    'lazygit’s model: a stack of small side panels about the selected instance (Instances, Worlds, Mods, Packs), the focused one open, and one main view that shows whatever the cursor is on: an overview, a world, a mod, the running game’s log. It trades the calm of a single list for a denser left column, and asks you to learn four panels.',
  notes: [
    'The accordion: the focused panel opens to what its list needs; the others fold to a few rows. Folded heights come from all instances, not the selected one, so moving through Instances never moves a rule. Leftover room gathers at the bottom, never between panels.',
    'Rules carry the state: `3 Mods 3 ─ ↑ 1`, `1 Instances 5 ─ ● 18m`, `─ ↓ 42%`, `─ ✗ crashed`, `─ / cr` while filtering. The panel’s number (lazygit’s [1]) sits in the rule’s first cell; the focused one takes the accent.',
    'The main view follows the cursor with no extra key: Instances → overview (or the live log while it runs, or what broke when it crashed); Worlds → the world (↵ plays straight into it); Mods → the mod, its update and what’s new; Packs → the pack; the status line → the account.',
    'One band on screen: the focused panel’s selection is tinted; a folded Instances panel keeps its selection in bold, so you always see whose worlds and mods you’re looking at. Mods sort what needs you first (broken, then updates).',
    'Nothing else persistent. The account is one quiet line until sign-in expires (`▲ sign-in expired ↵ renew`). Downloads live in the Instances rule and row while they run. Help, the actions menu and the command palette take the main view; delete asks in the key line.',
    'Screens: home (every scenario), mods (Mods open on the left, Modrinth search as a mode of the main view, details pinned under the results), play (the main view focused on the log: live header, level filter in the rule, find), wizard (the side panels become the steps Version / Loader / Name; ↵ ↵ ↵ creates; the main view previews it).',
  ],
  screens: { home, mods, wizard, play },
})
