// The Columns direction's parts: the page frame, the breadcrumb header, column
// titles and rows, the status line and the footer. Everything is one row
// tall; columns are separated by whitespace, never by rules.

import type { Ctx } from '../../kit'
import { type Line, type Rect, type Style, rect, right, bottom, inset, lineWidth, width } from '../../term/buffer'

export const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n))

export interface Frame {
  header: Rect
  body: Rect
  status: Rect
  footer: Rect
  /** Two columns instead of three (80×24). */
  narrow: boolean
  /** Room to spare (140×40): rows grow an extra column. */
  wide: boolean
}

export function frame(c: Ctx): Frame {
  const area = inset(c.buf.area(), 1, 2)
  const footer = rect(area.x, bottom(area) - 1, area.w, 1)
  const status = rect(area.x, footer.y - 2, area.w, 1)
  const body = rect(area.x, area.y + 2, area.w, status.y - 1 - (area.y + 2))
  return { header: rect(area.x, area.y, area.w, 1), body, status, footer, narrow: area.w < 90, wide: area.w >= 120 }
}

/**
 * The column slots. Each slot reaches one cell past the text on both sides so
 * a selection band has a cell of air around its text while the text itself
 * lines up with the header. Fixed shares, like ranger: content slides
 * through the slots, the slots never move.
 */
export function slots(f: Frame): Rect[] {
  const o = rect(f.body.x - 1, f.body.y, f.body.w + 2, f.body.h)
  if (f.narrow) {
    const a = clamp(Math.round(o.w * 0.42), 28, 36)
    return [rect(o.x, o.y, a, o.h), rect(o.x + a + 2, o.y, o.w - a - 2, o.h)]
  }
  const gap = f.wide ? 3 : 2
  const a = clamp(Math.round(o.w * 0.26), 27, 34)
  const b = clamp(Math.round(o.w * 0.24), 24, 32)
  return [
    rect(o.x, o.y, a, o.h),
    rect(o.x + a + gap, o.y, b, o.h),
    rect(o.x + a + b + 2 * gap, o.y, o.w - a - b - 2 * gap, o.h),
  ]
}

/** Two slots joined into one (the log takes the preview's room too). */
export const join = (a: Rect, b: Rect) => rect(a.x, a.y, right(b) - a.x, a.h)

/** A slot's text area: one cell in from each side, below the title. */
export const textArea = (r: Rect) => rect(r.x + 1, r.y + 2, r.w - 2, r.h - 2)

// ── Header ────────────────────────────────────────────────────────────────

/** `mctui › Survival 1.21 › Mods`: the path, last crumb brightest. Account flush right. */
export function header(c: Ctx, r: Rect, crumbs: [string, string?][]) {
  const { buf, t, data } = c
  const acct = data.account
  const expired = acct.status === 'expired'
  const acctLine: Line = expired
    ? [['▲ ', { fg: t.warning }], [acct.name, { fg: t.warning }]]
    : [[acct.name, { fg: c.hovered('screen:accounts') ? t.text : t.dim }]]
  const ax = buf.lineRight(r, acctLine)
  buf.region(rect(ax, r.y, right(r) - ax, 1), 'screen:accounts')
  const room = ax - r.x - 3
  let x = buf.text(r.x, r.y, 'mctui', { fg: t.accent, bold: true })
  buf.region(rect(r.x, r.y, 5, 1), 'focus:0')
  crumbs.forEach(([label, region], i) => {
    const last = i === crumbs.length - 1
    if (x + 4 >= r.x + room) return
    x = buf.text(x + 1, r.y, '›', { fg: t.faint }) + 1
    const hov = !!region && c.hovered(region)
    const x0 = x
    x = buf.text(x, r.y, label, { fg: last ? t.text : hov ? t.text : t.dim }, r.x + room - x)
    if (region) buf.region(rect(x0, r.y, x - x0, 1), region)
  })
}

// ── Columns ───────────────────────────────────────────────────────────────

/** A column's title: the name of what it lists. Accent while the column has focus. */
export function title(
  c: Ctx,
  r: Rect,
  label: string,
  o: { count?: string | number; focused?: boolean; right?: Line; region?: string } = {},
) {
  const { buf, t } = c
  const row = rect(r.x + 1, r.y, r.w - 2, 1)
  const rw = o.right ? Math.min(lineWidth(o.right), row.w) : 0
  if (o.right) buf.lineRight(row, o.right)
  const room = row.w - (rw ? rw + 2 : 0)
  const x = buf.text(row.x, r.y, label, { fg: o.focused ? t.accent : t.muted, bold: true }, room)
  if (o.count !== undefined && x + 2 < row.x + room) buf.text(x + 2, r.y, String(o.count), { fg: t.faint }, row.x + room - x - 2)
  if (o.region) buf.region(rect(r.x, r.y, r.w, 1), o.region)
}

/** `/ sod▏          2 of 5,056`: the filter, typed into the focused column's title row. */
export function filterRow(c: Ctx, r: Rect, query: string, count: string) {
  const { buf, t, ui } = c
  const row = rect(r.x + 1, r.y, r.w - 2, 1)
  const cx = buf.lineRight(row, [[count, { fg: t.muted }]])
  buf.text(row.x, r.y, '/', { fg: t.accent, bold: true })
  const x = buf.text(row.x + 2, r.y, query, { fg: t.strong }, cx - row.x - 4)
  if (ui.typing) buf.cursor = { x, y: r.y }
  buf.region(rect(r.x, r.y, r.w, 1), 'query')
}

export interface Item {
  label: string
  /** Letters of `label` to light (a filter match). */
  hits?: number[] | null
  trail?: Line
  /** A middle column shown when the row has room (wide terminals). */
  extra?: Line
  /** An action row (`Add from Modrinth…`) or something switched off. */
  quiet?: boolean
  /** A blank row above this one: whitespace between groups, never items. */
  gap?: boolean
  /** A group heading above this row (`Installed`, `Modrinth`). */
  group?: Line
}

/**
 * How a column shows its selected row:
 * - focus: the accent-tinted band (only ever one on screen);
 * - path: a column left of focus, its row on the path: a neutral band;
 * - ahead: a column right of focus, the row the preview follows: bold, no band.
 */
export type Mark = 'focus' | 'path' | 'ahead' | 'none'
/** How loud a column's rows are: the focused one reads first. */
export type Tone = 'focus' | 'parent' | 'preview'

export interface ListOpts {
  items: Item[]
  sel: number
  mark: Mark
  tone: Tone
  /** Rows click as `sel:N@pane`. */
  pane?: number
  empty?: Line
  /** Width of the `extra` column, when the row has room for it. */
  extraW?: number
}

type DRow = { k: 'blank' } | { k: 'group'; line: Line } | { k: 'item'; i: number }

/** A column's rows, scrolled to keep the selection in view. */
export function list(c: Ctx, r: Rect, o: ListOpts) {
  const { buf, t } = c
  // Rows past ~60 cells make the eye travel from a name to its values.
  r = rect(r.x, r.y, Math.min(r.w, 60), r.h)
  if (!o.items.length) {
    if (o.empty) buf.line(r.x + 1, r.y, o.empty, { fg: t.muted }, r.w - 2)
    return
  }
  const rows: DRow[] = []
  let selRow = 0
  o.items.forEach((it, i) => {
    if ((it.gap || it.group) && rows.length) rows.push({ k: 'blank' })
    if (it.group) rows.push({ k: 'group', line: it.group })
    if (i === o.sel) selRow = rows.length
    rows.push({ k: 'item', i })
  })
  let start = 0
  let fit = r.h
  if (rows.length > r.h) {
    fit = r.h - 1
    start = clamp(selRow - fit + 2, 0, rows.length - fit)
  }
  // Extras line up in one column, left of the widest trail.
  const trailW = Math.max(0, ...o.items.filter((it) => it.extra).map((it) => (it.trail ? lineWidth(it.trail) : 0)))
  const shown = rows.slice(start, start + fit)
  shown.forEach((d, j) => {
    const y = r.y + j
    if (d.k === 'group') buf.line(r.x + 1, y, d.line, { fg: t.muted }, r.w - 2)
    if (d.k === 'item') row(c, rect(r.x, y, r.w, 1), o.items[d.i], d.i, o, trailW)
  })
  if (rows.length > fit) {
    const below = rows.slice(start + fit).filter((d) => d.k === 'item').length
    const above = rows.slice(0, start).filter((d) => d.k === 'item').length
    const y = r.y + fit
    const note = [above && `↑ ${above}`, below && `↓ ${below}`].filter(Boolean).join('  ')
    if (note) buf.text(r.x + 1, y, `${note} more`, { fg: t.faint })
  }
}

function row(c: Ctx, r: Rect, it: Item, i: number, o: ListOpts, trailW: number) {
  const { buf, t } = c
  const key = o.pane === undefined ? undefined : `sel:${i}@${o.pane}`
  const on = i === o.sel
  const hovered = !!key && c.hovered(key)
  const band = on && o.mark === 'focus' ? t.selection : (on && o.mark === 'path') || hovered ? t.surface : undefined
  if (band) buf.fill(r, { bg: band })
  let st: Style
  if (on && o.mark === 'focus') st = { fg: t.strong, bold: true }
  else if (on && (o.mark === 'path' || o.mark === 'ahead')) st = { fg: t.text, bold: true }
  else if (o.tone === 'focus') st = { fg: it.quiet ? t.dim : hovered ? t.strong : t.text }
  else if (o.tone === 'parent') st = { fg: it.quiet ? t.faint : hovered ? t.text : t.dim }
  else st = { fg: it.quiet ? t.dim : hovered ? t.text : t.subtle }
  const txt = rect(r.x + 1, r.y, r.w - 2, 1)
  // The label wins: a long value gives way first (keeping at least a few cells).
  const trailRoom = Math.max(6, txt.w - width(it.label) - 2)
  const tx = it.trail ? buf.lineRight(rect(right(txt) - Math.min(trailRoom, lineWidth(it.trail)), r.y, Math.min(trailRoom, lineWidth(it.trail)), 1), it.trail) : right(txt)
  let end = tx - 2
  if (it.extra && o.extraW) {
    const ex = right(txt) - trailW - 2 - o.extraW
    if (ex - 2 - txt.x >= Math.min(width(it.label), 14)) {
      buf.line(ex, r.y, it.extra, {}, o.extraW)
      end = ex - 2
    }
  }
  buf.line(txt.x, r.y, lit(c, it.label, it.hits, st), {}, end - txt.x)
  if (key) buf.region(r, key)
}

/** `label` with the letters at `hits` in the accent. */
export function lit(c: Ctx, label: string, hits: number[] | null | undefined, st: Style): Line {
  if (!hits?.length) return [[label, st]]
  const set = new Set(hits)
  const hi: Style = { ...st, fg: c.t.accent, bold: true }
  const out: Line = []
  let run = ''
  let runHi = false
  ;[...label].forEach((ch, i) => {
    const h = set.has(i)
    if (h !== runHi && run) {
      out.push([run, runHi ? hi : st])
      run = ''
    }
    runHi = h
    run += ch
  })
  if (run) out.push([run, runHi ? hi : st])
  return out
}

/** Word-wrapped text in `r`; returns rows used. */
export function para(c: Ctx, r: Rect, text: string, st: Style, maxRows = r.h): number {
  const lines = wrapText(text, r.w)
  const n = Math.min(lines.length, maxRows)
  lines.slice(0, n).forEach((l, i) => {
    const cut = i === n - 1 && lines.length > n ? `${l.replace(/\s*\S*$/, '')}…` : l
    c.buf.text(r.x, r.y + i, cut, st, r.w)
  })
  return n
}

function wrapText(s: string, max: number): string[] {
  const out: string[] = []
  let line = ''
  for (const word of s.split(/\s+/).filter(Boolean)) {
    const next = line ? `${line} ${word}` : word
    if (width(next) <= max) line = next
    else {
      if (line) out.push(line)
      line = word
    }
  }
  if (line) out.push(line)
  return out
}

/** A label/value row: the label dim in a fixed column. */
export function kv(c: Ctx, x: number, y: number, w: number, label: string, value: Line, labelW = 12) {
  c.buf.text(x, y, label, { fg: c.t.dim }, labelW)
  c.buf.line(x + labelW, y, value, { fg: c.t.text }, w - labelW)
}

/** A key and what it does, for inline next-step lines: `↵ play again`. */
export function hint(c: Ctx, x: number, y: number, pairs: [string, string][], max: number): number {
  const { buf, t } = c
  const end = x + max
  pairs.forEach(([k, label], i) => {
    const need = width(k) + 1 + width(label) + (i ? 3 : 0)
    if (x + need > end) return
    if (i) x += 3
    x = buf.text(x, y, k, { fg: t.text, bold: true })
    x = buf.text(x + 1, y, label, { fg: t.dim })
  })
  return x
}

/** Sub-cell progress: an eighth-block bar on a one-row track. */
export function bar(c: Ctx, x: number, y: number, w: number, ratio: number) {
  const { buf, t } = c
  if (w <= 0) return
  const cells = clamp(ratio, 0, 1) * w
  const full = Math.floor(cells)
  const eighth = Math.round((cells - full) * 8)
  buf.fill(rect(x, y, w, 1), { bg: t.surface })
  for (let i = 0; i < full; i++) buf.set(x + i, y, '█', { fg: t.accent })
  if (eighth > 0 && full < w) buf.set(x + full, y, '▏▎▍▌▋▊▉█'[eighth - 1], { fg: t.accent })
}

// ── Status line and footer ────────────────────────────────────────────────

/** One line for what's true right now; nothing when nothing is happening. */
export function status(c: Ctx, r: Rect) {
  const { buf, t, data } = c
  if (data.running) {
    const run = data.running
    const tail: Line = []
    if (run.warnings) tail.push([`▲ ${run.warnings}`, { fg: t.warning }])
    if (run.errors) tail.push([`${run.warnings ? '   ' : ''}✗ ${run.errors}`, { fg: t.error }])
    const tx = tail.length ? buf.lineRight(r, tail) : right(r)
    const head: Line = [
      ['● ', { fg: t.success }],
      [run.instance, { fg: t.text, bold: true }],
      [`  running ${run.elapsed}`, { fg: t.success }],
    ]
    buf.line(r.x, r.y, [...head, ...segments(c, [run.world, run.memory, `since ${run.since}`], tx - r.x - 2 - lineWidth(head))], {}, tx - r.x - 2)
    return
  }
  if (data.crash) {
    const k = data.crash
    buf.line(
      r.x,
      r.y,
      [
        ['▲ ', { fg: t.warning }],
        [k.instance, { fg: t.text, bold: true }],
        [` crashed ${k.when}`, { fg: t.warning }],
        [`  ·  ${k.reason.split(' — ').pop()}`, { fg: t.dim }],
      ],
      {},
      r.w,
    )
    return
  }
  if (data.tasks.length) {
    const [k, ...rest] = data.tasks
    const tail: Line = rest.length ? [[`${rest.length} queued`, { fg: t.muted }]] : []
    const tx = tail.length ? buf.lineRight(r, tail) : right(r)
    let x = buf.line(r.x, r.y, [[`${c.spinner()} `, { fg: t.accent }], [k.label, { fg: t.text }]], {}, tx - r.x - 30)
    const barW = clamp(tx - x - 30, 8, 16)
    bar(c, x + 2, r.y, barW, k.ratio)
    x = buf.text(x + 3 + barW, r.y, `${Math.round(k.ratio * 100)}%`, { fg: t.text })
    buf.text(x + 2, r.y, k.detail.split(' · ')[0], { fg: t.dim }, tx - x - 4)
    return
  }
  if (data.account.status === 'expired') {
    buf.line(
      r.x,
      r.y,
      [
        ['▲ ', { fg: t.warning }],
        ['Microsoft sign-in expired', { fg: t.warning }],
        [`  ·  ${data.account.name} can still play offline; online play needs a new sign-in`, { fg: t.dim }],
      ],
      {},
      r.w,
    )
  }
}

/** `  ·  a  ·  b`: as many whole segments as fit in `w`, never half of one. */
function segments(c: Ctx, parts: string[], w: number): Line {
  const out: Line = []
  let used = 0
  for (const p of parts) {
    const s = `  ·  ${p}`
    if (used + width(s) > w) break
    out.push([s, { fg: c.t.dim }])
    used += width(s)
  }
  return out
}

/** The keys that work right now: plain text, `?` always last. */
export function footer(c: Ctx, r: Rect, hints: [string, string][]) {
  const { buf, t } = c
  const all: [string, string][] = [...hints, ['?', 'help']]
  const need = (hs: [string, string][]) => hs.reduce((w, [k, l], i) => w + width(k) + 1 + width(l) + (i ? 3 : 0), 0)
  while (all.length > 2 && need(all) > r.w) all.splice(all.length - 2, 1)
  let x = r.x
  all.forEach(([k, label], i) => {
    if (i) x += 3
    const key = `key:${k}`
    const hov = c.hovered(key)
    const x0 = x
    x = buf.text(x, r.y, k, { fg: hov ? t.strong : t.text, bold: true })
    x = buf.text(x + 1, r.y, label, { fg: hov ? t.text : t.dim })
    buf.region(rect(x0, r.y, x - x0, 1), key)
  })
}
