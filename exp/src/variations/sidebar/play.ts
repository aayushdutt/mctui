// Play: the running instance's page, all log. Same title and status line as
// home's live page (so ↵ just lets the log take the room), a level filter,
// and the tail followed. The sidebar stays; 1–9 still jump.

import type { Ctx } from '../../kit'
import type { LogLine } from '../../data/mock'
import { type Line, rect, bottom } from '../../term/buffer'
import * as p from './parts'
import { SESSION, CRASH_TAIL } from './data'
import { status } from './home'
import { helpPage } from './help'

export function play(c: Ctx) {
  const { buf, t, data, ui } = c
  const L = p.layout(c)
  const crash = data.crash
  const run = crash ? undefined : (data.running ?? SESSION)
  const name = crash?.instance ?? run!.instance
  const idx = Math.max(0, data.instances.findIndex((i) => i.name === name))
  const inst = data.instances[idx]
  const focus = c.panes(2)
  if (!inst) {
    p.sidebar(c, L, { sel: 0, focused: true })
    p.account(c, L)
    p.title(c, L.page, ['Nothing is running'], [['Make an instance first: n.', { fg: t.dim }]])
    return p.footer(c, L, [['n', 'new instance'], ['esc', 'back'], ['?', 'help']])
  }
  // The sidebar marks whose session this is (with its live timer); clicking another goes home to it.
  const live = run ? { ...c, data: { ...data, running: run } } : c
  p.sidebar(live, L, { sel: idx, focused: focus === 1, region: () => 'screen:home' })
  p.account(c, L)
  if (ui.overlay && ui.overlay !== 'confirm') return helpPage(c, L)

  const P = L.page
  p.title(c, P, [inst.name], [
    [`${inst.mc} · ${p.loaderName(inst)}`, { fg: t.dim }],
    [run ? ` · ${run.world} as ${data.account.name}` : ` · ${crash!.exit}`, { fg: t.muted }],
  ])

  if (run) status(live, P, P.y + 3)
  else {
    const row = p.pageRow(P, P.y + 3)
    buf.fill(row, { bg: t.errorBg })
    buf.line(P.x, P.y + 3, [['✗ ', { fg: t.error, bold: true }], ['Crashed ', { fg: t.strong, bold: true }], [`${crash!.when} · after 18m`, { fg: t.text }]], {}, P.w - 1)
  }

  // The level filter: ←→ in the lab, `v` in the app (1–9 stay instance jumps).
  const log: LogLine[] = crash ? [...data.log, ...CRASH_TAIL] : data.log
  const counts = { WARN: log.filter((l) => l.level === 'WARN').length, ERROR: log.filter((l) => l.level === 'ERROR').length }
  const levels: [string, number][] = [
    ['all', log.length],
    ['problems', counts.WARN + counts.ERROR],
    ['errors', counts.ERROR],
  ]
  const tab = c.tabs(levels.length)
  const ty = P.y + 5
  let x = P.x
  levels.forEach(([label, n], i) => {
    const on = i === tab
    const hov = c.hovered(`tab:${i}`)
    const start = x
    const line: Line = [[label, on ? { fg: t.strong, bold: true } : { fg: hov ? t.text : t.dim }], [` ${n}`, { fg: on ? t.subtle : t.faint }]]
    if (on) {
      const w = label.length + String(n).length + 3
      buf.fill(rect(x - 1, ty, w, 1), { bg: focus === 0 ? t.selection : t.surface })
    }
    x = buf.line(x, ty, line)
    buf.region(rect(start - 1, ty, x - start + 2, 1), `tab:${i}`)
    x += 4
  })
  buf.lineRight(rect(P.x, ty, P.w, 1), crash ? [['ended', { fg: t.faint }]] : [['following', { fg: t.muted }], [' ↓', { fg: t.success }]])

  const shown = log.filter((l) => (tab === 0 ? true : tab === 1 ? l.level !== 'INFO' : l.level === 'ERROR'))
  const body = rect(P.x, ty + 2, P.w, bottom(P) - ty - 2)
  // The newest line sits on the last row once the log outgrows the page.
  shown.slice(-body.h).forEach((l, i) => p.logLine(c, body.x, body.y + i, body.w, l, L.wide))
  if (!shown.length) buf.text(P.x, body.y, 'Nothing at this level yet.', { fg: t.faint })
  buf.region(body, 'focus:0')

  p.footer(
    c,
    L,
    crash
      ? [['↵', 'try again'], ['c', 'copy report'], ['m', 'fix mods'], ['←→', 'level'], ['esc', 'back'], ['?', 'help']]
      : [['s', 'stop'], ['←→', 'level'], ['c', 'copy'], ['/', 'find'], ['esc', 'back'], ['?', 'help']],
  )
  c.enter(null)
}
