// The app's components (crates/mctui/src/ui/widgets.rs, DESIGN.md), ported so
// the baseline looks like the real thing and variations can reuse or replace
// them. Each draws one row at most and can register a click region.

import type { Ctx } from '../kit'
import { type Color, type Line, type Rect, type Style, lineWidth, rect, right, bottom, width, centered, inset } from './buffer'
import { onColor } from './theme'

/** `INSTANCES   3`: uppercase, accent while its pane has focus. Returns the end x. */
export function section(c: Ctx, x: number, y: number, label: string, count?: string | number, focused = false, max = 40) {
  const { t, buf } = c
  const end = buf.text(x, y, label.toUpperCase(), { fg: focused ? t.accent : t.muted, bold: true }, max)
  if (count === undefined) return end
  return buf.text(end + 3, y, String(count), { fg: t.muted }, max - (end + 3 - x))
}

/** A list row's band (selection or hover); returns the content area inset 2 columns. */
export function band(c: Ctx, r: Rect, selected: boolean, hovered: boolean): Rect {
  const { t, buf } = c
  const row = rect(r.x, r.y, r.w, 1)
  if (selected) buf.fill(row, { bg: t.selection })
  else if (hovered) buf.fill(row, { bg: t.surface })
  return rect(r.x + 2, r.y, r.w - 4, 1)
}

export function rowLabel(c: Ctx, selected: boolean, hovered: boolean): Style {
  const { t } = c
  if (selected) return { fg: t.strong, bold: true }
  if (hovered) return { fg: t.strong }
  return { fg: t.text }
}

/** A one-row tinted pill: ` line ` on `bg`. Returns its area. */
export function pill(c: Ctx, x: number, y: number, line: Line, bg?: Color, max = 60): Rect {
  const w = Math.min(lineWidth(line) + 2, max)
  const r = rect(x, y, w, 1)
  if (bg) c.buf.fill(r, { bg })
  c.buf.line(x + 1, y, line, {}, w - 2)
  return r
}

export type ButtonKind = 'primary' | 'secondary' | 'danger'

/** ` ▶ Play  ↵ `: the key sits inside, quieter. Returns its area. */
export function button(c: Ctx, x: number, y: number, label: string, key?: string, kind: ButtonKind = 'secondary', region?: string): Rect {
  const { t } = c
  const hovered = !!region && c.hovered(region)
  const bg =
    kind === 'primary' ? (hovered ? t.accentHi : t.accent) : kind === 'danger' ? t.error : hovered ? t.surfaceHi : t.surface
  const fg = kind === 'secondary' ? (hovered ? t.strong : t.text) : onColor(bg)
  const line: Line = [' ', [label, { fg, bold: kind !== 'secondary' }]]
  if (key) line.push([`  ${key}`, { fg }])
  line.push(' ')
  const r = pill(c, x, y, line, bg)
  if (region) c.buf.region(r, region)
  return r
}

export type Tone = 'plain' | 'neutral' | 'selected' | 'success' | 'warning' | 'error'

/** A one-row chip. Returns its area. */
export function chip(c: Ctx, x: number, y: number, line: Line, tone: Tone = 'neutral', region?: string, max = 60): Rect {
  const { t } = c
  const hovered = !!region && c.hovered(region)
  const bg = {
    plain: hovered ? t.surface : undefined,
    neutral: hovered ? t.surfaceHi : t.surface,
    selected: t.selection,
    success: t.successBg,
    warning: t.warningBg,
    error: t.errorBg,
  }[tone]
  const r = pill(c, x, y, line, bg, max)
  if (region) c.buf.region(r, region)
  return r
}

/** Tab chips; the active one filled. Regions are `tab:N`. */
export function tabs(c: Ctx, r: Rect, items: [string, (string | number)?][], active: number, focused: boolean) {
  const { t } = c
  let x = r.x
  items.forEach(([label, count], i) => {
    if (right(r) - x < 4) return
    const on = i === active
    const hovered = c.hovered(`tab:${i}`)
    const fg = on ? t.strong : hovered ? t.text : t.dim
    const line: Line = [[label, { fg, bold: on }]]
    if (count !== undefined) line.push([` ${count}`, { fg: on ? t.subtle : t.muted }])
    const tone: Tone = on ? (focused ? 'selected' : 'neutral') : 'plain'
    x = right(chip(c, x, r.y, line, tone, `tab:${i}`, right(r) - x)) + 1
  })
}

/** A one-row text field on a band; the cursor when focused. */
export function field(c: Ctx, r: Rect, value: string, placeholder: string, focused: boolean, region?: string) {
  const { t, buf } = c
  const hovered = !!region && c.hovered(region)
  const row = rect(r.x, r.y, r.w, 1)
  buf.fill(row, { bg: focused ? t.selection : hovered ? t.surfaceHi : t.surface })
  const end = value
    ? buf.text(r.x + 1, r.y, value, { fg: t.strong }, r.w - 2)
    : (buf.text(r.x + 1, r.y, placeholder, { fg: t.muted }, r.w - 2), r.x + 1)
  if (focused) buf.cursor = { x: end, y: r.y }
  if (region) buf.region(row, region)
}

/** `/ Search Modrinth ……… 8 of 5,056`: a pane-wide search row, quiet until focused. */
export function search(c: Ctx, r: Rect, query: string, placeholder: string, count: string | undefined, focused: boolean) {
  const { t, buf } = c
  const row = rect(r.x, r.y, r.w, 1)
  const hovered = c.hovered('query')
  buf.fill(row, { bg: focused ? t.selection : hovered ? t.surfaceHi : t.surface })
  const inner = inset(row, 0, 2)
  const end = count ? buf.lineRight(inner, [[count, { fg: t.subtle }]]) - 2 : right(inner)
  buf.text(inner.x, r.y, '/', focused ? { fg: t.accent, bold: true } : { fg: t.muted })
  const fx = inner.x + 2
  const x = query
    ? buf.text(fx, r.y, query, { fg: focused ? t.strong : t.text }, end - fx)
    : (buf.text(fx, r.y, placeholder, { fg: t.muted }, end - fx), fx)
  if (focused) buf.cursor = { x, y: r.y }
  buf.region(row, 'query')
}

/** ` ● On ` / ` ○ Off `. */
export function toggle(c: Ctx, x: number, y: number, on: boolean, focused: boolean, region?: string): Rect {
  const { t } = c
  const [label, fg, tone]: [string, Color, Tone] = on ? ['● On', t.success, 'success'] : ['○ Off', t.dim, 'neutral']
  return chip(c, x, y, [[label, { fg, bold: focused }]], focused && !on ? 'selected' : tone, region, 12)
}

/** ` ‹ catppuccin › `. */
export function choice(c: Ctx, x: number, y: number, value: string, focused: boolean, region?: string): Rect {
  const { t } = c
  const arrows = { fg: focused ? t.accent : t.muted }
  return chip(
    c,
    x,
    y,
    [['‹ ', arrows], [value, focused ? { fg: t.strong, bold: true } : { fg: t.text }], [' ›', arrows]],
    focused ? 'selected' : 'neutral',
    region,
    40,
  )
}

/** `━━━━━━━──────  45%`. */
export function progress(c: Ctx, r: Rect, ratio: number, fg?: Color) {
  const { t, buf } = c
  ratio = Math.min(1, Math.max(0, ratio))
  const label = `${Math.round(ratio * 100)}%`.padStart(4)
  const barW = Math.max(0, r.w - width(label) - 2)
  const filled = Math.round(barW * ratio)
  buf.hline(r.x, r.y, filled, { fg: fg ?? t.accent }, '━')
  buf.hline(r.x + filled, r.y, barW - filled, { fg: t.rule }, '─')
  buf.text(r.x + barW + 2, r.y, label, { fg: t.subtle })
}

/** A label/value row: the label dim in a fixed column. */
export function kv(c: Ctx, r: Rect, label: string, value: Line, labelW = 16) {
  const { t, buf } = c
  buf.text(r.x, r.y, label, { fg: t.dim }, labelW)
  buf.line(r.x + labelW, r.y, value, { fg: t.text }, r.w - labelW)
}

/** Stat tiles: values bold over muted labels. Returns rows used. */
export function stats(c: Ctx, r: Rect, items: [string, string][]): number {
  const { t, buf } = c
  const MIN = 13
  if (!items.length || r.w <= 0) return 0
  const perRow = Math.max(1, Math.min(Math.floor(r.w / MIN), items.length))
  const tileW = Math.floor(r.w / perRow)
  let y = r.y
  for (let i = 0; i < items.length; i += perRow) {
    items.slice(i, i + perRow).forEach(([value, label], j) => {
      const x = r.x + j * tileW
      buf.text(x, y, value, { fg: t.strong, bold: true }, tileW - 1)
      buf.text(x, y + 1, label, { fg: t.muted }, tileW - 1)
    })
    y += 3
  }
  return y - r.y - 1
}

/** Footer keycaps: ` ↵ ` on a surface, then the label dim. Returns the end x. */
export function keycaps(c: Ctx, x: number, y: number, hints: [string, string][], max = c.buf.cols - x) {
  const { t, buf } = c
  const limit = x + max
  for (const [key, label] of hints) {
    const need = width(key) + 2 + 1 + width(label)
    if (x + need > limit) break
    const cap = pill(c, x, y, [[key, { fg: t.strong, bold: true }]], t.surface)
    buf.region(cap, `key:${key}`)
    x = buf.text(right(cap) + 1, y, label, { fg: c.hovered(`key:${key}`) ? t.text : t.dim }) + 3
  }
  return x
}

/** The page frame: 1 row / 2 column margins, a header, and the footer row. */
export function page(c: Ctx) {
  const area = inset(c.buf.area(), 1, 2)
  return {
    area,
    header: rect(area.x, area.y, area.w, 1),
    body: rect(area.x, area.y + 2, area.w, area.h - 4),
    footer: rect(area.x, bottom(area) - 1, area.w, 1),
  }
}

/** `mctui › Survival 1.21 › Mods` with the account flush right. */
export function header(c: Ctx, r: Rect, crumbs: string[] = []) {
  const { t, buf, data } = c
  let x = buf.text(r.x, r.y, 'mctui', { fg: t.accent, bold: true })
  crumbs.forEach((crumb, i) => {
    x = buf.text(x + 2, r.y, '›', { fg: t.faint })
    x = buf.text(x + 2, r.y, crumb, { fg: i === crumbs.length - 1 ? t.text : t.dim })
  })
  const acct = data.account
  const dot = acct.status === 'expired' ? t.warning : t.success
  const start = buf.lineRight(r, [['● ', { fg: dot }], [acct.name, { fg: t.text }]])
  buf.region(rect(start, r.y, right(r) - start, 1), 'screen:accounts')
}

/** A toast on the row above the footer. */
export function toast(c: Ctx, r: Rect, text: string, tone: 'success' | 'warning' | 'error' = 'success') {
  const { t } = c
  const [glyph, fg] = tone === 'success' ? ['✓', t.success] : tone === 'warning' ? ['▲', t.warning] : ['✗', t.error]
  chip(c, r.x, r.y, [[`${glyph} `, { fg }], [text, { fg: t.text }]], tone, undefined, r.w)
}

/**
 * A floating card over the dimmed page: bold title over a rule, a closing
 * rule. Returns the body area. Clicking outside closes it.
 */
export function card(c: Ctx, w: number, h: number, title: string, opts: { danger?: boolean; hint?: string } = {}): Rect {
  const { t, buf } = c
  buf.fade(t.backdrop)
  buf.region(buf.area(), 'close')
  const r = centered(buf.area(), w, h)
  buf.clear(inset(r, 0, -2))
  buf.text(r.x, r.y, title, { fg: opts.danger ? t.error : t.accent, bold: true })
  if (opts.hint) buf.lineRight(rect(r.x, r.y, r.w, 1), [[opts.hint, { fg: t.muted }]])
  buf.hline(r.x, r.y + 1, r.w, { fg: t.rule })
  buf.hline(r.x, bottom(r) - 1, r.w, { fg: t.rule })
  buf.region(r, 'card')
  return rect(r.x, r.y + 3, r.w, r.h - 5)
}

/** A `↓ 3 more` / `↑ 2 more` note. */
export function more(c: Ctx, x: number, y: number, n: number, up = false) {
  if (n > 0) c.buf.text(x, y, `${up ? '↑' : '↓'} ${n} more`, { fg: c.t.faint })
}

/** Rows from a list that fit in `h` rows at `stride` rows each, keeping `sel` visible. */
export function scroll<T>(items: T[], sel: number, fit: number): { start: number; shown: T[] } {
  const start = Math.max(0, Math.min(sel - fit + 1, items.length - fit, sel))
  const s = Math.max(0, Math.min(start, Math.max(0, items.length - fit)))
  return { start: s, shown: items.slice(s, s + fit) }
}
