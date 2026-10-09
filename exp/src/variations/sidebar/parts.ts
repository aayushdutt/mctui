// Sidebar's frame: herdr's model applied to a launcher. A slim numbered list
// of instances down the left (name + a dim version line, a state word on the
// right only when something is true), the selected instance's page on the
// right, and one bottom row shared by the account (under the sidebar) and the
// keys that work now (under the page). No boxes, no divider: a gutter.

import type { Ctx } from '../../kit'
import type { Instance, LogLine } from '../../data/mock'
import { type Line, type Rect, type Style, rect, right, lineWidth, width } from '../../term/buffer'
import { mix, onColor } from '../../term/theme'

// ── Layout ────────────────────────────────────────────────────────────────

export interface Layout {
  W: number
  H: number
  /** Sidebar width, band edge to band edge (content sits 2 in). */
  sbW: number
  /** The rows the sidebar's entries may use. */
  list: Rect
  /** The page: everything right of the gutter, above the bottom row. */
  page: Rect
  /** The bottom row: the account under the sidebar, keys under the page. */
  footY: number
  /** Room for a second column on the page. */
  wide: boolean
}

export function layout(c: Ctx): Layout {
  const W = c.buf.cols
  const H = c.buf.rows
  const sbW = W >= 130 ? 32 : W >= 96 ? 29 : 26
  const footY = H - 2
  const px = sbW + 3
  const page = rect(px, 1, W - px - 2, footY - 2)
  return { W, H, sbW, list: rect(0, 3, sbW, footY - 4), page, footY, wide: page.w >= 90 }
}

// ── Instances: what's true about one right now ─────────────────────────────

export const loaderName = (i: Instance) => (i.loader === 'Vanilla' ? 'Vanilla' : `${i.loader} ${i.loaderVersion ?? ''}`.trim())
export const shortMeta = (i: Instance) => `${i.mc} · ${i.loader}`
/** `1`–`9` jump; later entries have no key, so no number. */
export const num = (i: number) => (i < 9 ? String(i + 1) : ' ')

export type AttnKind = 'running' | 'downloading' | 'crashed' | 'trouble' | 'new'

export interface Attn {
  kind: AttnKind
  /** The state word, right-aligned on the name row. */
  full: Line
  /** What's left when the name needs the room. */
  short: Line
}

/** The one thing worth saying about `inst` in the sidebar, if anything. */
export function attention(c: Ctx, inst: Instance): Attn | null {
  const { t, data } = c
  if (data.running?.instance === inst.name) {
    const l: Line = [['● ', { fg: t.success }], [data.running.elapsed, { fg: t.text }]]
    return { kind: 'running', full: l, short: l }
  }
  if (data.crash?.instance === inst.name)
    return { kind: 'crashed', full: [['✗ crashed', { fg: t.error }]], short: [['✗', { fg: t.error }]] }
  const task = data.tasks.find((k) => k.instance === inst.name)
  if (task && task.ratio > 0) {
    const pct = `${Math.round(task.ratio * 100)}%`
    return { kind: 'downloading', full: [[`${c.spinner()} `, { fg: t.accent }], [pct, { fg: t.text }]], short: [[pct, { fg: t.text }]] }
  }
  if (inst.trouble) return { kind: 'trouble', full: [['▲ crashed', { fg: t.warning }]], short: [['▲', { fg: t.warning }]] }
  if (inst.fresh) return { kind: 'new', full: [['new', { fg: t.dim }]], short: [['new', { fg: t.dim }]] }
  return null
}

// ── The sidebar ───────────────────────────────────────────────────────────

export interface SideOpts {
  /** The selected instance's index; `instances.length` is `+ new`; -1 for none. */
  sel: number
  /** Focused: the accent band. Otherwise a quiet band that just says whose page this is. */
  focused: boolean
  /** Wizard: the instance being made, shown selected just before `+ new`. */
  draft?: { name: Line; meta: Line }
  /** Home's `/` filter: entries narrow, matches lit, numbers kept. */
  query?: string
  /** Click target for entry `i` (instances.length is `+ new`). Default `sel:i@0`. */
  region?: (i: number) => string
}

type Entry = { kind: 'inst'; i: number } | { kind: 'draft' } | { kind: 'new' }

const STRIDE = 3

export function sidebar(c: Ctx, L: Layout, o: SideOpts) {
  const { buf, t, data } = c
  const insts = data.instances
  const q = (o.query ?? '').toLowerCase()
  const region = o.region ?? ((i: number) => `sel:${i}@0`)

  // The wordmark, and the filter beside it while there is one.
  buf.text(2, 1, 'mctui', { fg: t.accent, bold: true })
  if (q || (c.ui.typing && o.query !== undefined)) {
    const x = buf.text(9, 1, '/ ', { fg: t.accent, bold: true })
    const end = buf.text(x, 1, o.query ?? '', { fg: t.strong }, L.sbW - 1 - x)
    if (c.ui.typing) buf.cursor = { x: end, y: 1 }
    buf.region(rect(9, 1, L.sbW - 10, 1), 'query')
  }

  const es: Entry[] = insts
    .map((_, i): Entry => ({ kind: 'inst', i }))
    .filter((e) => e.kind !== 'inst' || !q || insts[e.i].name.toLowerCase().includes(q))
  es.push(o.draft ? { kind: 'draft' } : { kind: 'new' })
  const selE = o.draft
    ? es.length - 1
    : es.findIndex((e) => (e.kind === 'inst' ? e.i === o.sel : e.kind === 'new' && o.sel === insts.length))

  // Scroll by whole entries, keeping the selection in view; the rows above
  // and below the list say what's hidden.
  const h = L.list.h
  const rowsFor = (n: number) => (n - 1) * STRIDE + 1
  let start = 0
  let k = es.length
  if (rowsFor(es.length) > h) {
    k = Math.max(1, Math.floor((h - 1) / STRIDE))
    start = Math.max(0, Math.min(es.length - k, selE - k + 1))
  }
  if (start > 0) buf.lineRight(rect(4, 1, L.sbW - 5, 1), [[`↑ ${start} more`, { fg: t.faint }]])
  const hidden = es.length - start - k
  if (hidden > 0) buf.lineRight(rect(4, L.list.y + k * STRIDE, L.sbW - 5, 1), [[`↓ ${hidden} more`, { fg: t.faint }]])

  es.slice(start, start + k).forEach((e, j) => {
    const y = L.list.y + j * STRIDE
    const selected = start + j === selE
    const key = e.kind === 'inst' ? region(e.i) : e.kind === 'new' ? region(insts.length) : 'focus:0'
    const hov = c.hovered(key)
    const row = rect(0, y, L.sbW, 1)
    if (selected) buf.fill(row, { bg: o.focused ? t.selection : t.surface })
    else if (hov) buf.fill(row, { bg: mix(t.bg, t.surface, 0.6) })
    const nameSt: Style = selected ? { fg: t.strong, bold: true } : hov ? { fg: t.strong } : { fg: t.text }
    const numSt: Style = selected ? { fg: o.focused ? t.accent : t.subtle, bold: true } : { fg: t.faint }

    if (e.kind === 'new') {
      buf.text(2, y, '+', selected ? { fg: o.focused ? t.accent : t.subtle, bold: true } : { fg: t.muted })
      buf.text(4, y, 'new', selected ? nameSt : { fg: hov ? t.text : t.muted })
      buf.region(row, key)
      return
    }
    if (e.kind === 'draft') {
      buf.text(2, y, '+', { fg: t.accent, bold: true })
      buf.line(4, y, o.draft!.name, { fg: t.strong, bold: true }, L.sbW - 5)
      buf.line(4, y + 1, o.draft!.meta, { fg: t.muted }, L.sbW - 5)
      return
    }
    const inst = insts[e.i]
    buf.text(2, y, num(e.i), numSt)
    const a = attention(c, inst)
    const edge = L.sbW - 1
    const nameW = width(inst.name)
    let ax = edge
    if (a) {
      const pick = 4 + nameW + 2 + lineWidth(a.full) <= edge ? a.full : a.short
      ax = buf.lineRight(rect(4, y, edge - 4, 1), pick)
    }
    buf.line(4, y, highlight(c, inst.name, q, nameSt), {}, ax - 4 - (a ? 1 : 0))
    const meta = width(shortMeta(inst)) <= edge - 4 ? shortMeta(inst) : inst.mc
    buf.text(4, y + 1, meta, { fg: selected ? t.dim : t.muted }, edge - 4)
    buf.region(rect(0, y, L.sbW, 2), key)
  })

  if (q && es.length === 1) buf.text(4, L.list.y, `nothing matches “${o.query}”`, { fg: t.faint }, L.sbW - 5)
}

/** The account: one quiet line under the sidebar, loud only when sign-in expired. */
export function account(c: Ctx, L: Layout) {
  const { buf, t, data } = c
  const a = data.account
  const y = L.footY
  const row = rect(0, y, L.sbW, 1)
  const hov = c.hovered('screen:accounts')
  if (a.status === 'expired') {
    buf.fill(row, { bg: t.warningBg })
    const word = 2 + width(a.name) + 2 + 15 <= L.sbW - 3 ? '  sign-in expired' : '  expired'
    buf.line(2, y, [['▲ ', { fg: t.warning }], [a.name, { fg: t.strong, bold: true }], [word, { fg: t.warning }]], {}, L.sbW - 3)
  } else {
    buf.line(2, y, [['● ', { fg: mix(t.bg, t.success, 0.75) }], [a.name, { fg: hov ? t.text : t.dim }]], {}, L.sbW - 3)
  }
  buf.region(row, 'screen:accounts')
}

// ── The page ──────────────────────────────────────────────────────────────

/** The page's first two rows: the name, bold, over a dim line of facts. */
export function title(c: Ctx, P: Rect, name: Line, meta: Line, trail?: Line) {
  const { buf, t } = c
  const tx = trail ? buf.lineRight(rect(P.x, P.y, P.w, 1), trail) : right(P)
  buf.line(P.x, P.y, name, { fg: t.strong, bold: true }, tx - P.x - 2)
  buf.line(P.x, P.y + 1, meta, { fg: t.dim }, P.w)
}

/** The keys that work now, on the page's side of the bottom row. `?` never drops off. */
export function footer(c: Ctx, L: Layout, hints: [string, string][]) {
  const { buf, t } = c
  const y = L.footY
  const need = ([k, l]: [string, string]) => width(k) + 1 + width(l) + 3
  const last = hints[hints.length - 1]
  const limit = right(L.page) - (last ? need(last) : 0) + 3
  const fits: [string, string][] = []
  let used = L.page.x
  for (const h of hints.slice(0, -1)) {
    if (used + need(h) > limit) break
    fits.push(h)
    used += need(h)
  }
  if (last) fits.push(last)
  let x = L.page.x
  for (const [key, label] of fits) {
    const hov = c.hovered(`key:${key}`)
    const start = x
    x = buf.text(x, y, key, { fg: hov ? t.strong : t.subtle, bold: true })
    x = buf.text(x + 1, y, label, { fg: hov ? t.text : t.muted })
    buf.region(rect(start, y, x - start, 1), `key:${key}`)
    x += 3
  }
}

/** The one primary action: ` Play  ↵ `, a one-row pill. */
export function pill(c: Ctx, x: number, y: number, label: string, key: string, region: string, kind: 'primary' | 'quiet' | 'danger' = 'primary'): Rect {
  const { buf, t } = c
  const hov = c.hovered(region)
  const bg = kind === 'primary' ? (hov ? t.accentHi : t.accent) : kind === 'danger' ? t.error : hov ? t.surfaceHi : t.surface
  const fg = kind === 'quiet' ? (hov ? t.strong : t.text) : onColor(bg)
  const keyFg = kind === 'quiet' ? t.accent : mix(bg, fg, 0.75)
  const line: Line = [' ', [label, { fg, bold: kind !== 'quiet' }], [`  ${key}`, { fg: keyFg }], ' ']
  const r = rect(x, y, lineWidth(line), 1)
  buf.fill(r, { bg })
  buf.line(x, y, line, { bg })
  buf.region(r, region)
  return r
}

/** A group label, herdr-style: lowercase, muted, bold, with a faint count. Returns the end x. */
export function label(c: Ctx, x: number, y: number, text: string, count?: string | number): number {
  const { buf, t } = c
  const end = buf.text(x, y, text, { fg: t.muted, bold: true })
  return count === undefined ? end : buf.text(end + 2, y, String(count), { fg: t.faint })
}

/** A full-width row of the page, one column of air either side of the text. */
export const pageRow = (P: Rect, y: number) => rect(P.x - 1, y, P.w + 2, 1)

/** A page row's band: accent-tinted when its pane has focus, a whisper on hover. */
export function band(c: Ctx, r: Rect, selected: boolean, focused: boolean, key: string) {
  const { buf, t } = c
  const row = rect(r.x - 1, r.y, r.w + 2, 1)
  if (selected) buf.fill(row, { bg: focused ? t.selection : t.surface })
  else if (c.hovered(key)) buf.fill(row, { bg: mix(t.bg, t.surface, 0.6) })
  buf.region(row, key)
}

/** `name` with every match of `q` lit in the accent. */
export function highlight(c: Ctx, name: string, q: string, st: Style): Line {
  if (!q) return [[name, st]]
  const out: Line = []
  const lower = name.toLowerCase()
  const needle = q.toLowerCase()
  let at = 0
  for (let i = lower.indexOf(needle); i >= 0; i = lower.indexOf(needle, at)) {
    if (i > at) out.push([name.slice(at, i), st])
    out.push([name.slice(i, i + q.length), { ...st, fg: c.t.accent, bold: true }])
    at = i + q.length
  }
  if (at < name.length) out.push([name.slice(at), st])
  return out
}

/** A sub-cell progress bar, one row: `████████▌·········`. */
export function bar(c: Ctx, x: number, y: number, w: number, ratio: number) {
  const { buf, t } = c
  const EIGHTHS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉']
  const cells = Math.max(0, Math.min(1, ratio)) * w
  const full = Math.floor(cells)
  const part = Math.round((cells - full) * 8)
  const track = mix(t.bg, t.text, t.light ? 0.1 : 0.13)
  for (let i = 0; i < w; i++) {
    if (i < full) buf.set(x + i, y, '█', { fg: t.accent })
    else if (i === full && part > 0 && part < 8) buf.set(x + i, y, EIGHTHS[part], { fg: t.accent, bg: track })
    else if (i === full && part === 8) buf.set(x + i, y, '█', { fg: t.accent })
    else buf.set(x + i, y, ' ', { bg: track })
  }
}

// ── The log ───────────────────────────────────────────────────────────────

export const levelFg = (c: Ctx, l: LogLine) => (l.level === 'ERROR' ? c.t.error : l.level === 'WARN' ? c.t.warning : c.t.subtle)

/** `14:02:11  [Render thread]  text`, the thread column only when there's room. */
export function logLine(c: Ctx, x: number, y: number, w: number, l: LogLine, thread = false) {
  const { buf, t } = c
  let tx = buf.text(x, y, l.time, { fg: t.faint }) + 2
  if (thread) tx = buf.text(tx, y, l.thread.padEnd(15).slice(0, 15), { fg: t.muted }) + 1
  buf.text(tx, y, l.text, { fg: levelFg(c, l) }, x + w - tx)
}

