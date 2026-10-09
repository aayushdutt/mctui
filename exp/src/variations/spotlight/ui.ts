// Spotlight's parts: the prompt row, the one ranked list whose selected row
// opens in place, the action line, and a helix-style status line. Every tint
// is a single row; nothing leans on vertical strokes.

import type { Ctx } from '../../kit'
import { type Color, type Line, type Rect, type Style, rect, right, width, truncate, charWidth, patch, lineWidth, wrap } from '../../term/buffer'
import { mix } from '../../term/theme'

export interface Act {
  key: string
  label: string
  /** Click region; `key:<key>` by default. */
  region?: string
  primary?: boolean
  danger?: boolean
}

export interface Item {
  section: string
  glyph: string
  glyphFg?: Color
  title: string
  hits?: number[]
  titleSt?: Style
  sub?: string
  subHits?: number[]
  right?: Line
  /** Lines shown under the row while it's selected. */
  detail?: Line[]
  acts?: Act[]
  score?: number
}

export type Tone = 'accent' | 'success' | 'warning' | 'error'

/**
 * The page: one centred column (a comfortable reading width) with the prompt
 * on its first row, the list from its fourth, one blank row above the status
 * line. `wide` spans the terminal (the game log).
 */
export function page(c: Ctx, wide = false) {
  const { cols, rows } = c.buf
  const w = wide ? cols - 8 : Math.min(cols - 8, cols >= 120 ? 84 : 72)
  const x = Math.floor((cols - w) / 2)
  const top = rows >= 36 ? 3 : 1
  const col = rect(x, top, w, rows - 2 - top)
  return { col, list: rect(x, top + 3, w, col.h - 3) }
}

/** `s` with the characters at `hits` in the accent. Returns the end x. */
export function hl(c: Ctx, x: number, y: number, s: string, hits: number[] | undefined, st: Style, max: number): number {
  const { buf, t } = c
  const cutStr = truncate(s, max)
  const clipped = cutStr !== s
  const cut = [...cutStr]
  const on = new Set(hits ?? [])
  cut.forEach((ch, i) => {
    const lit = on.has(i) && !(clipped && i === cut.length - 1)
    buf.set(x, y, ch, lit ? patch(st, { fg: t.accent, bold: true, underline: true }) : st)
    x += charWidth(ch)
  })
  return x
}

export interface Crumb {
  label: string
  region?: string
  current?: boolean
}

/**
 * `›  [scope] query▏ … right`: the one input, then a rule. Crumbs are the
 * scope (mods, log) or the answers so far (the new-instance prompts).
 */
export function prompt(c: Ctx, col: Rect, o: { placeholder: string; crumbs?: Crumb[]; right?: Line; query?: string; typing?: boolean }) {
  const { buf, t, ui } = c
  const query = o.query ?? ui.query
  const typing = o.typing ?? ui.typing
  const y = col.y
  const row = rect(col.x - 2, y, col.w + 4, 1)
  buf.region(row, 'query')
  const hov = c.hovered('query')
  buf.text(col.x, y, '›', { fg: t.accent, bold: true })
  let x = col.x + 3
  for (const cr of o.crumbs ?? []) {
    const chov = !!cr.region && c.hovered(cr.region)
    const r = rect(x, y, width(cr.label) + 2, 1)
    buf.fill(r, { bg: cr.current ? t.selection : chov ? t.surfaceHi : t.surface })
    buf.text(x + 1, y, cr.label, cr.current ? { fg: t.accent, bold: true } : { fg: chov ? t.strong : t.text })
    if (cr.region) buf.region(r, cr.region)
    x = right(r) + 1
  }
  const rx = o.right ? buf.lineRight(rect(col.x, y, col.w, 1), o.right) - 2 : right(col)
  let end = x
  if (query) end = buf.text(x, y, query, { fg: typing ? t.strong : t.text }, rx - x)
  else buf.text(x, y, o.placeholder, { fg: hov && !typing ? t.dim : t.muted }, rx - x)
  if (typing) buf.cursor = { x: end, y }
  buf.hline(col.x - 2, y + 1, col.w + 4, { fg: typing ? mix(t.rule, t.accent, 0.55) : t.rule })
}

/** `↵ play   o offline   e edit`: keys in the accent, labels quiet. Returns the end x. */
export function acts(c: Ctx, x: number, y: number, list: Act[], max: number): number {
  const { buf, t } = c
  const limit = x + max
  let end = x
  for (const a of list) {
    const need = width(a.key) + 1 + width(a.label)
    if (x + need > limit) break
    const region = a.region ?? `key:${a.key}`
    const hov = c.hovered(region)
    const kx = buf.text(x, y, a.key, { fg: a.danger ? t.error : t.accent, bold: true })
    const labelSt: Style = a.danger
      ? { fg: t.error }
      : a.primary
        ? { fg: t.strong, bold: true }
        : { fg: hov ? t.text : t.dim }
    end = buf.text(kx + 1, y, a.label, patch(labelSt, hov ? { underline: true } : {}))
    buf.region(rect(x, y, end - x, 1), region)
    x = end + 3
  }
  return end
}

type Row =
  | { k: 'label'; section: string }
  | { k: 'item'; i: number }
  | { k: 'detail'; line: Line }
  | { k: 'acts'; acts: Act[] }
  | { k: 'blank' }

/**
 * The ranked list: section labels, one row per result, and the selected
 * result opened in place (its detail lines and actions, indented under the
 * title). Scrolls to keep the whole opened result in view.
 */
export function results(
  c: Ctx,
  area: Rect,
  items: Item[],
  sel: number,
  o: { counts?: Record<string, Line>; empty?: Line } = {},
) {
  const { buf, t } = c
  if (!items.length) {
    if (o.empty) buf.line(area.x + 3, area.y, o.empty, { fg: t.faint }, area.w - 3)
    return
  }
  const rows: Row[] = []
  let section: string | null = null
  items.forEach((it, i) => {
    if (it.section !== section) {
      if (rows.length) rows.push({ k: 'blank' })
      section = it.section
      if (section) rows.push({ k: 'label', section })
    }
    rows.push({ k: 'item', i })
    if (i === sel) {
      for (const line of it.detail ?? []) rows.push({ k: 'detail', line })
      if (it.acts?.length) rows.push({ k: 'acts', acts: it.acts })
      const opened = (it.detail?.length ?? 0) + (it.acts?.length ? 1 : 0)
      if (opened && items[i + 1]?.section === section) rows.push({ k: 'blank' })
    }
  })

  const h = area.h
  const selStart = Math.max(0, rows.findIndex((r) => r.k === 'item' && r.i === sel))
  let selEnd = selStart
  while (rows[selEnd + 1] && (rows[selEnd + 1].k === 'detail' || rows[selEnd + 1].k === 'acts')) selEnd++
  let start = 0
  if (rows.length > h) {
    start = Math.max(0, selEnd - (h - 2))
    if (start > 0 && start > selStart - 1) start = Math.max(0, selStart - 1)
    // Keep a section's label with its first row.
    if (start > 0 && rows[selStart - 1]?.k === 'label' && start === selStart - 1) start = Math.max(0, start - 1)
    start = Math.min(start, rows.length - h)
  }
  const end = Math.min(rows.length, start + h)
  const itemsIn = (a: number, b: number) => rows.slice(a, b).filter((r) => r.k === 'item').length

  for (let j = start; j < end; j++) {
    const y = area.y + (j - start)
    if (j === start && start > 0) {
      buf.text(area.x + 3, y, `↑ ${itemsIn(0, j + 1)} more`, { fg: t.faint })
      continue
    }
    if (j === end - 1 && end < rows.length) {
      buf.text(area.x + 3, y, `↓ ${itemsIn(j, rows.length)} more`, { fg: t.faint })
      continue
    }
    const r = rows[j]
    if (r.k === 'label') {
      const x = buf.text(area.x, y, r.section.toUpperCase(), { fg: t.muted, bold: true })
      const count = o.counts?.[r.section] ?? [[String(items.filter((it) => it.section === r.section).length), { fg: t.faint }]]
      buf.line(x + 3, y, count, { fg: t.faint }, right(area) - x - 3)
    } else if (r.k === 'detail') {
      buf.line(area.x + 3, y, r.line, { fg: t.dim }, area.w - 3)
    } else if (r.k === 'acts') {
      acts(c, area.x + 3, y, r.acts, area.w - 3)
    } else if (r.k === 'item') {
      itemRow(c, area, y, items[r.i], r.i, r.i === sel)
    }
  }
}

function itemRow(c: Ctx, area: Rect, y: number, it: Item, i: number, on: boolean) {
  const { buf, t } = c
  const band = rect(area.x - 2, y, area.w + 4, 1)
  const hov = c.hovered(`sel:${i}`)
  if (on) buf.fill(band, { bg: t.selection })
  else if (hov) buf.fill(band, { bg: t.surface })
  buf.text(area.x, y, it.glyph, { fg: on ? t.accent : (it.glyphFg ?? t.faint), bold: on })
  const tx = area.x + 3
  const rx = it.right ? buf.lineRight(rect(area.x, y, area.w, 1), it.right) : right(area)
  const st: Style = on ? { fg: t.strong, bold: true } : hov ? { fg: t.strong } : (it.titleSt ?? { fg: t.text })
  const x = hl(c, tx, y, it.title, it.hits, st, rx - 2 - tx)
  if (it.sub && x + 5 < rx - 2) hl(c, x + 2, y, it.sub, it.subHits, { fg: on ? t.subtle : t.muted }, rx - 2 - (x + 2))
  buf.region(band, `sel:${i}`)
}

/**
 * The status line, helix-style: the mode on a tinted chip, the context, the
 * keys that work right now, the account flush right. One row, edge to edge.
 */
export function status(
  c: Ctx,
  o: { mode: string; tone?: Tone; context?: Line; hints?: Act[]; right?: Line | false; y?: number },
): { x: number; rx: number; y: number } {
  const { buf, t, data } = c
  const y = o.y ?? buf.rows - 1
  const row = rect(0, y, buf.cols, 1)
  buf.fill(row, { bg: t.surface })
  const tone = o.tone ?? 'accent'
  const [bg, fg]: [Color, Color] =
    tone === 'success'
      ? [t.successBg, t.success]
      : tone === 'warning'
        ? [t.warningBg, t.warning]
        : tone === 'error'
          ? [t.errorBg, t.error]
          : [t.selection, t.accent]
  const pill = rect(0, y, width(o.mode) + 4, 1)
  buf.fill(pill, { bg })
  buf.text(2, y, o.mode, { fg, bold: true })

  let rx = buf.cols
  if (o.right !== false) {
    const acct = data.account
    const dot = acct.status === 'expired' ? t.warning : t.success
    const line: Line = o.right ?? [['● ', { fg: dot }], [acct.name, { fg: t.text }], '  ']
    rx = buf.lineRight(row, line)
    if (!o.right) buf.region(rect(rx, y, buf.cols - rx, 1), 'screen:accounts')
  }
  let x = right(pill) + 2
  if (o.context?.length) x = buf.line(x, y, o.context, { fg: t.text }, rx - 2 - x) + 4
  if (o.hints?.length) {
    for (const a of o.hints) {
      const need = width(a.key) + 1 + width(a.label)
      if (x + need > rx - 3) break
      const region = a.region ?? `key:${a.key}`
      const hov = c.hovered(region)
      const kx = buf.text(x, y, a.key, { fg: t.accent, bold: true })
      const end = buf.text(kx + 1, y, a.label, { fg: hov ? t.strong : t.dim })
      buf.region(rect(x, y, end - x, 1), region)
      x = end + 3
    }
  }
  return { x: right(pill) + 2, rx, y }
}

/**
 * Overlays rise from the status line. A drawer clears whole rows above it
 * under an edge-to-edge rule; its content lines up with the page column.
 * Returns the content area (`h` rows).
 */
export function drawer(c: Ctx, h: number, x: number, w: number): Rect {
  const { buf, t } = c
  const y = Math.max(3, buf.rows - 2 - h)
  const r = rect(x, y, w, buf.rows - 2 - y)
  const all = rect(0, y - 2, buf.cols, buf.rows - 1 - (y - 2))
  buf.clear(all)
  buf.hline(0, y - 2, buf.cols, { fg: t.rule })
  buf.region(all, 'card')
  return r
}

/** A popover: a cleared block (2-column margins) sitting on the status line's right. */
export function popover(c: Ctx, h: number, w: number, rightEdge: number): Rect {
  const { buf } = c
  const y = Math.max(1, buf.rows - 2 - h)
  const r = rect(Math.max(2, rightEdge - w), y, w, Math.min(h, buf.rows - 2 - y))
  buf.clear(rect(r.x - 2, r.y - 1, r.w + 4, r.h + 2))
  buf.region(r, 'card')
  return r
}

/** Fades the page behind an overlay. */
export function backdrop(c: Ctx) {
  c.buf.fade(c.t.backdrop)
  c.buf.cursor = null
  c.buf.region(c.buf.area(), 'close')
}

/** A sheet's head: bold title, a quiet note flush right, a rule under both. */
export function head(c: Ctx, r: Rect, title: Line, note?: Line) {
  const { buf, t } = c
  buf.line(r.x, r.y, title, { fg: t.strong, bold: true }, r.w)
  if (note) buf.lineRight(rect(r.x, r.y, r.w, 1), note, { fg: t.muted })
  buf.hline(r.x, r.y + 1, r.w, { fg: t.rule })
}

/**
 * `·` separators between the parts of a detail line (values bright, units
 * quiet); the gaps tighten when the line wouldn't fit `max`.
 */
export function facts(c: Ctx, parts: Line[], max = Infinity): Line {
  const wide = parts.reduce((n, p) => n + lineWidth(p), 0) + 5 * (parts.length - 1) <= max
  const out: Line = []
  parts.forEach((p, i) => {
    if (i) out.push([wide ? '  ·  ' : ' · ', { fg: c.t.faint }])
    out.push(...p)
  })
  return out
}

/** `s` wrapped to `w`, at most `n` lines, the last ending in `…` if it had to cut. */
export function clamp(s: string, w: number, n: number): string[] {
  const lines = wrap(s, w)
  if (lines.length <= n) return lines
  const out = lines.slice(0, n)
  out[n - 1] = truncate(`${out[n - 1]} ${lines[n]}`, w)
  return out
}
