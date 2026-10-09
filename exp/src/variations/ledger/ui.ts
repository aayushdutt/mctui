// Ledger's small kit: the page frame (a top line, a footer that doubles as
// the `/` prompt), aligned tables that drop columns as the width shrinks, the
// titled rule over the drawer, and the drawer's label/value ledger.

import type { Ctx } from '../../kit'
import { type Line, type Rect, type Style, lineWidth, rect, right, width, truncate } from '../../term/buffer'
import { mix } from '../../term/theme'
import * as w from '../../term/widgets'

// ── Frame ──────────────────────────────────────────────────────────────────

export interface Frame {
  /** Content column: 2 cells in from each edge. */
  x: number
  w: number
  /** The top line, the column header, the first table row, the footer. */
  top: number
  head: number
  body: number
  foot: number
}

export function frame(c: Ctx): Frame {
  const { cols, rows } = c.buf
  return { x: 2, w: cols - 4, top: 1, head: 3, body: 4, foot: rows - 2 }
}

/** `mctui › Survival 1.21 › mods` on the left (the last crumb brighter); `rightLine` (default: the account) flush right. */
export function topLine(c: Ctx, f: Frame, crumbs: string[], rightLine?: Line) {
  const { buf, t } = c
  let x = buf.text(f.x, f.top, 'mctui', { fg: t.accent, bold: true })
  crumbs.forEach((crumb, i) => {
    x = buf.text(x, f.top, '  ›  ', { fg: t.faint })
    x = buf.text(x, f.top, crumb, { fg: i === crumbs.length - 1 ? t.text : t.dim })
  })
  const r = rect(f.x, f.top, f.w, 1)
  let line = rightLine ?? account(c)
  // Too long for what's left: shed trailing segments rather than overlap the crumbs.
  const room = right(r) - x - 4
  while (line.length > 1 && lineWidth(line) > room) line = line.slice(0, -1)
  if (!line.length || lineWidth(line) > room) return
  const start = buf.lineRight(r, line)
  if (!rightLine) buf.region(rect(start, f.top, right(r) - start, 1), 'screen:accounts')
}

/** The account: quiet, until sign-in expires. */
export function account(c: Ctx): Line {
  const { t, data } = c
  const a = data.account
  const hover = c.hovered('screen:accounts')
  if (a.status === 'expired') return [['▲ ', { fg: t.warning }], ['sign-in expired', { fg: t.warning }], ['  ', {}], [a.name, { fg: t.text }]]
  return [[a.name, { fg: hover ? t.text : t.muted }]]
}

export type Hint = [string, string]

const hintsWidth = (hs: Hint[]) => hs.reduce((n, [k, l]) => n + width(k) + 2 + 1 + width(l) + 3, 0) - 3

/** The footer: keycaps for what works right now. Help never drops off; the middle does. */
export function footer(c: Ctx, f: Frame, hints: Hint[], rightLine?: Line) {
  const { buf } = c
  let limit = f.w
  if (rightLine) limit -= lineWidth(rightLine) + 3
  let hs = hints
  const last = hs[hs.length - 1]
  while (hs.length > 2 && hintsWidth(hs) > limit) hs = last?.[0] === '?' ? [...hs.slice(0, -2), last] : hs.slice(0, -1)
  w.keycaps(c, f.x, f.foot, hs, limit + 1)
  if (rightLine) buf.lineRight(rect(f.x, f.foot, f.w, 1), rightLine)
}

/**
 * The footer as a prompt while `/` is typing: `/fa▏  7 of 14` on the left,
 * what ↵ and esc do on the right.
 */
export function prompt(c: Ctx, f: Frame, lead: string, query: string, placeholder: string, count: string, hints: Hint[]) {
  const { buf, t } = c
  const hw = hintsWidth(hints)
  const room = f.w - hw - 4
  const y = f.foot
  let x = buf.text(f.x, y, lead, { fg: t.accent, bold: true })
  if (query) x = buf.text(x, y, query, { fg: t.strong }, room - (x - f.x))
  else buf.text(x + 1, y, placeholder, { fg: t.faint }, room - (x - f.x))
  buf.cursor = { x, y }
  if (count) buf.text(Math.max(x + 1, f.x + (query ? x - f.x + 3 : width(placeholder) + 4)), y, count, { fg: t.muted })
  w.keycaps(c, right(rect(f.x, y, f.w, 1)) - hw, y, hints, hw + 1)
  buf.region(rect(f.x, y, room, 1), 'query')
}

// ── Rule and drawer ────────────────────────────────────────────────────────

/** `── Survival 1.21 · running ─────────── ↓ 4 more ──`: the drawer's title. */
export function rule(c: Ctx, f: Frame, y: number, title: Line, tail: Line = []) {
  const { buf, t } = c
  const st: Style = { fg: t.rule }
  buf.hline(f.x, y, f.w, st)
  let x = f.x + 3
  if (title.length) {
    x = buf.line(x, y, [' ', ...title, ' '], {}, f.w - 8)
  }
  if (tail.length) {
    const tw = lineWidth(tail) + 2
    const tx = right(rect(f.x, y, f.w, 1)) - 3 - tw
    if (tx > x + 2) buf.line(tx, y, [' ', ...tail, ' '])
  }
}

export interface Entry {
  label?: string
  labelFg?: string
  line: Line
}

/** Right-aligned labels, values in one column (`systemctl status`). Returns rows used. */
export function ledger(c: Ctx, r: Rect, entries: Entry[], gutter = 8): number {
  const { buf, t } = c
  const vx = r.x + gutter + 2
  let y = r.y
  for (const e of entries) {
    if (y >= r.y + r.h) break
    if (e.label) buf.text(r.x + gutter - width(e.label), y, e.label, { fg: e.labelFg ?? t.muted })
    buf.line(vx, y, e.line, { fg: t.text }, right(r) - vx)
    y++
  }
  return y - r.y
}

// ── Tables ─────────────────────────────────────────────────────────────────

export interface Col<T> {
  key: string
  label: string
  /** The width it wants. */
  w: number
  /** What it can make do with (default `w`). */
  min?: number
  right?: boolean
  /** Higher drops first when the table doesn't fit; 0 or unset never drops. */
  drop?: number
  /** Grows first, past `w` up to `max`, once everything has what it wants. */
  flex?: boolean
  max?: number
  /** Takes whatever is still spare: the last left-aligned column before the numbers. */
  spacer?: boolean
  /** Order it widens from `min` to `w` in (lower first; defaults to `drop`). */
  grow?: number
  /** Sortable: clicking the header picks this sort (region `tab:N`). */
  sort?: number
  cell: (row: T, w: number, on: boolean) => Line
}

export interface Placed<T> {
  col: Col<T>
  x: number
  w: number
}

/** Lays columns out across `avail` cells, dropping and widening by priority. */
export function fit<T>(cols: Col<T>[], x: number, avail: number, gap = 2, maxGap = gap): Placed<T>[] {
  let on = [...cols]
  const need = (cs: Col<T>[]) => cs.reduce((n, c) => n + (c.min ?? c.w), 0) + gap * Math.max(0, cs.length - 1)
  while (need(on) > avail) {
    const droppable = on.filter((c) => (c.drop ?? 0) > 0)
    if (!droppable.length) break
    const worst = droppable.reduce((a, b) => ((b.drop ?? 0) >= (a.drop ?? 0) ? b : a))
    on = on.filter((c) => c !== worst)
  }
  const ws = on.map((c) => c.min ?? c.w)
  let slack = avail - need(on)
  const give = (i: number, upTo: number) => {
    const add = Math.max(0, Math.min(slack, upTo - ws[i]))
    ws[i] += add
    slack -= add
  }
  // Flex columns to what they want, then the rest by priority, then flex to max.
  on.forEach((c, i) => c.flex && give(i, c.w))
  on.map((c, i) => [c, i] as const)
    .sort(([a], [b]) => (a.grow ?? a.drop ?? 0) - (b.grow ?? b.drop ?? 0))
    .forEach(([c, i]) => give(i, c.w))
  on.forEach((c, i) => c.flex && give(i, c.max ?? c.w))
  // Wide terminals: open the gaps evenly (up to maxGap) before the spacer takes the rest.
  const gaps = Math.max(1, on.length - 1)
  const extra = Math.max(0, Math.min(maxGap - gap, Math.floor(slack / gaps)))
  slack -= extra * gaps
  const si = on.findIndex((c) => c.spacer)
  if (si >= 0) ws[si] += slack
  let cx = x
  return on.map((col, i) => {
    const p = { col, x: cx, w: ws[i] }
    cx += ws[i] + gap + extra
    return p
  })
}

/** The quiet header row; the sort column brighter with its arrow. */
export function head<T>(c: Ctx, y: number, placed: Placed<T>[], sortKey?: string, desc = true) {
  const { buf, t } = c
  for (const p of placed) {
    const sorted = p.col.key === sortKey
    const hover = p.col.sort !== undefined && c.hovered(`tab:${p.col.sort}`)
    const line: Line = [[p.col.label, { fg: sorted ? t.text : hover ? t.subtle : t.muted, bold: sorted }]]
    if (sorted) line.push([desc ? ' ↓' : ' ↑', { fg: t.accent, bold: true }])
    cell(c, p, y, line)
    if (p.col.sort !== undefined) buf.region(rect(p.x, y, p.w, 1), `tab:${p.col.sort}`)
  }
}

/** One cell's content, aligned and cut to the column. */
export function cell<T>(c: Ctx, p: Placed<T>, y: number, line: Line) {
  const lw = lineWidth(line)
  const x = p.col.right ? p.x + Math.max(0, p.w - lw) : p.x
  c.buf.line(x, y, line, {}, p.w)
}

/** The one-row band behind a selected or hovered row, a cell wider than the table each side. */
export function band(c: Ctx, f: Frame, y: number, selected: boolean, hovered: boolean) {
  const { buf, t } = c
  const r = rect(f.x - 1, y, f.w + 2, 1)
  if (selected) buf.fill(r, { bg: t.selection })
  else if (hovered) buf.fill(r, { bg: t.surface })
  return r
}

/** Draws table rows `items[start…]` into `rows` rows; returns what's hidden above/below. */
export function body<T>(
  c: Ctx,
  f: Frame,
  y0: number,
  rows: number,
  placed: Placed<T>[],
  items: T[],
  sel: number,
  opts: { region?: (i: number) => string; showSel?: boolean } = {},
) {
  const { start, shown } = w.scroll(items, sel, Math.max(1, rows))
  shown.forEach((item, j) => {
    const i = start + j
    const y = y0 + j
    const key = opts.region ? opts.region(i) : `sel:${i}`
    const on = opts.showSel !== false && i === sel
    const r = band(c, f, y, on, c.hovered(key))
    for (const p of placed) cell(c, p, y, p.col.cell(item, p.w, on))
    c.buf.region(r, key)
  })
  return { above: start, below: Math.max(0, items.length - start - shown.length) }
}

/** `↑ 3  ↓ 4 more` for the rule's tail. */
export function moreTail(c: Ctx, above: number, below: number): Line {
  const { t } = c
  const out: Line = []
  if (above) out.push([`↑ ${above}`, { fg: t.muted }])
  if (above && below) out.push(['  ', {}])
  if (below) out.push([`↓ ${below} more`, { fg: t.muted }])
  return out
}

// ── Bits ───────────────────────────────────────────────────────────────────

const EIGHTHS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉']

/** Sub-cell progress: `███▍  ` on a quiet track, as spans for a table cell. */
export function bar(c: Ctx, cells: number, ratio: number, fg = c.t.accent): Line {
  const track = mix(c.t.bg, c.t.text, c.t.light ? 0.09 : 0.1)
  const eighths = Math.round(Math.min(1, Math.max(0, ratio)) * cells * 8)
  const full = Math.floor(eighths / 8)
  const part = eighths % 8
  const out: Line = []
  if (full) out.push(['█'.repeat(full), { fg }])
  if (part) out.push([EIGHTHS[part], { fg, bg: track }])
  const rest = cells - full - (part ? 1 : 0)
  if (rest > 0) out.push([' '.repeat(rest), { bg: track }])
  return out
}

/** `text` with the characters at `hits` lit in the accent. */
export function lit(c: Ctx, text: string, hits: number[] | undefined, base: Style, max?: number): Line {
  const s = max !== undefined ? truncate(text, max) : text
  if (!hits?.length) return [[s, base]]
  const on: Style = { ...base, fg: c.t.accent, bold: true, underline: true }
  const out: Line = []
  const chars = [...s]
  let run = ''
  let runOn = false
  chars.forEach((ch, i) => {
    const hit = hits.includes(i) && !(ch === '…' && i === chars.length - 1)
    if (hit !== runOn && run) {
      out.push([run, runOn ? on : base])
      run = ''
    }
    runOn = hit
    run += ch
  })
  if (run) out.push([run, runOn ? on : base])
  return out
}

/** Pads a plain string to `n` cells (left-aligned). */
export const pad = (s: string, n: number) => s + ' '.repeat(Math.max(0, n - width(s)))
export const padL = (s: string, n: number) => ' '.repeat(Math.max(0, n - width(s))) + s
