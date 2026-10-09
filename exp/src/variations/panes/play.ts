// Play: the running game's session stats, its log as a table, and the
// problems in it. Picking a problem (↑↓, the default panel) jumps the log to
// that line; tab moves the cursor into the log itself. ←→ picks the level.

import type { Ctx } from '../../kit'
import type { LogLine } from '../../data/mock'
import { type Line, type Rect, type Style, rect, cols, rows, right, bottom } from '../../term/buffer'
import * as f from './frame'
import { overlays } from './overlays'
import { logTail } from './data'

const LEVELS = ['All', 'Warnings', 'Errors'] as const

export function play(c: Ctx) {
  const { buf, t, data } = c
  const W = buf.cols
  const inst = data.instances[0]
  const body = rect(1, 0, W - 2, buf.rows - 1)
  const focus = c.panes(2)
  const level = c.tabs(LEVELS.length)

  const log = [...data.log, ...logTail]
  const shown = log
    .map((l, i) => ({ ...l, i }))
    .filter((l) => level === 0 || (level === 1 ? l.level !== 'INFO' : l.level === 'ERROR'))
  const problems = log
    .map((l, i) => ({ ...l, i }))
    .filter((l) => l.level !== 'INFO')
    .reverse()

  const sessH = body.w >= 124 ? 4 : 4
  const probH = 2 + Math.min(problems.length, body.h >= 36 ? 6 : body.h >= 28 ? 4 : 3)
  const [sessR, , logR, , probR] = rows(body, [sessH, 1, '1fr', 1, probH])

  const psel = focus === 0 ? c.sel(problems.length) : -1
  const lsel = focus === 1 ? c.sel(Math.max(1, shown.length)) : -1
  // Finding jumps to the newest match; otherwise the picked problem or line.
  const q = f.query(c).toLowerCase()
  const lastMatch = q ? [...shown].reverse().find((l) => l.text.toLowerCase().includes(q))?.i : undefined
  const target = focus === 1 ? shown[lsel]?.i : (lastMatch ?? problems[psel]?.i)

  session(c, sessR)
  logPanel(c, logR, shown, target ?? -1, focus === 1, level, problems, log.length)
  problemPanel(c, probR, problems, psel, focus === 0)

  const hints: [string, string][] = [
    ['s', 'stop'],
    ['←→', 'level'],
    ['/', 'find'],
    ['tab', focus === 0 ? 'log' : 'problems'],
    ['c', 'copy'],
    ['esc', 'home'],
    ['?', 'help'],
  ]
  f.statusBar(c, f.typing(c) ? { label: 'FIND' } : { label: '● RUNNING', tone: 'success' }, [[inst.name, { fg: t.text }]], hints)
  overlays(c, { screen: 'Game log', subject: inst.name })
}

function session(c: Ctx, r: Rect) {
  const { buf, t } = c
  const inner = f.panel(c, r, [['Survival 1.21', { fg: t.strong }]], {
    right: [['● ', { fg: t.success }], ['running ', { fg: t.success }], ['18m 04s', { fg: t.subtle }]],
  })
  type Fact = [string, Line, number?]
  const v = (a: string, b = ''): Line => [[a, { fg: t.text }], [b ? `  ${b}` : '', { fg: t.dim }]]
  const version: Fact = ['Version', v('1.21.4', 'Fabric 0.16.10')]
  const java: Fact = ['Java', v('21.0.5', 'arm64')]
  const account: Fact = ['Account', v('Steve')]
  const world: Fact = ['World', v('New World')]
  const pid: Fact = ['PID', v('48213')]
  const modsN: Fact = ['Mods', v('41 loaded')]
  const mem: Fact = ['Memory', v('2.1 / 4 GB'), 0.52]
  const cpu: Fact = ['CPU', v('38%'), 0.38]
  // Two rows of facts, column by column; meters take what's left.
  const groups: [Fact[], number | '1fr'][] =
    inner.w >= 120
      ? [[[version, java], 34], [[account, world], 24], [[pid, modsN], 20], [[mem, cpu], '1fr']]
      : inner.w >= 90
        ? [[[version, java], 34], [[account, world], 22], [[mem, cpu], '1fr']]
        : [[[version, java], 32], [[mem, cpu], '1fr']]
  const cs = cols(inner, groups.map(([, w]) => w), 3)
  groups.forEach(([facts], gi) => {
    const col = cs[gi]
    facts.forEach(([label, value, ratio], k) => {
      const y = inner.y + k
      const lw = 9
      buf.text(col.x, y, label, { fg: t.dim })
      if (ratio !== undefined) {
        const valW = 11
        const bw = Math.max(4, col.w - lw - valW - 2)
        const bx = f.bar(c, col.x + lw, y, bw, ratio, t.secondary)
        buf.lineRight(rect(bx, y, right(col) - bx, 1), value)
      } else buf.line(col.x + lw, y, value, {}, col.w - lw)
    })
  })
}

type Numbered = LogLine & { i: number }

function levelStyle(c: Ctx, l: LogLine): { tag: Style; msg: Style } {
  const { t } = c
  if (l.level === 'ERROR') return { tag: { fg: t.error, bold: true }, msg: { fg: t.error } }
  if (l.level === 'WARN') return { tag: { fg: t.warning, bold: true }, msg: { fg: t.text } }
  return { tag: { fg: t.faint }, msg: { fg: t.subtle } }
}

function logPanel(c: Ctx, r: Rect, shown: Numbered[], target: number, focused: boolean, level: number, problems: Numbered[], all: number) {
  const { buf, t } = c
  const errors = problems.filter((p) => p.level === 'ERROR').length
  const q = f.query(c).toLowerCase()
  const matches = q ? shown.filter((l) => l.text.toLowerCase().includes(q)).length : 0
  const inner = f.panel(c, r, 'Game log', {
    focused,
    region: 'focus:1',
    tabs: { items: [['All', all], ['Warnings', problems.length], ['Errors', errors]], active: level },
    right: q ? [[`${matches} match${matches === 1 ? '' : 'es'}`, { fg: t.accent }]] : undefined,
  })
  let top = inner.y
  if (f.typing(c) || q) {
    const row = rect(r.x, top, r.w, 1)
    buf.fill(row, { bg: f.typing(c) ? t.selection : t.surface })
    buf.text(inner.x, top, '/', { fg: t.accent, bold: true })
    const end = q ? buf.text(inner.x + 2, top, f.query(c), { fg: t.strong }) : inner.x + 2
    if (!q) buf.text(inner.x + 2, top, 'find in log', { fg: t.muted })
    if (f.typing(c)) buf.cursor = { x: end, y: top }
    buf.region(row, 'query')
    top++
  }
  const iw = inner.w
  const spec: f.Col[] = [{ label: 'Time', w: 8 }]
  if (iw >= 88) spec.push({ label: 'Thread', w: 13 })
  spec.push({ label: 'Level', w: 5 }, { label: 'Message', w: 'fr' })
  const list = rect(inner.x, top, inner.w, bottom(inner) - top)
  const p = f.place(list, spec)
  f.header(c, list.y, p)
  const fit = list.h - 1
  const n = shown.length
  const at = shown.findIndex((l) => l.i === target)
  // Follow the tail; scroll back only as far as the target needs.
  let start = Math.max(0, n - fit)
  if (at >= 0 && at < start + 1) start = Math.max(0, at - Math.floor(fit / 3))
  const newer = n - start - fit
  const foot: Line =
    newer > 0
      ? [['paused', { fg: t.subtle }], [`  ·  ${newer} newer  ·  `, { fg: t.muted }], ['G', { fg: t.accent, bold: true }], [' follow', { fg: t.dim }]]
      : [['following ↓', { fg: t.muted }]]
  buf.lineRight(rect(r.x, bottom(r) - 1, r.w - 2, 1), [' ', ...foot, ' '])
  shown.slice(start, start + fit).forEach((l, j) => {
    const y = list.y + 1 + j
    const row = rect(r.x, y, r.w, 1)
    const on = l.i === target
    const state: f.RowState = on ? (focused ? 'focus' : 'blur') : null
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${start + j}` : 'focus:1')
    const st = levelStyle(c, l)
    for (const col of p) {
      const v: Line =
        col.label === 'Time'
          ? [[l.time, { fg: t.muted }]]
          : col.label === 'Thread'
            ? [[l.thread, { fg: t.faint }]]
            : col.label === 'Level'
              ? [[l.level === 'INFO' ? 'info' : l.level === 'WARN' ? 'warn' : 'error', st.tag]]
              : f.highlight(c, l.text, f.query(c), on ? { ...st.msg, fg: l.level === 'INFO' ? t.strong : st.msg.fg } : st.msg)
      f.cell(c, col, y, v)
    }
  })
  if (!n) buf.text(list.x + 2, list.y + 1, 'Nothing at this level yet.', { fg: t.faint })
}

function problemPanel(c: Ctx, r: Rect, problems: Numbered[], sel: number, focused: boolean) {
  const { buf, t } = c
  const inner = f.panel(c, r, 'Problems', {
    focused,
    count: problems.length,
    region: 'focus:0',
    right: [['↵', { fg: t.accent, bold: true }], [' show in log', { fg: t.dim }]],
  })
  const narrow = inner.w < 60
  const p = f.place(inner, narrow ? [{ label: '', w: 8 }, { label: '', w: 'fr' }] : [{ label: '', w: 8 }, { label: '', w: 'fr' }, { label: '', w: 13 }])
  const fit = inner.h
  const start = f.scrollStart(problems.length, Math.max(0, sel), fit)
  problems.slice(start, start + fit).forEach((l, j) => {
    const i = start + j
    const y = inner.y + j
    const row = rect(r.x, y, r.w, 1)
    const state = f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${i}` : 'focus:0')
    const glyph: Line = l.level === 'ERROR' ? [['✗ ', { fg: t.error }]] : [['▲ ', { fg: t.warning }]]
    f.cell(c, p[0], y, [[l.time, { fg: t.muted }]])
    f.cell(c, p[1], y, [...glyph, [l.text, f.rowText(c, state)]])
    if (p[2]) f.cell(c, p[2], y, [[l.thread, { fg: t.faint }]])
  })
  const below = problems.length - start - fit
  if (below > 0) buf.lineRight(rect(r.x, bottom(r) - 1, r.w - 2, 1), [[` ↓ ${below} more `, { fg: t.muted }]])
}
