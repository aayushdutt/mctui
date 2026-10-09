# mctui — UX foundation

Round 1 explored styles. The feedback: icons are noise, cards are a GUI
idea, the baseline list is airy and unpolished, the dashboard shows things
nobody needs all the time, and a prompt for everything is slower than a key.
So before drawing, this is who uses mctui, what they come for, and what a TUI
is good at. Every round-2 direction starts here.

## Who, and what they come for

A player who lives in a terminal and would rather not open a 300 MB launcher.
They have a handful of instances (1–6 usually, sometimes 15): one main world,
a modded one or two, something for a new version or a snapshot.

| How often | What they do | What mctui must make effortless |
|---|---|---|
| Nearly every session | Open mctui, play the instance they played last, switch to the game | **Zero decisions**: last-played is selected, ↵ plays, the terminal gets out of the way |
| Often | Glance back at the terminal while playing | **Is it still running? Did something break?** One line tells them |
| Weekly | Update or add mods, swap resource packs | **What's installed, what's outdated**, add from Modrinth without losing place |
| Per Minecraft release | Make a new instance | **↵ ↵ ↵**: latest version, Fabric or vanilla, a good default name |
| Rarely | Crash, sign-in expired, delete, move memory/Java | **Say what happened and the next key to press**, only when it happens |
| Almost never | Settings, multiple accounts | Out of the way until asked |

Most sessions last seconds. The terminal is a launcher and a monitor, not a
place to browse.

## The TUI mindset

1. **Text is the material.** Hierarchy comes from alignment, weight (bold,
   dim), position and one accent colour — not boxes, cards, icons or pictures.
   Glyphs only mark state (`● ○ ▲ ✓ ✗ ↑`) and structure (`─ › ·`).
2. **One row, one thing.** Lists are compact: one line per item, aligned
   columns, numbers right-aligned, no blank line between rows. Whitespace
   separates *groups*, never items. A second line per item only if it earns it.
3. **The selection drives the screen.** Moving the cursor updates everything
   that depends on it (a preview, the footer's verbs) with no extra keys. You
   never "open" something just to look at it.
4. **Keys are verbs.** ↵ does the obvious thing; single letters do the rest,
   the same letter meaning the same verb everywhere (`/` filter, `d` delete,
   `e` edit, `m` mods, `?` help, esc back). The footer lists only what works
   right now.
5. **Show state, not furniture.** Persistent space goes to what's true right
   now: what's running, what's downloading, what broke, what needs a decision
   (herdr's working / blocked / idle). Everything else appears when it
   matters and leaves after. The account is one quiet line; it gets loud only
   when sign-in expires. Downloads appear while they run.
6. **Stable geometry.** Panes don't move or resize as the selection changes;
   content swaps in place. The eye learns where things are. Overlays are
   small, anchored, and rare.
7. **Progressive disclosure.** Row (name, state) → preview (what's in it) →
   its own screen (manage it). Each level answers more, costs one key.
8. **Craft is the magic.** Not decoration: perfect alignment, truncation that
   keeps the useful end, sub-cell progress (`▏▎▍▌▋▊▉█`), a spinner only while
   work runs, a live elapsed timer on the running game, instant fuzzy filter
   with matched letters lit, the launched row turning into the running row,
   copy that says what happened and the next key. Quiet until it matters,
   then precise.

## What each screen answers

- **Home**: Which instance? Is it ready? What's in it (worlds, mods, version)?
  Is anything happening or wrong? → play in one key.
- **Game running** (home's state, or its own screen): Running fine? For how
  long? Any errors? The log is there when wanted, not in the way.
- **Crash**: What broke, in one sentence, and what to do (`l` log, `c` copy,
  `↵` try again, `m` fix mods).
- **Mods**: The instance's installed mods first (what's outdated, disabled,
  broken); Modrinth search is a mode of the same screen, not a separate place.
- **New instance**: A form with good defaults: version (latest), loader,
  name. Enter three times makes something sensible.

## Scenarios the lab can show

`State` in the toolbar (`--scenario` for `pnpm shot`): idle · game running ·
downloading · just crashed · sign-in expired · first run (no instances) ·
lots of instances. A direction is only as good as its worst state.

## Constraints (unchanged)

Cells only, no emoji or Nerd Font icons, theme roles for colour, and the
user's terminal adds line spacing: backgrounds and vertical strokes must not
span rows. 100×30 is the reference; 80×24 must work; 140×40 should use the room.
