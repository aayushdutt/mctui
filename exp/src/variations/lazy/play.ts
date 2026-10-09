// Play: the main view focused and given over to the game's log (lazygit's
// command output). One header line says how the session is going; the level
// filter lives in the rule; the side panels stay where they were.
//
// Panes: 0 the log. Tabs: All · Warnings · Errors.

import type { Ctx } from '../../kit'
import type { Running } from '../../data/mock'
import { type Line, rect } from '../../term/buffer'
import * as f from './frame'
import { sideStack } from './side'
import { statusLine } from './home'
import * as mv from './main'
import { crashTail } from './data'

const LEVELS = ['All', 'Warnings', 'Errors'] as const

export function play(c0: Ctx) {
  // Opening the log means a game was launched: the idle mock gets one.
  const run: Running = c0.data.running ?? {
    instance: 'Survival 1.21',
    elapsed: '18m',
    since: '14:02',
    world: 'New World',
    warnings: 3,
    errors: 1,
    memory: '2.1 of 4 GB',
  }
  const crashed = !!c0.data.crash
  const c: Ctx = { ...c0, data: { ...c0.data, running: crashed ? undefined : run } }
  const { buf, t, data, ui } = c
  const L = f.layout(c)
  c.panes(1)
  const level = c.tabs(LEVELS.length)
  const inst = data.instances.find((i) => i.name === run.instance) ?? data.instances[0]
  const isel = Math.max(0, data.instances.indexOf(inst))

  statusLine(c, L.status, false)
  sideStack(c, L.side, 0, data.instances, inst ?? null, [
    { focused: false, sel: isel, pane: null },
    { focused: false, sel: -1, pane: null },
    { focused: false, sel: -1, pane: null },
    { focused: false, sel: -1, pane: null },
  ])

  const log = crashed ? [...data.log.slice(0, 12), ...crashTail] : data.log
  const warns = log.filter((l) => l.level === 'WARN').length
  const errs = log.filter((l) => l.level === 'ERROR').length
  const shown = log.filter((l) => level === 0 || (level === 1 ? l.level === 'WARN' : l.level === 'ERROR'))
  const q = f.query(c)
  const m = L.main

  if (ui.overlay === 'help') {
    mv.helpView(c, m, 'Game log', keys(crashed).filter(([k]) => k !== 'esc'))
    f.keyLine(c, L.keys, [['esc', 'close']], [])
    return
  }

  // The level filter, set into the rule as words: the active one lit.
  const narrow = m.w < 50
  const labels = narrow ? ['all', 'warn', 'err'] : ['all', 'warnings', 'errors']
  const counts = [log.length, warns, errs]
  const tabs: Line[] = labels.map((label, i) => {
    const on = i === level
    const hov = c.hovered(`tab:${i}`)
    return [
      [label, on ? { fg: t.strong, bold: true, underline: true } : { fg: hov ? t.text : t.dim, bold: false }],
      [` ${counts[i]}`, { fg: on ? t.subtle : t.muted, bold: false }],
    ]
  })
  const r = mv.mainFrame(c, m, ['Log'], { focused: true, segs: tabs, segRegions: tabs.map((_, i) => `tab:${i}`) })

  // The session in one line (or the find field while finding).
  if (f.typing(c) || q) {
    const row = rect(m.x, r.y, m.w, 1)
    buf.fill(row, { bg: f.typing(c) ? t.selection : t.surface })
    buf.text(r.x, r.y, '/', { fg: t.accent, bold: true })
    const end = q ? buf.text(r.x + 2, r.y, q, { fg: t.strong }) : r.x + 2
    if (!q) buf.text(r.x + 2, r.y, 'find in the log', { fg: t.muted })
    if (f.typing(c)) buf.cursor = { x: end, y: r.y }
    const hits = q ? shown.filter((l) => l.text.toLowerCase().includes(q.toLowerCase())).length : 0
    if (q) buf.lineRight(rect(r.x, r.y, r.w, 1), [[`${hits} line${hits === 1 ? '' : 's'}`, { fg: t.subtle }]])
    buf.region(row, 'query')
  } else if (crashed) {
    buf.text(m.x, r.y, '✗', { fg: t.error })
    buf.line(r.x, r.y, [[`crashed ${data.crash!.when}`, { fg: t.error, bold: true }], ['  ·  ', { fg: t.faint }], [data.crash!.exit, { fg: t.dim }]], {}, r.w)
  } else {
    // A live timer, the world, memory: how the session is going.
    buf.text(m.x, r.y, '●', { fg: t.success })
    const head: Line = [
      ['running ', { fg: t.success, bold: true }],
      [f.elapsed(c, run.elapsed), { fg: t.success, bold: true }],
      ['  ·  ', { fg: t.faint }],
      [run.world, { fg: t.text }],
      ...(r.w >= 50 ? ([['  ·  ', { fg: t.faint }], [run.memory, { fg: t.dim }]] as Line) : []),
    ]
    buf.line(r.x, r.y, head, {}, r.w)
    if (r.w >= 60) buf.lineRight(rect(r.x, r.y, r.w, 1), [[`since ${run.since}`, { fg: t.muted }]])
  }

  const body = rect(r.x, r.y + 2, r.w, r.h - 2)
  if (!shown.length) buf.text(body.x, body.y, 'Nothing at this level.', { fg: t.faint })
  else mv.logTail(c, body, shown, { gutter: true, q, top: level > 0 })

  c.enter(null)
  f.keyLine(c, L.keys, f.typing(c) ? [['↵', 'keep'], ['esc', 'clear']] : keys(crashed))
}

function keys(crashed: boolean): f.Hint[] {
  return crashed
    ? [['↵', 'try again'], ['m', 'fix mods'], ['←→', 'level'], ['/', 'find'], ['c', 'copy report'], ['esc', 'home']]
    : [['s', 'stop'], ['←→', 'level'], ['/', 'find'], ['c', 'copy'], ['G', 'follow'], ['esc', 'home']]
}
