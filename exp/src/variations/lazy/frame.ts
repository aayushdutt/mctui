// Lazy's chrome: lazygit's layout drawn with rules only. A one-line status
// at the top-left, a stack of side panels whose top rules carry title, count
// and state, one main view on the right, a closing rule, and the key line.
// No vertical strokes, no multi-row fills: every band is one row.

import type { Ctx } from '../../kit'
import { type Line, type Rect, type Style, rect, right, lineWidth, width } from '../../term/buffer'
import { mix } from '../../term/theme'

export interface Layout {
  /** Row 0 over the side stack: the account, lazygit's status panel. */
  status: Rect
  /** The side stack, rows 1 … H-3 (a blank row above the keys). */
  side: Rect
  /** The main view, its top rule included. */
  main: Rect
  /** The key line. */
  keys: Rect
}

/** Side column width: about a third, like lazygit. */
export function sideWidth(W: number) {
  return W >= 130 ? 46 : W >= 96 ? 38 : W >= 86 ? 34 : 31
}

export function layout(c: Ctx): Layout {
  const { cols: W, rows: H } = c.buf
  const sw = sideWidth(W)
  const x0 = 1
  const h = H - 3
  const mx = x0 + sw + 2
  return {
    status: rect(x0, 0, sw, 1),
    side: rect(x0, 1, sw, h),
    main: rect(mx, 1, W - 1 - mx, h),
    keys: rect(x0, H - 1, W - 2, 1),
  }
}

/** How many rows a folded panel keeps. */
export const foldRows = (H: number) => (H < 27 ? 2 : H < 36 ? 3 : 4)

/**
 * Splits the side stack. Each panel is a rule and its rows. A folded panel
 * keeps `fold[i]` rows (fixed per panel, so moving the cursor never moves a
 * rule); the open one gets as many as its list needs, up to what's left.
 * Leftover room stays at the bottom, not between panels.
 */
export function stack(r: Rect, open: number, fold: number[], need: number): Rect[] {
  const folded = fold.reduce((s, h, i) => (i === open ? s : s + 1 + h), 0)
  const room = Math.max(1, r.h - folded - 1)
  const openRows = Math.max(1, Math.min(room, Math.max(need, fold[open] ?? 1)))
  let y = r.y
  return fold.map((h, i) => {
    const rows = i === open ? openRows : h
    const out = rect(r.x, y, r.w, 1 + rows)
    y += 1 + rows
    return out
  })
}

/** Where a panel's rows go: 2 columns in (under the title), 2 short of the end. */
export const inner = (r: Rect) => rect(r.x + 2, r.y + 1, r.w - 4, r.h - 1)

export function ruleFg(c: Ctx, focused: boolean) {
  return focused ? mix(c.t.rule, c.t.accent, 0.72) : c.t.rule
}

export interface RuleOpts {
  focused?: boolean
  /** The key that jumps here, set into the rule's first cell (lazygit's [1]). */
  num?: string
  count?: string | number
  /** State set off by dashes after the title: `─ Mods 3 ─ ↑1 ─`. */
  segs?: Line[]
  /** Click regions for the segments, in order (tabs set into a rule). */
  segRegions?: string[]
  /** Flush right in the rule. */
  right?: Line
  region?: string
  /** Style for the title when it's not focused (the main view's is brighter). */
  titleSt?: Style
}

/** `─ Title 3 ─ state ─────────── right ─` on one row. Returns the x after the last segment. */
export function rule(c: Ctx, x: number, y: number, w: number, title: Line, o: RuleOpts = {}) {
  const { buf, t } = c
  const fg = ruleFg(c, !!o.focused)
  buf.hline(x, y, w, { fg })
  if (o.region) buf.region(rect(x, y, w, 1), o.region)
  if (o.num) buf.text(x, y, o.num, o.focused ? { fg: t.accent, bold: true } : { fg: t.faint })
  const base: Style = o.focused ? { fg: t.accent, bold: true } : (o.titleSt ?? { fg: t.subtle, bold: true })
  const rightW = o.right ? lineWidth(o.right) + 3 : 0
  const limit = x + w - 1 - rightW
  const head: Line = [' ', ...title]
  if (o.count !== undefined) head.push([` ${o.count}`, { fg: o.focused ? t.subtle : t.muted, bold: false }])
  head.push(' ')
  let cx = buf.line(x + 1, y, head, base, Math.max(0, limit - x - 1))
  ;(o.segs ?? []).forEach((seg, i) => {
    const segW = lineWidth(seg) + 2
    if (cx + 1 + segW > limit) return
    const sx = cx + 1
    cx = buf.line(sx, y, [' ', ...seg, ' '])
    const reg = o.segRegions?.[i]
    if (reg) buf.region(rect(sx, y, segW, 1), reg)
  })
  if (o.right) buf.lineRight(rect(x, y, w - 1, 1), [' ', ...o.right, ' '])
  return cx
}

export type RowState = 'focus' | 'blur' | 'hover' | 'danger' | null

/**
 * A list row's band, one row across the panel. The focused panel's
 * selection is accent-tinted; a folded panel's selection gets no band, only
 * weight (lazygit's inactive selection), so there's one band on screen.
 */
export function band(c: Ctx, r: Rect, y: number, state: RowState) {
  const { buf, t } = c
  const row = rect(r.x, y, r.w, 1)
  if (state === 'focus') buf.fill(row, { bg: t.selection })
  else if (state === 'danger') buf.fill(row, { bg: t.errorBg })
  else if (state === 'hover') buf.fill(row, { bg: t.surface })
}

export function rowState(c: Ctx, selected: boolean, focused: boolean, key: string): RowState {
  if (selected) return focused ? (c.ui.overlay === 'confirm' ? 'danger' : 'focus') : 'blur'
  return c.hovered(key) ? 'hover' : null
}

/** The name style for a row in `state`. */
export function rowText(c: Ctx, state: RowState, quiet = false): Style {
  const { t } = c
  if (state === 'focus' || state === 'danger') return { fg: t.strong, bold: true }
  if (state === 'blur') return { fg: t.strong, bold: true }
  if (state === 'hover') return { fg: t.strong }
  return { fg: quiet ? t.dim : t.text }
}

/** First visible index so `sel` stays in view, centred when it can be. */
export function windowStart(n: number, sel: number, fit: number) {
  if (n <= fit) return 0
  return Math.max(0, Math.min(sel - Math.floor((fit - 1) / 2), n - fit))
}

/** `name` with every case-insensitive match of `q` lit. */
export function highlight(c: Ctx, name: string, q: string, st: Style): Line {
  if (!q) return [[name, st]]
  const out: Line = []
  const lower = name.toLowerCase()
  const needle = q.toLowerCase()
  let at = 0
  for (let i = lower.indexOf(needle); i >= 0; i = lower.indexOf(needle, at)) {
    if (i > at) out.push([name.slice(at, i), st])
    out.push([name.slice(i, i + q.length), { ...st, fg: c.t.accent, underline: true }])
    at = i + q.length
  }
  if (at < name.length) out.push([name.slice(at), st])
  return out
}

/** A label/value row: the label dim in a fixed column. */
export function kv(c: Ctx, x: number, y: number, w: number, label: string, value: Line, lw = 12) {
  const { buf, t } = c
  buf.text(x, y, label, { fg: t.dim }, lw)
  buf.line(x + lw, y, value, { fg: t.text }, w - lw)
}

const EIGHTHS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉']

/** A sub-cell progress bar on one row: `████▍` over a quiet track. Returns the end x. */
export function bar(c: Ctx, x: number, y: number, w: number, ratio: number, fg?: string) {
  const { buf, t } = c
  const track = t.surface
  const units = Math.round(Math.max(0, Math.min(1, ratio)) * w * 8)
  const full = Math.floor(units / 8)
  buf.fill(rect(x, y, w, 1), { bg: track })
  for (let i = 0; i < full; i++) buf.set(x + i, y, '█', { fg: fg ?? t.text, bg: track })
  if (full < w && units % 8) buf.set(x + full, y, EIGHTHS[units % 8], { fg: fg ?? t.text, bg: track })
  return x + w
}

/** A key hint: `↵ play`. */
export type Hint = [string, string]

/**
 * The key line: the verbs that work right now, key bright and word dim;
 * `rightHints` flush right (help). Hints are clickable (`key:K`).
 */
export function keyLine(c: Ctx, r: Rect, hints: Hint[], rightHints: Hint[] = [['?', 'help']]) {
  const { buf, t } = c
  const w = ([k, l]: Hint) => width(k) + 1 + width(l)
  const rw = rightHints.reduce((s, h) => s + w(h) + 3, 0)
  let x = r.x + 2
  const limit = right(r) - rw - 1
  const draw = ([k, l]: Hint, at: number) => {
    const hov = c.hovered(`key:${k}`)
    const end = buf.text(at, r.y, k, { fg: t.strong, bold: true })
    const out = buf.text(end + 1, r.y, l, { fg: hov ? t.text : t.dim })
    buf.region(rect(at, r.y, out - at, 1), `key:${k}`)
    return out
  }
  for (const h of hints) {
    if (x + w(h) > limit) break
    x = draw(h, x) + 3
  }
  let rx = right(r) - rw + 3
  for (const h of rightHints) rx = draw(h, rx) + 3
}

/**
 * The filter, set into a rule after the title: `─ Instances 2/14 ─ / cr▏ ──`.
 * Text only (a tint here would stack on the selection band below).
 */
export function ruleFilter(c: Ctx, r: Rect, x: number, q: string, typing: boolean) {
  const { buf, t } = c
  const room = right(r) - 3 - x
  if (room < 6) return
  buf.text(x, r.y, ' ', {})
  buf.text(x + 1, r.y, '/', { fg: typing ? t.accent : t.muted, bold: true })
  buf.text(x + 2, r.y, ' ', {})
  const end = buf.text(x + 3, r.y, q, { fg: t.strong, bold: true }, room - 4)
  buf.text(end, r.y, ' ', {})
  if (typing) buf.cursor = { x: end, y: r.y }
  buf.region(rect(x, r.y, end - x + 1, 1), 'query')
}

/** The screen's `/` text (the palette owns typing while it's open). */
export const query = (c: Ctx) => (c.ui.overlay === 'palette' ? '' : c.ui.query)
export const typing = (c: Ctx) => c.ui.overlay !== 'palette' && c.ui.typing

/** The running game's elapsed time, ticking while the lab animates. */
export function elapsed(c: Ctx, base = '18m') {
  const s = (4 + Math.floor(c.ui.tick * 0.08)) % 60
  return `${base} ${String(s).padStart(2, '0')}s`
}

