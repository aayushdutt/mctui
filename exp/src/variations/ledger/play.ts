// Play: the game log as a table that follows the tail, a one-line session
// header, and a level filter that lives on the table's header row.

import type { Ctx } from '../../kit'
import type { LogLine } from '../../data/mock'
import { type Line, rect, right, width } from '../../term/buffer'
import { CRASH_TAIL, LOG_TAIL, sessionOf } from './data'
import { type Col, type Hint, cell, fit, footer, frame, head, lit, topLine } from './ui'
import { overlay } from './overlays'

const LEVELS = ['all', 'warn', 'error'] as const

export function play(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  const s = sessionOf(data)
  const crashed = !!data.crash
  const log: LogLine[] = crashed ? [...data.log, ...CRASH_TAIL] : [...data.log, ...LOG_TAIL.slice(1)]
  const level = c.tabs(LEVELS.length)
  const counts = [log.length, log.filter((l) => l.level === 'WARN').length, log.filter((l) => l.level === 'ERROR').length]
  const shown = log.filter((l) => level === 0 || (level === 1 ? l.level !== 'INFO' : l.level === 'ERROR'))

  // ── The session, in one line ──
  const seg = (x: string): [string, { fg: string }] => [`  ·  ${x}`, { fg: t.dim }]
  const session: Line = crashed
    ? [[`✗ crashed ${data.crash!.when}`, { fg: t.error }], seg(data.crash!.exit), seg(`ran ${s.elapsed}`)]
    : [[`● running ${s.elapsed}`, { fg: t.success }], seg(s.world), seg(s.memory), seg(`as ${data.account.name}`)]
  topLine(c, f, [s.instance, 'log'], session)

  // ── The table ──
  const cols: Col<LogLine>[] = [
    { key: 'time', label: 'TIME', w: 8, cell: (l) => [[l.time, { fg: t.faint }]] },
    {
      key: 'level',
      label: 'LEVEL',
      w: 5,
      cell: (l) => [[l.level, l.level === 'ERROR' ? { fg: t.error, bold: true } : l.level === 'WARN' ? { fg: t.warning } : { fg: t.muted }]],
    },
  ]
  if (f.w >= 116) cols.push({ key: 'thread', label: 'THREAD', w: 13, cell: (l) => [[l.thread, { fg: t.muted }]] })
  cols.push({
    key: 'message',
    label: 'MESSAGE',
    w: 40,
    flex: true,
    max: 400,
    cell: (l, w) => lit(c, l.text, undefined, { fg: l.level === 'INFO' ? t.dim : t.text }, w),
  })
  const placed = fit(cols, f.x, f.w, 3)
  head(c, f.head, placed)

  // The level filter sits at the header row's right end: `all 27  warn 4  error 1`.
  let x = right(rect(f.x, f.head, f.w, 1))
  for (let i = LEVELS.length - 1; i >= 0; i--) {
    const on = i === level
    const label = `${LEVELS[i]} ${counts[i]}`
    const w = width(label) + 2
    x -= w
    const r = rect(x, f.head, w, 1)
    if (on) buf.fill(r, { bg: t.selection })
    else if (c.hovered(`tab:${i}`)) buf.fill(r, { bg: t.surface })
    buf.line(x + 1, f.head, [
      [LEVELS[i], on ? { fg: t.strong, bold: true } : { fg: t.dim }],
      [` ${counts[i]}`, { fg: on ? t.subtle : i === 2 && counts[2] ? t.error : i === 1 && counts[1] ? t.warning : t.muted }],
    ])
    buf.region(r, `tab:${i}`)
    x -= 1
  }

  // Follows the tail: the newest lines that fit, oldest at the top.
  const rows = f.foot - 2 - f.body
  const tail = shown.slice(-rows)
  tail.forEach((l, j) => {
    const y = f.body + j
    // The line that says why it crashed gets the one band on screen.
    if (crashed && l.text.includes('needs Fabric API')) buf.fill(rect(f.x - 1, y, f.w + 2, 1), { bg: t.errorBg })
    else if (c.hovered(`line:${j}`)) buf.fill(rect(f.x - 1, y, f.w + 2, 1), { bg: t.surface })
    for (const p of placed) cell(c, p, y, p.col.cell(l, p.w, false))
    buf.region(rect(f.x - 1, y, f.w + 2, 1), `line:${j}`)
  })

  // ── Footer ──
  const hints: Hint[] = crashed
    ? [['↵', 'try again'], ['u', 'fix mods'], ['c', 'copy report'], ['v', 'level'], ['/', 'find'], ['esc', 'home'], ['?', 'help']]
    : [['x', 'stop'], ['v', 'level'], ['/', 'find'], ['c', 'copy'], ['esc', 'home'], ['?', 'help']]
  const hidden = log.length - shown.length
  footer(c, f, hints, [
    [hidden ? `${hidden} lines hidden  ·  ` : '', { fg: t.faint }],
    [crashed ? 'ended 14:20:32' : '↓ following', { fg: crashed ? t.muted : t.subtle }],
  ])
  c.enter(crashed ? 'home' : null)
  if (ui.overlay) overlay(c, f, f.foot - 8, rect(f.x, f.foot - 7, f.w, 5), 'play', null, null)
}
