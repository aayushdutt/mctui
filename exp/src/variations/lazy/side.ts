// The side stack: Instances, Worlds, Mods, Packs. Each is a rule and a few
// rows; the open one takes the room the folded ones leave (lazygit's
// accordion). Worlds, Mods and Packs are about the selected instance.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { type Line, type Rect, rect, right } from '../../term/buffer'
import * as f from './frame'
import { crashFix, sortedMods, stateOf } from './data'

export interface PanelOpts {
  focused: boolean
  /** Selected row (-1: none). */
  sel: number
  /** The pane rows focus when clicked (`sel:N@P`); null: not focusable here. */
  pane: number | null
  /** Filter text for this panel's list. */
  q?: string
}

/** Rows in `r` under the rule, and the list window that keeps `sel` in view. */
function rowsOf(r: Rect, n: number, sel: number) {
  const fit = Math.max(0, r.h - 1)
  const start = f.windowStart(n, Math.max(0, sel), fit)
  return { fit, start, end: Math.min(n, start + fit) }
}

/** `↑ 2  ↓ 3` at the rule's right end when the list hides rows. */
function overflow(c: Ctx, r: Rect, start: number, end: number, n: number) {
  const { t } = c
  const line: Line = []
  if (start > 0) line.push([`↑ ${start}`, { fg: t.muted }])
  if (end < n) line.push([`${start > 0 ? '  ' : ''}↓ ${n - end}`, { fg: t.muted }])
  if (line.length) c.buf.lineRight(rect(r.x, r.y, r.w - 1, 1), [' ', ...line, ' '])
}

/** The click key for row `i`: plain in the focused panel, `@pane` elsewhere (it focuses first). */
const rowKey = (i: number, o: PanelOpts) => (o.pane === null ? `-` : o.focused ? `sel:${i}` : `sel:${i}@${o.pane}`)

function rowRegion(c: Ctx, r: Rect, y: number, i: number, o: PanelOpts) {
  if (o.pane === null) return
  c.buf.region(rect(r.x, y, r.w, 1), rowKey(i, o))
}

/** The quiet note an empty panel shows on its first row. */
function empty(c: Ctx, r: Rect, text: Line) {
  c.buf.line(r.x + 2, r.y + 1, text, { fg: c.t.faint }, r.w - 4)
}

/** A right-hand column cell: `line` flush against the panel's right edge. */
function rightCell(c: Ctx, r: Rect, y: number, line: Line) {
  const inn = f.inner(r)
  return c.buf.lineRight(rect(inn.x, y, inn.w, 1), line)
}

// ── Instances ─────────────────────────────────────────────────────────────

export function instancesPanel(c: Ctx, r: Rect, shown: Instance[], o: PanelOpts) {
  const { buf, t, data } = c
  const total = data.instances.length
  const q = o.q ?? ''
  const segs: Line[] = []
  if (data.running) segs.push([['● ', { fg: t.success }], [f.elapsed(c).replace(/ \d+s$/, ''), { fg: t.success }]])
  const dl = data.tasks.find((k) => k.ratio > 0)
  if (dl) segs.push([['↓ ', { fg: t.text }], [`${Math.round(dl.ratio * 100)}%`, { fg: t.text }]])
  if (data.crash) segs.push([['✗ crashed', { fg: t.error }]])
  const filtering = o.focused && (f.typing(c) || !!q)
  const ix = f.rule(c, r.x, r.y, r.w, ['Instances'], {
    focused: o.focused,
    num: '1',
    count: q ? `${shown.length}/${total}` : total,
    segs: filtering ? [] : segs,
    region: o.pane === null ? 'screen:home' : `focus:${o.pane}`,
  })
  if (filtering) f.ruleFilter(c, rect(r.x, r.y, r.w, 1), ix + 1, q, f.typing(c))

  if (!total) {
    // First run: the one thing to do.
    const y = r.y + 1
    const st = f.rowState(c, true, o.focused, rowKey(0, o))
    f.band(c, r, y, st)
    buf.text(r.x + 2, y, 'New instance', f.rowText(c, st))
    rightCell(c, r, y, [['↵', { fg: o.focused ? t.strong : t.muted, bold: true }]])
    rowRegion(c, r, y, 0, o)
    return
  }
  if (!shown.length) return empty(c, r, [`Nothing matches “${q}”`])

  const { start, end } = rowsOf(r, shown.length, o.sel)
  if (!(o.focused && (f.typing(c) || q))) overflow(c, r, start, end, shown.length)
  for (let i = start; i < end; i++) {
    const inst = shown[i]
    const y = r.y + 1 + (i - start)
    const st = f.rowState(c, i === o.sel, o.focused, rowKey(i, o))
    f.band(c, r, y, st)
    const s = stateOf(data, inst)
    const [glyph, gfg]: [string, string] = s.running
      ? ['●', t.success]
      : s.crashed
        ? ['✗', t.error]
        : s.task
          ? [c.spinner(), t.text]
          : inst.trouble
            ? ['▲', t.warning]
            : inst.fresh
              ? ['○', t.faint]
              : [' ', t.faint]
    buf.text(r.x, y, glyph, { fg: gfg })
    const tail: Line = s.running
      ? [['running', { fg: t.success }]]
      : s.crashed
        ? [['crashed', { fg: t.error }]]
        : s.task
          ? [[`${Math.round(s.task.ratio * 100)}%`, { fg: t.text }]]
          : inst.fresh
            ? [['new', { fg: t.dim }]]
            : [[inst.lastPlayed ?? '', { fg: st === 'focus' ? t.subtle : t.muted }]]
    let tx = rightCell(c, r, y, tail)
    // Wide: the version in its own column, left of the state.
    if (r.w >= 44) {
      const vx = right(r) - 2 - 9 - 2 - 10
      buf.text(vx, y, inst.mc, { fg: st === 'focus' ? t.subtle : t.faint }, 10)
      tx = vx - 1
    }
    const name = f.highlight(c, inst.name, q, f.rowText(c, st))
    buf.line(r.x + 2, y, name, {}, tx - r.x - 3)
    rowRegion(c, r, y, i, o)
  }
}

// ── Worlds ────────────────────────────────────────────────────────────────

export function worldsPanel(c: Ctx, r: Rect, inst: Instance | null, o: PanelOpts) {
  const { buf, t } = c
  const worlds = inst?.worlds ?? []
  f.rule(c, r.x, r.y, r.w, ['Worlds'], { focused: o.focused, num: '2', count: inst ? worlds.length : undefined, region: o.pane === null ? undefined : `focus:${o.pane}` })
  if (!inst) return
  if (!worlds.length) return empty(c, r, ['None yet'])
  const { start, end } = rowsOf(r, worlds.length, o.sel)
  overflow(c, r, start, end, worlds.length)
  const inn = f.inner(r)
  const modeW = inn.w >= 30 ? 10 : 0
  for (let i = start; i < end; i++) {
    const wd = worlds[i]
    const y = r.y + 1 + (i - start)
    const st = f.rowState(c, o.focused && i === o.sel, o.focused, rowKey(i, o))
    f.band(c, r, y, st)
    const tx = rightCell(c, r, y, [[wd.lastPlayed, { fg: st === 'focus' ? t.subtle : t.muted }]])
    const nameEnd = modeW ? tx - modeW - 1 : tx - 1
    buf.text(inn.x, y, wd.name, f.rowText(c, st), nameEnd - inn.x)
    if (modeW) buf.text(tx - modeW, y, wd.mode.toLowerCase(), { fg: t.faint }, modeW - 1)
    rowRegion(c, r, y, i, o)
  }
}

// ── Mods ──────────────────────────────────────────────────────────────────

export function modsPanel(c: Ctx, r: Rect, inst: Instance | null, o: PanelOpts) {
  const { buf, t, data } = c
  const all = sortedMods(data, inst)
  const q = o.q ?? ''
  const mods = q ? all.filter((m) => m.name.toLowerCase().includes(q.toLowerCase())) : all
  const crash = !!inst && data.crash?.instance === inst.name
  const s = inst ? stateOf(data, inst) : null
  const updates = all.filter((m) => m.update).length
  const segs: Line[] = []
  if (crash) segs.push([['▲ 1', { fg: t.warning }]])
  if (updates) segs.push(s?.queued ? [[`↑ ${updates} queued`, { fg: t.dim }]] : [[`↑ ${updates}`, { fg: t.success }]])
  const vanilla = inst?.loader === 'Vanilla'
  const filtering = o.focused && (f.typing(c) || !!q)
  const mx = f.rule(c, r.x, r.y, r.w, ['Mods'], {
    focused: o.focused,
    num: '3',
    count: !inst || vanilla ? undefined : q ? `${mods.length}/${all.length}` : all.length,
    segs: filtering ? [] : vanilla ? [[['vanilla', { fg: t.faint }]]] : segs,
    region: o.pane === null ? undefined : `focus:${o.pane}`,
  })
  if (filtering) f.ruleFilter(c, rect(r.x, r.y, r.w, 1), mx + 1, q, f.typing(c))
  if (!inst) return
  if (vanilla) return empty(c, r, ['Vanilla runs without mods'])
  if (!all.length) return empty(c, r, ['None yet · ', ['m', { fg: t.dim, bold: true }], ' adds some'])
  if (!mods.length) return empty(c, r, [`Nothing matches “${q}”`])
  const { start, end } = rowsOf(r, mods.length, o.sel)
  if (!filtering) overflow(c, r, start, end, mods.length)
  const inn = f.inner(r)
  const wide = inn.w >= 30
  for (let i = start; i < end; i++) {
    const m = mods[i]
    const y = r.y + 1 + (i - start)
    const st = f.rowState(c, o.focused && i === o.sel, o.focused, rowKey(i, o))
    f.band(c, r, y, st)
    const culprit = crash && m.name === crashFix.mod
    const queued = s?.queued && s.queued.label.includes(m.name)
    const [glyph, gfg]: [string, string] = culprit
      ? ['▲', t.warning]
      : queued
        ? ['↑', t.dim]
        : m.update
          ? ['↑', t.success]
          : !m.enabled
            ? ['○', t.faint]
            : [' ', t.faint]
    buf.text(r.x, y, glyph, { fg: gfg })
    const vfg = st === 'focus' ? t.subtle : t.muted
    const tail: Line = culprit
      ? [[m.version, { fg: vfg }], [' → ', { fg: t.faint }], [crashFix.to, { fg: t.warning }]]
      : m.update && wide
        ? [[m.version, { fg: vfg }], [' → ', { fg: t.faint }], [m.update, { fg: t.success }]]
        : !m.enabled
          ? [['off', { fg: t.faint }]]
          : [[m.version, { fg: vfg }]]
    const tx = rightCell(c, r, y, tail)
    buf.line(inn.x, y, f.highlight(c, m.name, q, f.rowText(c, st, !m.enabled)), {}, tx - inn.x - 1)
    rowRegion(c, r, y, i, o)
  }
}

// ── Packs ─────────────────────────────────────────────────────────────────

export function packsPanel(c: Ctx, r: Rect, inst: Instance | null, o: PanelOpts) {
  const { buf, t } = c
  const packs = inst?.packs ?? []
  f.rule(c, r.x, r.y, r.w, ['Packs'], { focused: o.focused, num: '4', count: inst ? packs.length : undefined, region: o.pane === null ? undefined : `focus:${o.pane}` })
  if (!inst) return
  if (!packs.length) return empty(c, r, ['None'])
  const { start, end } = rowsOf(r, packs.length, o.sel)
  overflow(c, r, start, end, packs.length)
  const inn = f.inner(r)
  for (let i = start; i < end; i++) {
    const p = packs[i]
    const y = r.y + 1 + (i - start)
    const st = f.rowState(c, o.focused && i === o.sel, o.focused, rowKey(i, o))
    f.band(c, r, y, st)
    buf.text(r.x, y, String(i + 1), { fg: t.faint })
    const tx = rightCell(c, r, y, [['on', { fg: st === 'focus' ? t.subtle : t.muted }]])
    buf.text(inn.x, y, p, f.rowText(c, st), tx - inn.x - 1)
    rowRegion(c, r, y, i, o)
  }
}

/**
 * The four panels in their stack; `open` is the one that gets the room.
 * Folded heights come from every instance, not the selected one, so the
 * rules stay put while the cursor moves through Instances.
 */
export function sideStack(
  c: Ctx,
  side: Rect,
  open: number,
  shown: Instance[],
  inst: Instance | null,
  o: [PanelOpts, PanelOpts, PanelOpts, PanelOpts],
) {
  const all = c.data.instances
  const cap = f.foldRows(c.buf.rows)
  const most = (k: (i: Instance) => number) => Math.max(1, ...all.map(k))
  // First run: nothing to show under Worlds, Mods and Packs, so no rows.
  const none = !all.length
  const fold = none ? [1, 0, 0, 0] : [
    Math.max(1, Math.min(cap, all.length)),
    Math.min(cap, most((i) => i.worlds.length)),
    Math.min(cap, most((i) => i.mods.length)),
    Math.min(cap, most((i) => i.packs.length)),
  ]
  const need = [
    Math.max(1, shown.length),
    inst?.worlds.length ?? 1,
    inst?.loader === 'Vanilla' ? 1 : (inst?.mods.length ?? 1),
    inst?.packs.length ?? 1,
  ][open]
  const rs = f.stack(side, open, fold, need)
  instancesPanel(c, rs[0], shown, o[0])
  worldsPanel(c, rs[1], inst, o[1])
  modsPanel(c, rs[2], inst, o[2])
  packsPanel(c, rs[3], inst, o[3])
  return rs
}
