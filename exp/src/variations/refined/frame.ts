// Refined's chrome: a one-line header (where you are, who you are), the body,
// a status line that exists only while something is happening, and a footer
// of the keys that work right now. Every row is one line; nothing is boxed.

import type { Ctx } from '../../kit'
import { type Line, type Rect, rect, right, lineWidth, width, truncate } from '../../term/buffer'
import { mix } from '../../term/theme'
import { bar, elapsed, fitParts, hints, hintsWidth, pct, crashWords, type Hint } from './bits'

export interface Frame {
  /** The text column: header, labels, rows and keys all start here. */
  x: number
  w: number
  header: Rect
  body: Rect
  status: Rect
  footer: Rect
}

export function frame(c: Ctx): Frame {
  const { cols, rows } = c.buf
  const x = cols >= 120 ? 4 : 3
  const w = cols - 2 * x
  const top = 1
  const bot = rows >= 26 ? 1 : 0
  const header = rect(x, top, w, 1)
  const footer = rect(x, rows - 1 - bot, w, 1)
  // The status row is reserved even when empty, so the body never resizes.
  const status = rect(x, footer.y - 1, w, 1)
  const body = rect(x, top + 2, w, status.y - 1 - (top + 2))
  return { x, w, header, body, status, footer }
}

/** `mctui › Survival 1.21 › Mods`, and the account: quiet, loud only when expired. */
export function header(c: Ctx, f: Frame, crumbs: string[] = []) {
  const { buf, t, data } = c
  const r = f.header
  let x = buf.text(r.x, r.y, 'mctui', { fg: t.accent, bold: true })
  crumbs.forEach((crumb, i) => {
    x = buf.text(x + 1, r.y, '›', { fg: t.faint })
    x = buf.text(x + 1, r.y, crumb, i === crumbs.length - 1 ? { fg: t.text, bold: true } : { fg: t.dim })
  })
  const acct = data.account
  if (acct.status === 'expired') {
    // One loud pill, and the key that fixes it.
    const line: Line = [
      ['▲ ', { fg: t.warning, bold: true }],
      [`${acct.name}'s sign-in expired`, { fg: t.warning }],
    ]
    const keys: Line = [
      ['a', { fg: t.strong, bold: true }],
      [' sign in', { fg: t.muted }],
    ]
    const kx = right(r) - lineWidth(keys)
    buf.line(kx, r.y, keys)
    const pw = lineWidth(line) + 2
    const px = kx - 3 - pw
    buf.fill(rect(px, r.y, pw, 1), { bg: t.warningBg })
    buf.line(px + 1, r.y, line)
    buf.region(rect(px, r.y, right(r) - px, 1), 'screen:accounts')
  } else {
    const line: Line = [[acct.name, { fg: c.hovered('screen:accounts') ? t.text : t.dim }]]
    const start = buf.lineRight(r, line)
    buf.region(rect(start, r.y, right(r) - start, 1), 'screen:accounts')
  }
}

/** The footer: keys that work now on the left, help on the right. */
export function footer(c: Ctx, f: Frame, list: Hint[], end: Hint[] = [['?', 'help']]) {
  const r = f.footer
  const ew = end.length ? hintsWidth(end) : 0
  // The left list drops whole hints when narrow; help never moves.
  hints(c, r.x, r.y, list, r.w - ew - 4)
  if (end.length) hints(c, right(r) - ew, r.y, end, ew)
}

/**
 * The status line: what's happening outside the screen, one row, only while
 * it exists (a running game, a crash, downloads). Returns whether it drew.
 */
export function statusLine(c: Ctx, f: Frame, opts: { selected?: string; keys?: boolean } = {}): boolean {
  const { buf, t, data } = c
  const r = f.status
  const band = rect(r.x - 1, r.y, r.w + 2, 1)
  const run = data.running
  const crash = data.crash
  const tasks = data.tasks
  if (!run && !crash && !tasks.length) return false
  buf.fill(band, { bg: t.surface })
  let keys: Hint[] = []
  let line: Line = []
  // Keys only when the subject isn't already selected (the footer has them then).
  const showKeys = (name: string) => opts.keys !== false && opts.selected !== name
  if (crash) {
    if (showKeys(crash.instance)) keys = [['l', 'log', 'screen:play']]
    const room = r.w - (keys.length ? hintsWidth(keys) + 3 : 0)
    // Longest that fits: with when, without it, then just the headline.
    const head = (when: boolean): Line => [
      ['✗ ', { fg: t.error, bold: true }],
      [crash.instance, { fg: t.strong, bold: true }],
      [when ? ` crashed ${crash.when}` : ' crashed', { fg: t.error }],
    ]
    const withWhy = (h: Line): Line => {
      const { sentence } = crashWords(crash.reason, room - lineWidth(h) - 5)
      return [...h, ['  ·  ', { fg: t.faint }], [sentence, { fg: t.text }]]
    }
    line = [withWhy(head(true)), withWhy(head(false)), head(true)].find((l) => lineWidth(l) <= room) ?? head(false)
  } else if (run) {
    const e = elapsed(c, run.elapsed)
    if (showKeys(run.instance)) keys = [['l', 'log', 'screen:play']]
    const room = r.w - (keys.length ? hintsWidth(keys) + 3 : 0)
    const parts: { line: Line; prio: number }[] = [
      {
        line: [['● ', { fg: t.success }], [run.instance, { fg: t.strong, bold: true }], [' running ', { fg: t.dim }], [e.long, { fg: t.text }]],
        prio: 0,
      },
      { line: [[run.world, { fg: t.dim }]], prio: 3 },
    ]
    if (run.warnings) parts.push({ line: [[`${run.warnings} warnings`, { fg: t.warningSoft }]], prio: 2 })
    if (run.errors) parts.push({ line: [[`${run.errors} error${run.errors > 1 ? 's' : ''}`, { fg: t.error }]], prio: 1 })
    line = fitParts(c, parts, room)
  }
  const kw = keys.length ? hintsWidth(keys) : 0
  const kx = right(r) - kw
  if (keys.length) hints(c, kx, r.y, keys, kw)
  let x = r.x
  let room = (keys.length ? kx - 3 : right(r)) - x
  if (line.length) {
    x = buf.line(x, r.y, line, {}, room)
    room = (keys.length ? kx - 3 : right(r)) - x
  }
  // Downloads: the first as a live bar, the rest counted.
  const active = tasks.filter((k) => k.detail !== 'queued')
  const queued = tasks.length - active.length
  if (tasks.length && room > 20) {
    if (line.length) x += 3
    const task = active[0]
    if (task) {
      x = buf.text(x, r.y, `${c.spinner()} `, { fg: t.accent })
      const label = task.label.replace(/^Downloading /, '')
      const tail = `  ${pct(task.ratio).padStart(4)}${queued ? `  ·  ${queued} queued` : ''}`
      const barW = Math.max(6, Math.min(16, Math.floor(room / 5)))
      const lw = Math.max(0, room - width(tail) - barW - 4 - 2)
      x = buf.text(x, r.y, truncate(label, lw), { fg: t.text }) + 2
      bar(c, x, r.y, barW, task.ratio, t.accent, mix(t.surface, t.text, 0.14))
      x += barW
      x = buf.text(x, r.y, `  ${pct(task.ratio).padStart(4)}`, { fg: t.accent, bold: true })
      if (queued) buf.text(x, r.y, `  ·  ${queued} queued`, { fg: t.dim })
    } else {
      buf.text(x, r.y, `○ ${queued} queued`, { fg: t.dim })
    }
    if (!keys.length) buf.region(band, 'key:downloads')
  }
  return true
}

/** A one-row question in the status line: `Delete “X”? …  y delete  esc keep`. */
export function ask(c: Ctx, f: Frame, question: Line, keys: Hint[]) {
  const { buf, t } = c
  const r = f.status
  buf.fill(rect(r.x - 1, r.y, r.w + 2, 1), { bg: t.errorBg })
  const kw = hintsWidth(keys)
  buf.line(r.x, r.y, question, {}, r.w - kw - 3)
  hints(c, right(r) - kw, r.y, keys, kw)
}

/**
 * Help: a sheet anchored to the bottom, over the faded page. Columns of
 * keys; the footer turns into `esc close`.
 */
export function helpSheet(c: Ctx, f: Frame, title: string, groups: [string, [string, string][]][]) {
  const { buf, t } = c
  buf.fade(t.backdrop)
  buf.region(buf.area(), 'close')
  const colW = 22
  const perRow = Math.max(1, Math.min(groups.length, Math.floor((f.w + 4) / colW)))
  const rowsOf = (gs: typeof groups) => Math.max(...gs.map(([, k]) => k.length)) + 1
  const bands: (typeof groups)[] = []
  for (let i = 0; i < groups.length; i += perRow) bands.push(groups.slice(i, i + perRow))
  const h = bands.reduce((s, b) => s + rowsOf(b) + 1, 0) + 1
  const top = Math.max(f.body.y, f.footer.y - 1 - h)
  buf.clear(rect(0, top - 1, buf.cols, buf.rows - top + 1))
  buf.hline(f.x - 1, top - 1, f.w + 2, { fg: t.rule })
  buf.text(f.x, top - 1, ` ${title} `, { fg: t.accent, bold: true })
  buf.region(rect(0, top - 1, buf.cols, buf.rows - top + 1), 'card')
  let y = top + 1
  for (const b of bands) {
    b.forEach(([name, keys], j) => {
      const x = f.x + j * Math.floor(f.w / perRow)
      buf.text(x, y, name.toUpperCase(), { fg: t.muted, bold: true })
      const kw = Math.max(...keys.map(([k]) => width(k))) + 2
      keys.forEach(([k, d], i) => {
        if (y + 1 + i >= f.footer.y - 1) return
        buf.text(x, y + 1 + i, k, { fg: t.strong, bold: true }, kw)
        buf.text(x + kw, y + 1 + i, d, { fg: t.dim }, Math.floor(f.w / perRow) - kw - 2)
      })
    })
    y += rowsOf(b) + 1
  }
  hints(c, f.footer.x, f.footer.y, [['esc', 'close', 'close']])
}
