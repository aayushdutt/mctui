// Refined game screen: the state of the game in one line on top (or, after a
// crash, what broke and the next key), then the log: a level filter, `/` to
// find, and it follows the end while you watch.

import type { Ctx } from '../../kit'
import type { LogLine } from '../../data/mock'
import { type Line, rect, right, bottom, lineWidth } from '../../term/buffer'
import { frame, header, footer, statusLine, helpSheet } from './frame'
import { affordance, crashNeeds, crashWords, elapsed, fitParts, hints, lit, type Hint } from './bits'

export function play(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  const inst =
    data.instances.find((i) => i.name === (data.running?.instance ?? data.crash?.instance)) ?? data.instances[0]
  header(c, f, [inst?.name ?? 'Game', 'Game'])
  c.panes(1)
  // s stops the game here (a no-op in the lab), not the lab's Settings shortcut.
  c.bind('s', 'key:')
  const run = data.running
  const crash = data.crash
  const X = f.x
  let y = f.body.y

  // ── The game's state ──
  const top = rect(X, y, f.w, 1)
  let aff: Line
  let affKey = '↵'
  if (crash) {
    const { sentence } = crashWords(crash.reason, f.w)
    const fix = inst ? crashNeeds(crash.reason, inst.mods) : null
    aff = affordance(c, '↵', 'try again')
    buf.line(X, y, [
      ['✗ ', { fg: t.error, bold: true }],
      [`Crashed ${crash.when}`, { fg: t.error, bold: true }],
      ['  ·  ', { fg: t.faint }],
      [crash.exit, { fg: t.dim }],
    ])
    buf.text(X, y + 1, sentence, { fg: t.strong }, f.w)
    hints(c, X, y + 2, [
      ['m', fix ? `update ${fix.mod}` : 'fix mods', 'screen:mods'],
      ['c', 'copy report'],
    ])
    y += 3
  } else if (run) {
    const e = elapsed(c, run.elapsed)
    aff = affordance(c, 's', 'stop')
    affKey = 's'
    buf.line(
      X,
      y,
      fitParts(
        c,
        [
          { line: [['● ', { fg: t.success }], ['Running ', { fg: t.success, bold: true }], [e.long, { fg: t.text }]], prio: 0 },
          { line: [[run.world, { fg: t.text }]], prio: 1 },
          { line: [[run.memory, { fg: t.dim }]], prio: 2 },
          { line: [[`since ${run.since}`, { fg: t.dim }]], prio: 3 },
        ],
        f.w - 12,
      ),
    )
    y += 1
  } else {
    aff = affordance(c, '↵', 'play')
    buf.line(
      X,
      y,
      [
        ['○ ', { fg: t.muted }],
        ['Not running', { fg: t.text }],
        ['  ·  ', { fg: t.faint }],
        [`last played ${inst?.lastPlayed ?? 'never'}`, { fg: t.dim }],
      ],
      {},
      f.w - 12,
    )
    y += 1
  }
  const ax = buf.lineRight(top, aff)
  buf.region(rect(ax, top.y, lineWidth(aff), 1), `key:${affKey}`)
  y += 1

  // ── The log ──
  const log: LogLine[] = [...data.log]
  if (crash) {
    const { cause } = crashWords(crash.reason)
    log.push(
      { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: cause || crash.reason },
      { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: crashWords(crash.reason).sentence.replace(/\.$/, '') },
      { time: '14:20:32', thread: 'main', level: 'INFO', text: `Game exited with ${crash.exit}` },
    )
  }
  const warns = log.filter((l) => l.level === 'WARN').length
  const errs = log.filter((l) => l.level === 'ERROR').length
  const tab = c.tabs(3)
  const levels: [string, number][] = [
    ['all', log.length],
    ['warnings', warns],
    ['errors', errs],
  ]
  // Level filter: words, the active one on a quiet pill.
  let x = X
  const ly = y
  x = buf.text(x, ly, crash || run ? 'LOG' : 'LAST SESSION', { fg: t.muted, bold: true }) + 3
  levels.forEach(([name, n], i) => {
    const on = tab === i
    const hov = c.hovered(`tab:${i}`)
    const w = name.length + String(n).length + 3
    if (on) buf.fill(rect(x, ly, w, 1), { bg: t.surfaceHi })
    else if (hov) buf.fill(rect(x, ly, w, 1), { bg: t.surface })
    buf.line(x + 1, ly, [
      [name, on ? { fg: t.strong, bold: true } : { fg: t.dim }],
      [` ${n}`, { fg: i === 2 && n ? t.error : i === 1 && n ? t.warningSoft : t.faint }],
    ])
    buf.region(rect(x, ly, w, 1), `tab:${i}`)
    x += w + 1
  })

  const q = ui.query.toLowerCase()
  const byLevel = log.filter((l) => tab === 0 || (tab === 1 ? l.level === 'WARN' : l.level === 'ERROR'))
  const lines = q ? byLevel.filter((l) => l.text.toLowerCase().includes(q)) : byLevel
  const right0: Line =
    ui.typing || q
      ? [
          ['/ ', { fg: t.accent, bold: true }],
          [ui.query || ' ', { fg: t.strong }],
          [`   ${lines.length} line${lines.length === 1 ? '' : 's'}`, { fg: t.muted }],
        ]
      : run
        ? [['following the end', { fg: t.faint }]]
        : [[`${lines.length} lines`, { fg: t.faint }]]
  const rx = buf.lineRight(rect(x + 2, ly, right(f.body) - x - 2, 1), right0)
  if (ui.typing) buf.cursor = { x: rx + 2 + ui.query.length, y: ly }

  const body = rect(X, ly + 1, f.w, bottom(f.body) - ly - 1)
  const threadW = f.w >= 120 ? 14 : 0
  const shown = lines.slice(-body.h)
  shown.forEach((l, i) => {
    const ry = body.y + i
    let lx = buf.text(X, ry, l.time, { fg: t.faint }) + 2
    const [glyph, gfg, tfg] =
      l.level === 'ERROR'
        ? ['✗', t.error, t.error]
        : l.level === 'WARN'
          ? ['▲', t.warning, t.text]
          : [' ', t.faint, /\[CHAT\]|joined the game/.test(l.text) ? t.text : t.dim]
    buf.text(lx, ry, glyph, { fg: gfg })
    lx += 2
    if (threadW) {
      buf.text(lx, ry, l.thread, { fg: t.muted }, threadW)
      lx += threadW + 2
    }
    const at = q ? [...Array(q.length)].map((_, k) => l.text.toLowerCase().indexOf(q) + k) : []
    lit(c, lx, ry, l.text, at, { fg: tfg }, right(body) - lx)
  })
  if (!shown.length) buf.text(X, body.y, q ? `No line has “${ui.query}”` : 'Nothing at this level.', { fg: t.dim })

  // Downloads still show below; the game's own state is already on top.
  statusLine({ ...c, data: { ...data, running: undefined, crash: undefined } }, f)
  const keys: Hint[] = ui.typing
    ? [['↵', 'done'], ['esc', 'clear']]
    : crash
      ? [['↵', 'try again'], ['m', 'fix mods'], ['c', 'copy report'], ['←→', 'level'], ['esc', 'back']]
      : run
        ? [['s', 'stop'], ['←→', 'level'], ['/', 'find'], ['c', 'copy'], ['esc', 'back']]
        : [['↵', 'play'], ['←→', 'level'], ['/', 'find'], ['esc', 'back']]
  footer(c, f, keys)
  c.enter(crash || !run ? 'home' : null)
  if (ui.overlay === 'help')
    helpSheet(c, f, 'Game · keys', [
      ['Log', [['←→  1-3', 'level'], ['/', 'find'], ['↑↓', 'scroll'], ['G', 'follow the end']]],
      ['Game', [['s', 'stop'], ['↵', 'play again'], ['c', 'copy log']]],
      ['Go', [['m', 'mods'], ['esc', 'home']]],
    ])
}
