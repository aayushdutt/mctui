// The one screen: Miller columns over Instances → contents → items → detail.
// Three slots show three levels; focus moves with ←→ and the levels slide
// through the slots (ranger-style) once you go past the second. Home, Mods
// and the game log are the same view at different depths.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import type { Line } from '../../term/buffer'
import { type Frame, type Item, filterRow, footer, frame, header, join, list, slots, status, textArea, title } from './ui'
import { type Entry, type Kind, entries, fuzzy, logOf, stateOf, stateWord } from './model'
import { type Level, type Ref, LISTS, detail, downloadPreview, level, logLines, logPreview, newPreview, refName } from './previews'
import { overlays } from './sheet'

/** A drilled-in entry point: this kind, of this instance, focused. */
export interface Fixed {
  kind: Kind
  inst: Instance
}

export function browse(c: Ctx, fixed?: Fixed) {
  const { data, ui, t } = c
  const f = frame(c)
  const q = ui.query

  // Level 0: instances.
  const filtering0 = !fixed && ui.focus === 0 && !!q
  const found = data.instances.map((inst) => ({ inst, hits: filtering0 ? fuzzy(q, inst.name) : [] })).filter((x) => x.hits)
  const l0: Item[] = found.map(({ inst, hits }) => ({ label: inst.name, hits, trail: stateWord(c, inst) }))
  if (!filtering0) l0.push({ label: '+ New instance', quiet: true, gap: data.instances.length > 0 })
  const s0 = fixed ? data.instances.indexOf(fixed.inst) : c.sel(Math.max(1, l0.length), { pane: 0 })
  const inst = fixed ? fixed.inst : (found[s0]?.inst ?? null)
  const depth = fixed ? 2 : c.panes(inst ? 3 : 1)

  // Level 1: its contents.
  const ents = inst ? entries(c, inst) : []
  const s1 = fixed ? Math.max(0, ents.findIndex((e) => e.kind === fixed.kind)) : c.sel(Math.max(1, ents.length), { pane: 1 })
  const entry: Entry | undefined = ents[s1]

  // Level 2: the entry's items (worlds, mods…), or content (log, download).
  const pane2 = fixed ? 0 : 2
  const isList = !!entry && LISTS.includes(entry.kind)
  const lv: Level | null = inst && entry && isList ? level(c, inst, entry.kind, depth === 2 ? q : '') : null
  const s2 = c.sel(Math.max(1, lv?.items.length ?? 1), { pane: pane2 })
  const ref: Ref | undefined = lv?.refs[s2]

  const l1Items: Item[] = ents.map((e) => ({ label: e.label, trail: e.trail }))
  const sl = slots(f)

  // ── Columns ──
  const col0 = (r: typeof sl[0], focused: boolean) => {
    if (filtering0) filterRow(c, r, q, `${found.length} of ${data.instances.length}`)
    else title(c, r, 'Instances', { count: data.instances.length || undefined, focused, region: 'focus:0' })
    list(c, inner(r), {
      items: l0,
      sel: s0,
      mark: focused ? 'focus' : 'path',
      tone: focused ? 'focus' : 'parent',
      pane: fixed ? undefined : 0,
      empty: [['Nothing matches.', { fg: t.muted }]],
    })
  }
  const col1 = (r: typeof sl[0], mode: 'focus' | 'path' | 'ahead') => {
    if (!inst) return
    title(c, r, inst.name, { focused: mode === 'focus', region: fixed ? undefined : 'focus:1' })
    list(c, inner(r), {
      items: l1Items,
      sel: s1,
      mark: mode,
      tone: mode === 'focus' ? 'focus' : mode === 'path' ? 'parent' : 'preview',
      pane: fixed ? undefined : 1,
    })
  }
  /** Level 2 as a list: focused, or previewed from level 1. */
  const col2 = (r: typeof sl[0], focused: boolean) => {
    if (!inst || !entry) return
    if (entry.kind === 'log') return logPreview(c, r, inst)
    if (entry.kind === 'download') return downloadPreview(c, r, inst)
    if (!lv) return
    if (focused && q) filterRow(c, r, q, lv.filterCount ?? '')
    else title(c, r, entry.label, { count: lv.count === '' ? undefined : lv.count, focused, region: fixed ? undefined : `focus:${pane2}` })
    list(c, inner(r), {
      items: lv.items,
      sel: s2,
      mark: focused ? 'focus' : 'none',
      tone: focused ? 'focus' : 'preview',
      pane: pane2,
      empty: lv.empty,
      extraW: lv.extraW,
    })
  }
  /** The log (or download) given the room of two slots. */
  const wideContent = (r: typeof sl[0]) => {
    if (!inst || !entry) return
    if (entry.kind === 'download') return downloadPreview(c, r, inst, true)
    const right: Line = [[`${logOf(c, inst).length} lines`, { fg: t.faint }]]
    if (q) filterRow(c, r, q, `${logOf(c, inst).filter((l) => l.text.toLowerCase().includes(q.toLowerCase())).length} lines`)
    else title(c, r, 'Game log', { focused: true, right })
    logLines(c, textArea(r), logOf(c, inst), q, false)
  }

  if (!inst) {
    // First run, or `+ New instance` selected.
    col0(sl[0], true)
    newPreview(c, f.narrow ? sl[1] : join(sl[1], sl[2]), !data.instances.length)
  } else if (f.narrow) {
    if (depth === 0) {
      col0(sl[0], true)
      col1(sl[1], 'ahead')
    } else if (depth === 1) {
      col1(sl[0], 'focus')
      col2(sl[1], false)
    } else if (!isList) wideContent(join(sl[0], sl[1]))
    else {
      col2(sl[0], true)
      detail(c, sl[1], inst, ref)
    }
  } else if (depth < 2) {
    col0(sl[0], depth === 0)
    col1(sl[1], depth === 1 ? 'focus' : 'ahead')
    col2(sl[2], false)
  } else {
    col1(sl[0], 'path')
    if (!isList) wideContent(join(sl[1], sl[2]))
    else {
      col2(sl[1], true)
      detail(c, sl[2], inst, ref)
    }
  }

  // ── Header, status, footer ──
  const crumbs: [string, string?][] = []
  if (!inst && data.instances.length) crumbs.push(['New instance'])
  if (inst) crumbs.push([inst.name, fixed ? 'screen:home' : 'focus:0'])
  if (inst && entry && depth >= 1) crumbs.push([entry.label, fixed ? undefined : 'focus:1'])
  const leaf = refName(ref)
  if (inst && depth === 2 && leaf) crumbs.push([leaf])
  header(c, f.header, crumbs)
  status(c, f.status)
  const verbs = keys(c, f, inst, entry, ref, depth)
  c.enter(enterTarget(inst, entry, ref, depth, !!fixed))
  footer(c, f.footer, verbs)
  overlays(c, f, inst, ref)
}

/** A slot below its title row and the blank under it (rows pad themselves). */
const inner = (r: { x: number; y: number; w: number; h: number }) => ({ x: r.x, y: r.y + 2, w: r.w, h: r.h - 2 })

function enterTarget(inst: Instance | null, entry: Entry | undefined, ref: Ref | undefined, depth: number, fixed: boolean) {
  if (!inst) return 'wizard' as const
  if (depth < 2) return 'play' as const
  if (fixed) return null
  if (entry?.kind === 'worlds' && ref) return 'play' as const
  return null
}

/** The footer: only what works on the focused row. */
function keys(c: Ctx, f: Frame, inst: Instance | null, entry: Entry | undefined, ref: Ref | undefined, depth: number): [string, string][] {
  const { ui, data } = c
  void f
  if (ui.typing) return [['↵', 'done'], ['esc', 'clear'], ['↑↓', 'move']]
  if (!inst) return [['↵', 'new instance']]
  const st = stateOf(c, inst)
  const expired = data.account.status === 'expired'
  const back: [string, string] = ['←', 'back']
  if (depth === 0) {
    const play: [string, string] =
      st.phase === 'running'
        ? ['↵', 'game log']
        : st.phase === 'crashed'
          ? ['↵', 'play again']
          : expired
            ? ['↵', 'play offline']
            : ['↵', 'play']
    const out: [string, string][] = [play, ['→', st.phase === 'crashed' ? 'see why' : 'open']]
    if (expired) out.push(['a', 'sign in'])
    if (st.phase === 'crashed') out.push(['m', 'mods'])
    out.push(['n', 'new'], ['/', 'filter'])
    return out
  }
  if (depth === 1) {
    const out: [string, string][] = [st.phase === 'running' ? ['↵', 'game log'] : ['↵', 'play'], ['→', 'open'], back]
    return out
  }
  if (!entry) return [back]
  if (entry.kind === 'log')
    return st.phase === 'running'
      ? [['/', 'search'], ['s', 'stop game'], ['c', 'copy'], back]
      : st.phase === 'crashed'
        ? [['m', 'fix mods'], ['c', 'copy report'], ['p', 'play again'], ['/', 'search'], back]
        : [['/', 'search'], ['c', 'copy'], ['p', 'play'], back]
  if (entry.kind === 'download') return [['p', 'play when ready'], back]
  if (!ref) return [['p', 'play'], back]
  switch (ref.k) {
    case 'add':
      return [['/', 'search Modrinth'], ['p', 'play'], back]
    case 'mod': {
      const out: [string, string][] = []
      if (ref.m.update && !ref.m.queued) out.push(['↵', 'update'])
      out.push(['space', ref.m.enabled ? 'turn off' : 'turn on'], ['d', 'remove'], ['/', 'search'], back)
      return out
    }
    case 'hit':
      return [['↵', 'install'], ['o', 'Modrinth page'], ['esc', 'clear'], back]
    case 'world':
      return [['↵', 'play this world'], ['o', 'open folder'], ['d', 'delete'], back]
    case 'pack':
      return [['space', 'turn off'], ['d', 'remove'], back]
    case 'setting':
      return [['↵', 'change'], ['p', 'play'], back]
  }
}
