// The Cards direction's building blocks: a responsive grid, the card head
// (one tinted title row over a rule), the one-row action bar, a filter field
// and the floating sheet overlays sit on. Nothing here fills more than one
// row or draws a vertical stroke.

import type { Ctx } from '../../kit'
import { type Line, type Rect, type Style, rect, right, bottom, cols, inset, width, lineWidth } from '../../term/buffer'
import * as w from '../../term/widgets'

/** Gutter between card columns (DESIGN's pane gutter). */
export const GAP = 4

export interface Look {
  /** The selected card. */
  on: boolean
  /** Whether the grid holding it has focus (else the selection is quieter). */
  focused: boolean
  hover: boolean
}

export const look = (c: Ctx, i: number, sel: number, focused: boolean): Look => ({
  on: sel === i,
  focused,
  hover: c.hovered(`sel:${i}`),
})

/** Evenly split columns: as many `minW`-wide cards as fit, at most `max`. */
export function columns(area: Rect, minW: number, max = 6): Rect[] {
  const n = Math.max(1, Math.min(max, Math.floor((area.w + GAP) / (minW + GAP))))
  return cols(area, Array.from({ length: n }, () => '1fr' as const), GAP)
}

export interface Placed<T> {
  item: T
  i: number
  r: Rect
}

export interface Layout<T> {
  placed: Placed<T>[]
  above: number
  below: number
  /** Where a `↓ N more` note goes, if anything is cut off. */
  moreY: number
  ncols: number
}

/** How many card rows of height `h` fit in `rows` with `gap` blank rows between (reserving a `more` row if not all do). */
function fitRows(rows: number, total: number, h: number, gap: number): number {
  if (total * h + (total - 1) * gap <= rows) return total
  return Math.max(1, Math.floor((rows - 1) / (h + gap)))
}

/** Whether all of `count` cards fit in `area` with two blank rows between card rows. */
export function fitsAll(area: Rect, count: number, minW: number, h: number, max = 6): boolean {
  const total = Math.ceil(count / columns(area, minW, max).length)
  return fitRows(area.h, total, h, 2) >= total
}

/**
 * Lays `items` out in reading order as `h`-row cards, scrolled by whole rows
 * so `sel` stays in view. Card rows sit two blank rows apart (so a title's
 * rule never reads as the card above's bottom edge), one when two would
 * leave room for a single row (or cost small cards a row).
 */
export function place<T>(area: Rect, items: T[], sel: number, minW: number, h: number, max = 6): Layout<T> {
  const cs = columns(area, minW, max)
  const n = cs.length
  const total = Math.max(1, Math.ceil(items.length / n))
  // Small cards (3 rows) can take the tighter gap sooner: they read as one line with a caption.
  const wide = fitRows(area.h, total, h, 2)
  const gap = wide >= total || (h > 3 && wide >= 2) ? 2 : 1
  const fit = gap === 2 ? wide : fitRows(area.h, total, h, 1)
  const stride = h + gap
  const selRow = Math.floor(Math.max(0, sel) / n)
  const start = Math.min(Math.max(0, selRow - fit + 1), Math.max(0, total - fit))
  const placed: Placed<T>[] = []
  for (let row = start; row < Math.min(total, start + fit); row++)
    for (let col = 0; col < n; col++) {
      const i = row * n + col
      if (i >= items.length) break
      const cr = cs[col]
      placed.push({ item: items[i], i, r: rect(cr.x, area.y + (row - start) * stride, cr.w, h) })
    }
  const above = start * n
  const below = Math.max(0, items.length - (start + fit) * n)
  return { placed, above, below, moreY: area.y + fit * stride, ncols: n }
}

/** `↑ 3 more   ↓ 2 more` under a scrolled grid. */
export function more(c: Ctx, x: number, y: number, above: number, below: number) {
  const { buf, t } = c
  const parts: Line = []
  if (above) parts.push([`↑ ${above} more`, { fg: t.faint }])
  if (above && below) parts.push('   ')
  if (below) parts.push([`↓ ${below} more`, { fg: t.faint }])
  if (parts.length) buf.line(x, y, parts)
}

/**
 * A card's head: the title row (tinted only when selected or hovered) and the
 * rule under it, which turns into the accent when selected. Returns the
 * content column (inset one cell) for the rows below.
 */
export function head(c: Ctx, r: Rect, title: Line, accessory: Line | null, lk: Look, rule: '─' | '╌' = '─'): Rect {
  const { buf, t } = c
  const band = rect(r.x, r.y, r.w, 1)
  const bg = lk.on ? (lk.focused ? t.selection : t.surfaceHi) : lk.hover ? t.surface : undefined
  if (bg) buf.fill(band, { bg })
  const inner = inset(band, 0, 1)
  const ax = accessory && lineWidth(accessory) ? buf.lineRight(inner, accessory) : right(inner) + 1
  const base: Style = { fg: lk.on || lk.hover ? t.strong : t.text, bold: true }
  buf.line(inner.x, r.y, title, base, ax - inner.x - 1)
  const st: Style = { fg: lk.on ? t.accent : lk.hover ? t.muted : t.rule }
  buf.hline(r.x, r.y + 1, r.w, st, lk.on ? '━' : rule)
  return rect(r.x + 1, r.y + 2, r.w - 2, r.h - 2)
}

/** A title with the filter's match lit in the accent. */
export function matched(c: Ctx, s: string, query: string, st: Style = {}): Line {
  const q = query.trim().toLowerCase()
  const at = q ? s.toLowerCase().indexOf(q) : -1
  if (at < 0) return [[s, st]]
  return [
    [s.slice(0, at), st],
    [s.slice(at, at + q.length), { ...st, fg: c.t.accent, underline: true }],
    [s.slice(at + q.length), st],
  ]
}

/** An action: label, the key that does it, and what a click does (default: press the key). */
export type Act = [label: string, key: string, region?: string]

/**
 * The one-row action bar for what's selected: a primary button, then quiet
 * `Label key` actions (the key in the accent), then trailing ones flush right.
 * Actions that don't fit drop from the end.
 */
export function actionBar(c: Ctx, r: Rect, primary: Act | null, acts: Act[], trailing: Act[] = []) {
  const { buf, t } = c
  const actW = ([label, key]: Act) => width(label) + 1 + width(key) + 2
  const act = (x: number, a: Act) => {
    const [label, key, region = `key:${key}`] = a
    const hov = c.hovered(region)
    const pr = rect(x, r.y, actW(a), 1)
    if (hov) buf.fill(pr, { bg: t.surface })
    const end = buf.text(x + 1, r.y, label, { fg: hov ? t.strong : t.dim })
    buf.text(end + 1, r.y, key, { fg: t.accent, bold: true })
    buf.region(pr, region)
    return right(pr)
  }
  const trailW = trailing.reduce((s, a) => s + actW(a) + 1, 0)
  let x = r.x
  if (primary) {
    const [label, key, region = `key:${key}`] = primary
    x = right(w.button(c, x, r.y, label, key, 'primary', region)) + 2
  }
  for (const a of acts) {
    if (x + actW(a) > right(r) - trailW - 2) break
    x = act(x, a) + 1
  }
  let tx = right(r) - trailW + 1
  for (const a of trailing) tx = act(tx, a) + 1
}

/**
 * The `/` filter. Quiet `/ filter` text until used; then a one-row field of
 * at most `maxW`, flush right in `r`. Full-width when `always`.
 */
export function filter(
  c: Ctx,
  r: Rect,
  opts: { placeholder: string; count?: string; always?: boolean; maxW?: number },
) {
  const { buf, t, ui } = c
  const active = ui.typing || !!ui.query
  const hov = c.hovered('query')
  if (!active && !opts.always) {
    const line: Line = [['/ ', { fg: hov ? t.accent : t.muted, bold: true }], [opts.placeholder, { fg: hov ? t.text : t.muted }]]
    const x = buf.lineRight(rect(r.x, r.y, r.w - 1, 1), line)
    buf.region(rect(x - 1, r.y, right(r) - x + 1, 1), 'query')
    return
  }
  const fw = opts.always ? r.w : Math.min(opts.maxW ?? 34, r.w)
  const f = rect(right(r) - fw, r.y, fw, 1)
  buf.fill(f, { bg: ui.typing ? t.selection : hov ? t.surfaceHi : t.surface })
  const inner = inset(f, 0, 1)
  const cx = opts.count ? buf.lineRight(inner, [[opts.count, { fg: t.subtle }]]) - 2 : right(inner)
  buf.text(inner.x, r.y, '/', { fg: ui.typing ? t.accent : t.muted, bold: true })
  const x = ui.query
    ? buf.text(inner.x + 2, r.y, ui.query, { fg: ui.typing ? t.strong : t.text }, cx - inner.x - 2)
    : (buf.text(inner.x + 2, r.y, opts.placeholder, { fg: t.muted }, cx - inner.x - 2), inner.x + 2)
  if (ui.typing) buf.cursor = { x, y: r.y }
  buf.region(f, 'query')
}

/**
 * A floating card over the dimmed page, drawn like the cards on the page:
 * a tinted title row over the accent rule, the body, a closing rule.
 * Returns the body area. Clicking outside closes it.
 */
export function sheet(c: Ctx, r: Rect, title: string, opts: { danger?: boolean; hint?: string; clear?: boolean } = {}): Rect {
  const { buf, t } = c
  if (opts.clear !== false) {
    buf.fade(t.backdrop)
    buf.region(buf.area(), 'close')
  }
  buf.clear(inset(r, -1, -2))
  const band = rect(r.x, r.y, r.w, 1)
  buf.fill(band, { bg: opts.danger ? t.errorBg : t.selection })
  buf.text(r.x + 1, r.y, title, { fg: opts.danger ? t.error : t.strong, bold: true }, r.w - 2)
  if (opts.hint) buf.lineRight(inset(band, 0, 1), [[opts.hint, { fg: t.muted }]])
  buf.hline(r.x, r.y + 1, r.w, { fg: opts.danger ? t.error : t.accent }, '━')
  buf.hline(r.x, bottom(r) - 1, r.w, { fg: t.rule })
  buf.region(r, 'card')
  return rect(r.x + 1, r.y + 3, r.w - 2, r.h - 5)
}

/** Singular or plural: `1 world`, `3 mods`. */
export const count = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`
