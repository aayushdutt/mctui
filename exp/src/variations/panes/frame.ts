// The Panes direction's chrome: gap-safe panel frames (a titled top rule and
// a closing rule, no vertical strokes, no multi-row fills), dense tables with
// aligned columns, and the one-row status bar.

import type { Ctx } from '../../kit'
import { type Line, type Rect, type Style, rect, right, bottom, lineWidth, width } from '../../term/buffer'
import { mix, onColor } from '../../term/theme'

export type Tone = 'accent' | 'error' | 'success' | 'warning'

export interface PanelOpts {
  focused?: boolean
  /** Muted number right after the title: `Instances 5`. */
  count?: string | number
  /** Set into the right end of the top rule. */
  right?: Line
  /** Set into the right end of the closing rule (scroll position, hints). */
  foot?: Line
  /** Click the title row to focus this panel (`focus:N`). */
  region?: string
  /** A danger dialog paints its frame red. */
  tone?: 'error'
  /** Skip the closing rule (the last panel above the status bar keeps it). */
  open?: boolean
  /** Tabs set into the top rule after the title: `── Log ── All 24 ─ Warnings 3 ──`. */
  tabs?: { items: [string, (string | number)?][]; active: number }
}

/** The rule colour for a panel: the accent bleeds in when it has focus. */
export function ruleColor(c: Ctx, focused = false, tone?: 'error') {
  const { t } = c
  if (tone === 'error') return mix(t.rule, t.error, 0.7)
  return focused ? mix(t.rule, t.accent, 0.7) : t.rule
}

/**
 * `── Title 5 ─────────────── right ─` over the content, `──────── foot ─`
 * under it. Returns the content area (1 column in from each rule end).
 */
export function panel(c: Ctx, r: Rect, title: string | Line, o: PanelOpts = {}): Rect {
  const { buf, t } = c
  if (r.h < 2 || r.w < 6) return rect(r.x, r.y, 0, 0)
  const fg = ruleColor(c, o.focused, o.tone)
  const titleSt: Style =
    o.tone === 'error' ? { fg: t.error, bold: true } : o.focused ? { fg: t.accent, bold: true } : { fg: t.subtle, bold: true }
  buf.hline(r.x, r.y, r.w, { fg })
  if (o.region) buf.region(rect(r.x, r.y, r.w, 1), o.region)
  const tl: Line = typeof title === 'string' ? [title] : title
  const head: Line = [' ', ...tl]
  if (o.count !== undefined) head.push([` ${o.count}`, { fg: o.focused ? t.subtle : t.muted, bold: false }])
  head.push(' ')
  // The right-hand label gives way before the title does.
  const right0 = o.right && lineWidth(head) + lineWidth(o.right) + 8 <= r.w ? o.right : undefined
  const rightW = right0 ? lineWidth(right0) + 4 : 0
  let tx = buf.line(r.x + 2, r.y, head, titleSt, Math.max(0, r.w - 4 - rightW))
  if (o.tabs) {
    tx += 1
    o.tabs.items.forEach(([label, count], i) => {
      const on = i === o.tabs!.active
      const hov = c.hovered(`tab:${i}`)
      const line: Line = [' ', [label, on ? { fg: o.focused ? t.strong : t.text, bold: true, underline: true } : { fg: hov ? t.text : t.dim }]]
      if (count !== undefined) line.push([` ${count}`, { fg: on ? t.subtle : t.muted }])
      line.push(' ')
      const w = lineWidth(line)
      if (tx + w > right(r) - 2 - rightW) return
      buf.line(tx, r.y, line)
      buf.region(rect(tx, r.y, w, 1), `tab:${i}`)
      tx += w + 1
    })
  }
  if (right0) buf.lineRight(rect(r.x, r.y, r.w - 2, 1), [' ', ...right0, ' '], { fg: t.dim })
  if (!o.open) {
    const y = bottom(r) - 1
    buf.hline(r.x, y, r.w, { fg })
    if (o.foot) buf.lineRight(rect(r.x, y, r.w - 2, 1), [' ', ...o.foot, ' '], { fg: t.muted })
  }
  return rect(r.x + 1, r.y + 1, r.w - 2, r.h - (o.open ? 1 : 2))
}

/** A quiet rule inside a panel: `── Java 4 ───────`. */
export function subRule(c: Ctx, r: Rect, title: string, count?: string | number) {
  const { buf, t } = c
  buf.hline(r.x, r.y, r.w, { fg: t.rule })
  const line: Line = [' ', [title, { fg: t.dim, bold: true }]]
  if (count !== undefined) line.push([` ${count}`, { fg: t.muted }])
  line.push(' ')
  buf.line(r.x + 2, r.y, line, {}, Math.max(0, r.w - 4))
}

/**
 * A rule inside a panel carrying tabs: `── Worlds 2 ── Mods 3 ── Packs 0 ────`.
 * Regions `tab:N`. Returns nothing; draws on `r`'s first row, full width.
 */
export function tabRule(c: Ctx, r: Rect, items: [string, number?][], active: number, focused: boolean) {
  const { buf, t } = c
  const fg = focused ? mix(t.rule, t.accent, 0.45) : t.rule
  buf.hline(r.x, r.y, r.w, { fg })
  let x = r.x + 2
  items.forEach(([label, count], i) => {
    const on = i === active
    const hov = c.hovered(`tab:${i}`)
    const line: Line = [
      ' ',
      [label, on ? { fg: focused ? t.accent : t.strong, bold: true } : { fg: hov ? t.text : t.dim }],
    ]
    if (count !== undefined) line.push([` ${count}`, { fg: on ? t.subtle : t.muted }])
    line.push(' ')
    const w = lineWidth(line)
    if (x + w > right(r) - 1) return
    buf.line(x, r.y, line)
    buf.region(rect(x, r.y, w, 1), `tab:${i}`)
    x += w + 1
  })
}

// ── Tables ────────────────────────────────────────────────────────────────

export interface Col {
  label: string
  /** Cells, or 'fr' for a share of what's left. */
  w: number | 'fr'
  align?: 'left' | 'right'
}

export interface Placed extends Col {
  x: number
  cw: number
}

/** Lays columns out left to right with `gap` cells between them. */
export function place(r: Rect, cols: Col[], gap = 2): Placed[] {
  const fixed = cols.reduce((s, c) => s + (c.w === 'fr' ? 0 : c.w), 0) + gap * (cols.length - 1)
  const frs = cols.filter((c) => c.w === 'fr').length
  const free = Math.max(0, r.w - fixed)
  let x = r.x
  let frUsed = 0
  return cols.map((c) => {
    let cw: number
    if (c.w === 'fr') {
      frUsed++
      cw = frUsed === frs ? free - Math.floor(free / frs) * (frs - 1) : Math.floor(free / frs)
    } else cw = c.w
    const out = { ...c, x, cw: Math.max(0, Math.min(cw, right(r) - x)) }
    x += cw + gap
    return out
  })
}

/** The header row: labels muted, aligned like their columns. */
export function header(c: Ctx, y: number, cols: Placed[]) {
  for (const col of cols) cell(c, col, y, [[col.label, { fg: c.t.muted }]])
}

/** One cell, left or right aligned inside its column. */
export function cell(c: Ctx, col: Placed, y: number, line: Line, base: Style = {}) {
  const { buf } = c
  if (col.cw <= 0) return
  if (col.align === 'right') buf.lineRight(rect(col.x, y, col.cw, 1), line, base)
  else buf.line(col.x, y, line, base, col.cw)
}

export type RowState = 'focus' | 'blur' | 'hover' | null

/** A row's band: the accent-tinted selection, a quiet one when its panel is blurred, or hover. */
export function band(c: Ctx, r: Rect, state: RowState) {
  const { buf, t } = c
  const row = rect(r.x, r.y, r.w, 1)
  if (state === 'focus') buf.fill(row, { bg: t.selection })
  else if (state === 'blur') buf.fill(row, { bg: t.surface })
  else if (state === 'hover') buf.fill(row, { bg: mix(t.bg, t.surface, 0.55) })
}

export function rowState(c: Ctx, selected: boolean, focused: boolean, key: string): RowState {
  if (selected) return focused ? 'focus' : 'blur'
  return c.hovered(key) ? 'hover' : null
}

/** The label style for a row in `state`. */
export function rowText(c: Ctx, state: RowState): Style {
  const { t } = c
  if (state === 'focus') return { fg: t.strong, bold: true }
  if (state === 'blur') return { fg: t.strong }
  if (state === 'hover') return { fg: t.strong }
  return { fg: t.text }
}

/** `name` with every case-insensitive match of `q` in the accent. */
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

/** First visible index so `sel` stays in view in a list of `n` with `fit` rows. */
export function scrollStart(n: number, sel: number, fit: number) {
  if (n <= fit) return 0
  return Math.max(0, Math.min(sel - Math.floor(fit / 2), n - fit))
}

/** `━━━━━━────  64%` in one row. Returns the end x. */
export function bar(c: Ctx, x: number, y: number, w: number, ratio: number, fg?: string) {
  const { buf, t } = c
  const filled = Math.round(Math.max(0, w) * Math.min(1, Math.max(0, ratio)))
  buf.hline(x, y, filled, { fg: fg ?? t.accent }, '━')
  buf.hline(x + filled, y, Math.max(0, w - filled), { fg: t.rule }, '─')
  return x + Math.max(0, w)
}

// ── Status bar ────────────────────────────────────────────────────────────

export interface Mode {
  label: string
  tone?: 'accent' | 'success'
}

/**
 * The one-row status bar: a mode badge and context on the left, key hints
 * that fit, the account flush right. Hints are clickable (`key:K`).
 */
export function statusBar(c: Ctx, mode: Mode, context: Line, hints: [string, string][]) {
  const { buf, t, data } = c
  const y = buf.rows - 1
  const r = rect(0, y, buf.cols, 1)
  buf.fill(r, { bg: t.surface })
  const badge = mode.tone === 'success' ? { bg: t.successBg, fg: t.success } : { bg: t.accent, fg: t.onAccent }
  let x = buf.text(0, y, ` ${mode.label} `, { ...badge, bold: true })
  if (context.length) x = buf.line(x + 1, y, context, { fg: t.text }) + 1

  const acct = data.account
  const dot = acct.status === 'expired' ? t.warning : t.success
  const acctLine: Line = [['● ', { fg: dot }], [acct.name, { fg: c.hovered('screen:accounts') ? t.strong : t.text }], ' ']
  const ax = buf.lineRight(r, acctLine)
  buf.region(rect(ax, y, right(r) - ax, 1), 'screen:accounts')

  x += 2
  // The last hint (help) always shows: the ones before it drop first.
  const w = ([k, l]: [string, string]) => width(k) + 1 + width(l) + 2
  const last = hints[hints.length - 1]
  const limit = ax - 1 - (last ? w(last) : 0)
  const fits: [string, string][] = []
  for (const h of hints.slice(0, -1)) {
    if (x + fits.reduce((s, h2) => s + w(h2), 0) + w(h) > limit) break
    fits.push(h)
  }
  if (last) fits.push(last)
  for (const [key, label] of fits) {
    const hov = c.hovered(`key:${key}`)
    const start = x
    x = buf.text(x, y, key, { fg: t.accent, bold: true })
    x = buf.text(x + 1, y, label, { fg: hov ? t.strong : t.dim })
    buf.region(rect(start, y, x - start, 1), `key:${key}`)
    x += 2
  }
}

/** A one-row pill button: ` ▶ Play  ↵ `. */
export function button(c: Ctx, x: number, y: number, label: string, key: string, kind: 'primary' | 'secondary' | 'danger', region: string, max = 40): Rect {
  const { buf, t } = c
  const hov = c.hovered(region)
  const bg = kind === 'primary' ? (hov ? t.accentHi : t.accent) : kind === 'danger' ? t.error : hov ? t.surfaceHi : t.surface
  const fg = kind === 'primary' ? onColor(bg) : kind === 'danger' ? onColor(bg) : hov ? t.strong : t.text
  const keyFg = kind === 'secondary' ? t.accent : fg
  const line: Line = [' ', [label, { fg, bold: kind !== 'secondary' }], [`  ${key}`, { fg: keyFg }], ' ']
  const w = Math.min(lineWidth(line), max)
  const r = rect(x, y, w, 1)
  buf.fill(r, { bg })
  buf.line(x, y, line, { bg }, w)
  buf.region(r, region)
  return r
}

/** The `/` filter text for the screen's lists (the palette owns it while open). */
export const query = (c: Ctx) => (c.ui.overlay === 'palette' ? '' : c.ui.query)
/** Whether the screen's `/` field has the cursor. */
export const typing = (c: Ctx) => c.ui.overlay !== 'palette' && c.ui.typing
