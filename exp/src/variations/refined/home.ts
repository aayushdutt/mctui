// Refined home: a compact table of instances on the left (one row each,
// aligned columns, a state column that's empty unless something is true),
// and a composed preview sheet of the selected one on the right.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { type Line, type Rect, rect, right, bottom, width, wrap, lineWidth } from '../../term/buffer'
import { mix } from '../../term/theme'
import { frame, header, footer, statusLine, ask, helpSheet, type Frame } from './frame'
import {
  affordance,
  agoColor,
  balance,
  fitParts,
  label,
  cutAt,
  bar,
  byRecent,
  crashNeeds,
  crashWords,
  elapsed,
  filter,
  hints,
  javaShort,
  joinFit,
  lit,
  loaderText,
  pct,
  shortMc,
  stateOf,
  type Hint,
  type InstState,
} from './bits'
import { overlayMenu, palette } from './overlays'

const G = 2

interface Cols {
  name: number
  ver: number
  loader: number
  time: number
  state: number
  when: number
  /** Cells from the first column's start to the last one's end. */
  total: number
  /** Narrow: the state word stands in for the time. */
  merged: boolean
}

function whenText(c: Ctx, inst: Instance, st: InstState): string {
  if (st.kind === 'running') return elapsed(c, c.data.running?.elapsed).short
  if (st.kind === 'crashed') return c.data.crash?.when ?? ''
  if (!inst.lastPlayed) return ''
  return inst.lastPlayed
}

function columns(c: Ctx, all: Instance[], W: number): Cols {
  let merged = false
  const states = all.map((i) => stateOf(c, i).kind)
  const cols = {
    name: Math.min(Math.max(14, ...all.map((i) => width(i.name))), W >= 130 ? 26 : 20),
    ver: Math.max(6, ...all.map((i) => width(shortMc(i.mc)))),
    loader: W >= 124 ? 7 : 0,
    time: W >= 124 ? Math.max(6, ...all.map((i) => width(i.playtime))) : 0,
    state: states.some((k) => k !== 'idle') ? 7 : 0,
    when: Math.max(6, ...all.map((i) => width(whenText(c, i, stateOf(c, i))))),
  }
  const total = () =>
    cols.name +
    [cols.ver, cols.loader, cols.time, cols.state].reduce((a, w) => a + (w ? G + w : 0), 0) +
    G +
    cols.when
  const room = () => W - total() - 6
  const MIN = W < 90 ? 32 : 34
  if (room() < MIN) cols.time = 0
  if (room() < MIN) cols.loader = 0
  if (room() < MIN) cols.ver = 0
  // Narrow: state takes the time's place when there is one (it's the newer news).
  if (room() < MIN && cols.state) {
    cols.when = Math.max(cols.when, cols.state)
    cols.state = 0
    merged = true
  }
  while (room() < MIN && cols.name > 12) cols.name--
  return { ...cols, total: total(), merged }
}

/** The state column: a word, or a download's live bar. */
function drawState(c: Ctx, x: number, y: number, w: number, st: InstState, on = false) {
  const { buf, t } = c
  switch (st.kind) {
    case 'running':
      return buf.text(x, y, 'running', { fg: t.success }, w)
    case 'crashed':
      return buf.text(x, y, 'crashed', { fg: t.error, bold: true }, w)
    case 'trouble':
      return buf.text(x, y, 'crashed', { fg: t.warning }, w)
    case 'queued':
      return buf.text(x, y, 'queued', { fg: t.dim }, w)
    case 'new':
      return buf.text(x, y, 'new', { fg: t.accent }, w)
    case 'download': {
      const p = pct(st.ratio ?? 0).padStart(4)
      bar(c, x, y, w - 4, st.ratio ?? 0, c.t.accent, on ? mix(c.t.selection, c.t.text, 0.16) : c.t.surfaceHi)
      return buf.text(x + w - 4, y, p, { fg: t.accent })
    }
  }
}

export function home(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  header(c, f)
  c.panes(1)
  if (data.running || data.crash) c.bind('l', 'screen:play')

  const all = byRecent(data.instances)
  const filtering = ui.typing || !!ui.query
  const shown = filter(all, ui.query, (i) => i.name)
  const n = shown.length
  const sel = c.sel(n + 1) // the last row is “+ New instance”
  const cur = sel < n ? shown[sel].item : null
  const curSt = cur ? stateOf(c, cur) : null

  const cols = columns(c, all, f.w)
  const X = f.x
  const listW = cols.total
  const px = X + listW + 6
  const pv = rect(px, f.body.y, Math.min(64, right(f.body) - px), f.body.h)

  // ── Label row: `INSTANCES 5`, or the filter field in its place ──
  const ly = f.body.y
  if (filtering) {
    const qx = buf.text(X, ly, '/', { fg: t.accent, bold: true })
    const end = buf.text(qx + 1, ly, ui.query, { fg: t.strong }, listW - 12)
    if (ui.typing) buf.cursor = { x: end, y: ly }
    else if (!ui.query) buf.text(qx + 1, ly, 'filter', { fg: t.faint })
    buf.lineRight(rect(X, ly, listW, 1), [[`${n} of ${all.length}`, { fg: t.muted }]])
    buf.region(rect(X, ly, listW, 1), 'query')
  } else {
    const ex = buf.text(X, ly, 'INSTANCES', { fg: t.muted, bold: true })
    if (all.length) buf.text(ex + 2, ly, String(all.length), { fg: t.faint })
  }

  // ── Rows ──
  const rowsY = ly + 1
  const fit = Math.max(1, bottom(f.body) - rowsY - 2) // leave a gap + the “new” row
  const start = Math.max(0, Math.min(sel - fit + 1, n - fit, sel))
  const first = Math.max(0, Math.min(start, Math.max(0, n - fit)))
  const visible = shown.slice(first, first + fit)
  const band = (y: number, on: boolean, hov: boolean) => {
    if (on) buf.fill(rect(X - 2, y, listW + 4, 1), { bg: t.selection })
    else if (hov) buf.fill(rect(X - 2, y, listW + 4, 1), { bg: t.surface })
  }
  visible.forEach(({ item: inst, at }, j) => {
    const i = first + j
    const y = rowsY + j
    const on = sel === i
    const hov = c.hovered(`sel:${i}`)
    band(y, on, hov)
    const st = stateOf(c, inst)
    let x = X
    lit(c, x, y, inst.name, at, on ? { fg: t.strong, bold: true } : { fg: hov ? t.strong : t.text }, cols.name)
    x += cols.name + G
    if (cols.ver) {
      buf.text(x, y, shortMc(inst.mc), { fg: on ? t.subtle : t.dim }, cols.ver)
      x += cols.ver + G
    }
    if (cols.loader) {
      buf.text(x, y, inst.loader, { fg: t.muted }, cols.loader)
      x += cols.loader + G
    }
    if (cols.time) {
      if (!inst.fresh) buf.lineRight(rect(x, y, cols.time, 1), [[inst.playtime, { fg: t.muted }]])
      x += cols.time + G
    }
    if (cols.state) {
      drawState(c, x, y, cols.state, st, on)
      x += cols.state + G
    }
    if (cols.merged && st.kind !== 'idle') {
      const word = { running: 7, crashed: 7, trouble: 7, queued: 6, new: 3 } as Record<string, number>
      const sw = st.kind === 'download' ? cols.when : Math.min(cols.when, word[st.kind] ?? 7)
      drawState(c, x + cols.when - sw, y, sw, st, on)
      buf.region(rect(X - 2, y, listW + 4, 1), `sel:${i}`)
      return
    }
    const when = whenText(c, inst, st)
    const whenFg = st.kind === 'running' ? t.text : st.kind === 'crashed' ? t.subtle : on ? t.subtle : agoColor(c, inst.lastPlayed)
    buf.lineRight(rect(x, y, cols.when, 1), [[when, { fg: whenFg }]])
    buf.region(rect(X - 2, y, listW + 4, 1), `sel:${i}`)
  })
  const hiddenAbove = first
  const hiddenBelow = n - first - visible.length
  if (hiddenAbove && !filtering) buf.lineRight(rect(X, ly, listW, 1), [[`↑ ${hiddenAbove} more`, { fg: t.faint }]])
  const gapY = rowsY + visible.length
  if (hiddenBelow) buf.text(X, gapY, `↓ ${hiddenBelow} more`, { fg: t.faint })
  if (filtering && !n) buf.text(X, rowsY, `Nothing matches “${ui.query}”`, { fg: t.dim }, listW)
  else if (!all.length) buf.text(X, rowsY, 'None yet', { fg: t.faint })

  // ── + New instance ──
  const ny = (!n ? rowsY + 1 : gapY) + 1
  const onNew = sel === n
  const hovNew = c.hovered(`sel:${n}`)
  band(ny, onNew, hovNew)
  buf.text(X, ny, '+ New instance', onNew ? { fg: t.strong, bold: true } : { fg: hovNew ? t.text : t.dim })
  buf.lineRight(rect(X, ny, listW, 1), [['n', { fg: t.faint }]])
  buf.region(rect(X - 2, ny, listW + 4, 1), `sel:${n}`)

  // ── Preview ──
  if (cur && curSt) sheet(c, pv, cur, curSt)
  else newSheet(c, pv, !all.length)

  // ── What ↵ does, the status line, the footer ──
  c.enter(!cur ? 'wizard' : curSt?.kind === 'download' ? null : 'play')
  const showStatus = !ui.overlay
  if (showStatus) statusLine(c, f, { selected: cur?.name })
  footer(c, f, footerHints(c, cur, curSt, filtering))
  overlays(c, f, cur, rowsY + Math.max(0, sel - first), px)
}

function footerHints(c: Ctx, cur: Instance | null, st: InstState | null, filtering: boolean): Hint[] {
  if (c.ui.typing) return [['↑↓', 'pick'], ['↵', 'done'], ['esc', 'clear']]
  const esc: Hint[] = filtering ? [['esc', 'clear filter']] : []
  if (!cur || !st) return [['↵', 'create'], ...(c.data.instances.length ? ([['/', 'filter']] as Hint[]) : []), ...esc]
  const expired = c.data.account.status === 'expired'
  switch (st.kind) {
    case 'running':
      return [['↵', 'log'], ['/', 'filter'], ['n', 'new'], ['m', 'mods'], ...esc]
    case 'crashed':
      return [['↵', 'try again'], ['l', 'log', 'screen:play'], ['m', 'fix mods'], ['c', 'copy report'], ...esc]
    case 'download':
      return [['/', 'filter'], ['n', 'new'], ['e', 'edit'], ...esc]
    default:
      return [
        ['↵', expired ? 'sign in & play' : 'play'],
        ['o', 'offline'],
        ['/', 'filter'],
        ['n', 'new'],
        ['m', 'mods'],
        ['e', 'edit'],
        ...esc,
      ]
  }
}

// ── The preview sheet ────────────────────────────────────────────────────

type Row = (y: number) => void

interface Section {
  head?: (y: number, hidden: number) => void
  items: Row[]
  /** Items kept when space runs short. */
  min: number
  /** Higher gives way first. */
  drop: number
}

/** Rows `sections` take stacked in full. */
const height = (sections: Section[]) => {
  const live = sections.filter((s) => s.head || s.items.length)
  return live.reduce((a, s) => a + (s.head ? 1 : 0) + s.items.length, 0) + Math.max(0, live.length - 1)
}

/** Stacks sections one blank row apart, trimming then dropping to fit `h`. */
function stack(sections: Section[], y0: number, h: number) {
  const secs = sections
    .filter((s) => s.head || s.items.length)
    .map((s) => ({ ...s, n: s.items.length, gone: false }))
  const rowsOf = (s: (typeof secs)[number]) => (s.head ? 1 : 0) + s.n
  const need = () => {
    const live = secs.filter((s) => !s.gone)
    return live.reduce((a, s) => a + rowsOf(s), 0) + Math.max(0, live.length - 1)
  }
  const order = [...secs].sort((a, b) => b.drop - a.drop)
  for (const s of order) while (need() > h && s.n > s.min) s.n--
  for (const s of order) if (need() > h && s.drop > 0) s.gone = true
  let y = y0
  for (const s of secs) {
    if (s.gone) continue
    if (y >= y0 + h) break
    if (s.head) s.head(y++, s.items.length - s.n)
    s.items.slice(0, s.n).forEach((row) => {
      if (y < y0 + h) row(y++)
    })
    y++
  }
}

function sheet(c: Ctx, r: Rect, inst: Instance, st: InstState) {
  const { buf, t, data } = c
  const expired = data.account.status === 'expired'

  // Title, and the one quiet affordance.
  const aff: Line | null =
    st.kind === 'running'
      ? affordance(c, '↵', 'log')
      : st.kind === 'crashed'
        ? affordance(c, '↵', 'try again')
        : st.kind === 'download'
          ? null
          : affordance(c, '↵', expired ? 'sign in & play' : 'play')
  const aw = aff ? lineWidth(aff) : 0
  if (aff) {
    const ax = buf.lineRight(rect(r.x, r.y, r.w, 1), aff)
    buf.region(rect(ax, r.y, aw, 1), 'key:↵')
  }
  buf.text(r.x, r.y, inst.name, { fg: t.strong, bold: true }, r.w - aw - 2)
  const sub = (mc: string): Line => [
    [mc, { fg: t.dim }],
    ['  ·  ', { fg: t.faint }],
    [loaderText(inst), { fg: t.dim }],
  ]
  const full = sub(`Minecraft ${inst.mc}`)
  buf.line(r.x, r.y + 1, lineWidth(full) <= r.w ? full : sub(inst.mc), {}, r.w)

  const build = (expand: boolean): Section[] => [
    { items: stateRows(c, r, inst, st), min: 1, drop: 0 },
    worlds(c, r, inst),
    modsSection(c, r, inst, st, expand),
    { items: [factsRow(c, r, inst, st)], min: 1, drop: 3 },
  ]
  const wide = build(true)
  stack(height(wide) <= r.h - 3 ? wide : build(false), r.y + 3, r.h - 3)
}

function stateRows(c: Ctx, r: Rect, inst: Instance, st: InstState): Row[] {
  const { buf, t, data } = c
  const line = (l: Line) => (y: number) => buf.line(r.x, y, l, {}, r.w)
  const playedFull: Line = [
    ['Played ', { fg: t.dim }],
    [inst.lastPlayed ?? '', { fg: t.text }],
    ['  ·  ', { fg: t.faint }],
    [inst.playtime, { fg: t.text }],
    [' in total', { fg: t.dim }],
  ]
  const played: Line = !inst.lastPlayed
    ? [['Never played', { fg: t.text }]]
    : lineWidth(playedFull) <= r.w
      ? playedFull
      : playedFull.slice(0, 4)
  switch (st.kind) {
    case 'running': {
      const run = data.running!
      const e = elapsed(c, run.elapsed)
      return [
        line(
          fitParts(
            c,
            [
              { line: [['● ', { fg: t.success }], ['Running ', { fg: t.success, bold: true }], [e.long, { fg: t.text }]], prio: 0 },
              { line: [[run.world, { fg: t.text }]], prio: 1 },
            ],
            r.w,
          ),
        ),
        line(
          fitParts(
            c,
            (
              [
              { line: [[run.memory, { fg: t.dim }]], prio: 3 },
              { line: run.warnings ? [[`${run.warnings} warnings`, { fg: t.warningSoft }]] : [], prio: 2 },
              { line: run.errors ? [[`${run.errors} error${run.errors > 1 ? 's' : ''}`, { fg: t.error }]] : [], prio: 1 },
              ] as { line: Line; prio: number }[]
            ).filter((p) => p.line.length),
            r.w,
          ),
        ),
      ]
    }
    case 'crashed': {
      const crash = data.crash!
      const { sentence, cause } = crashWords(crash.reason, r.w)
      const fix = crashNeeds(crash.reason, inst.mods)
      const rows: Row[] = [
        line(
          fitParts(
            c,
            [
              { line: [['✗ ', { fg: t.error, bold: true }], [`Crashed ${crash.when}`, { fg: t.error, bold: true }]], prio: 0 },
              { line: [[crash.exit, { fg: t.dim }]], prio: 1 },
            ],
            r.w,
          ),
        ),
      ]
      balance(sentence, r.w).forEach((l) => rows.push(line([[l, { fg: t.strong }]])))
      if (cause) rows.push(line([[cutAt(cause, r.w), { fg: t.faint }]]))
      rows.push((y) =>
        hints(c, r.x, y, [
          ['m', fix ? `update ${fix.mod}` : 'fix mods', 'screen:mods'],
          ['l', 'log', 'screen:play'],
        ]),
      )
      return rows
    }
    case 'trouble':
      return [
        line([
          ['▲ ', { fg: t.warning }],
          ['Crashed last time', { fg: t.warning }],
          ['  ·  ', { fg: t.faint }],
          [inst.lastPlayed ?? '', { fg: t.dim }],
          ['  ·  ', { fg: t.faint }],
          ['l', { fg: t.strong, bold: true }],
          [' log', { fg: t.muted }],
        ]),
        line(played),
      ]
    case 'download': {
      const ratio = st.ratio ?? 0
      return [
        (y) => {
          buf.line(r.x, y, [[`${c.spinner()} `, { fg: t.accent }], ['Downloading the game', { fg: t.text }]], {}, r.w - 6)
          buf.lineRight(rect(r.x, y, r.w, 1), [[pct(ratio), { fg: t.accent, bold: true }]])
        },
        (y) => bar(c, r.x, y, r.w, ratio),
        line([[st.detail ?? '', { fg: t.dim }]]),
        line([['Starts as soon as it’s done.', { fg: t.dim }]]),
      ]
    }
    case 'queued':
      const long = `${st.label ?? 'Update'}: queued`
      const short = `${(st.label ?? '').match(/^Updating (\S+)/)?.[1] ?? 'An'} update queued`
      return [line([['○ ', { fg: t.dim }], [width(long) + 2 <= r.w ? long : short, { fg: t.dim }]]), line(played)]
    case 'new':
      return [
        line([['New', { fg: t.accent }], ['  ·  ', { fg: t.faint }], ['never played', { fg: t.text }]]),
        line([['↵ downloads the game, then starts it.', { fg: t.dim }]]),
      ]
    default:
      return [line(played)]
  }
}

function worlds(c: Ctx, r: Rect, inst: Instance): Section {
  const { buf, t } = c
  const ws = inst.worlds
  const showMode = r.w >= 48
  const whenW = Math.max(0, ...ws.map((w) => width(w.lastPlayed)))
  const sizeW = Math.max(0, ...ws.map((w) => width(w.size)))
  return {
    head: (y, hidden) => {
      label(c, r.x, y, 'Worlds', ws.length)
      if (hidden) buf.lineRight(rect(r.x, y, r.w, 1), [[`+${hidden} more`, { fg: t.faint }]])
    },
    items: ws.length
      ? ws.map((w) => (y: number) => {
          const rx = right(r)
          buf.lineRight(rect(r.x, y, r.w, 1), [[w.lastPlayed.padStart(whenW), { fg: t.dim }]])
          const sx = rx - whenW - 3 - sizeW
          buf.text(sx, y, w.size.padStart(sizeW), { fg: t.dim })
          let nameMax = sx - r.x - 2
          if (showMode) {
            const mx = sx - 3 - 8
            buf.text(mx, y, w.mode, { fg: w.mode === 'Hardcore' ? t.subtle : t.muted })
            nameMax = mx - r.x - 2
          }
          buf.text(r.x, y, w.name, { fg: t.text }, nameMax)
        })
      : [(y: number) => buf.text(r.x, y, r.w >= 42 ? 'None yet. They appear here after you play.' : 'None yet.', { fg: t.faint }, r.w)],
    min: 1,
    drop: 1,
  }
}

function modsSection(c: Ctx, r: Rect, inst: Instance, st: InstState, expand: boolean): Section {
  const { buf, t, data } = c
  if (inst.loader === 'Vanilla') {
    if (!inst.packs.length) return { items: [], min: 0, drop: 2 }
    return {
      head: (y) => label(c, r.x, y, 'Resource packs', inst.packs.length),
      items: [(y) => buf.text(r.x, y, joinFit(inst.packs, r.w), { fg: t.text })],
      min: 1,
      drop: 2,
    }
  }
  const mods = inst.mods
  const blame = st.kind === 'crashed' ? crashNeeds(data.crash?.reason, mods) : null
  const task = data.tasks.find((k) => k.instance === inst.name)
  const notes = mods.map((m) => {
    if (blame?.mod === m.name) return { m, rank: 0, note: [[`needs ${blame.needs}`, { fg: t.error, bold: true }]] as Line }
    if (m.update && task?.label.includes(m.name)) return { m, rank: 1, note: [[`↑ ${m.update} queued`, { fg: t.dim }]] as Line }
    if (m.update) return { m, rank: 1, note: [[`↑ ${m.update}`, { fg: t.accent }]] as Line }
    if (!m.enabled) return { m, rank: 2, note: [['off', { fg: t.faint }]] as Line }
    return { m, rank: 9, note: null }
  })
  const attention = notes.filter((x) => x.note).sort((a, b) => a.rank - b.rank)
  const fine = notes.filter((x) => !x.note).map((x) => x.m.name)
  const updates = notes.filter((x) => x.rank === 1).length
  const off = notes.filter((x) => x.rank === 2).length
  const summary: Line = []
  const add = (s: string, fg: string) => {
    if (summary.length) summary.push(['  ·  ', { fg: t.faint }])
    summary.push([s, { fg }])
  }
  if (blame) add('1 to fix', t.error)
  if (updates) add(`${updates} update${updates > 1 ? 's' : ''}`, t.accent)
  if (off) add(`${off} off`, t.dim)
  // One aligned table: what needs you first. Collapsed, the fine ones share a line.
  const listed = expand ? [...attention, ...notes.filter((x) => !x.note)] : attention
  const verW = Math.max(0, ...listed.map((a) => width(a.m.version)))
  const nameW = Math.min(Math.max(0, ...listed.map((a) => width(a.m.name))), r.w - verW - 14)
  const items: Row[] = listed.map(({ m, note }) => (y: number) => {
    const quiet = !note || !m.enabled
    buf.text(r.x, y, m.name, { fg: quiet ? t.dim : t.text }, nameW)
    buf.text(r.x + nameW + 2, y, m.version, { fg: quiet ? t.muted : t.dim }, verW)
    if (note) buf.lineRight(rect(r.x, y, r.w, 1), note)
  })
  if (!expand && fine.length) items.push((y) => buf.text(r.x, y, joinFit(fine, r.w), { fg: t.dim }))
  if (!mods.length)
    items.push((y) =>
      buf.line(r.x, y, [['None yet.  ', { fg: t.faint }], ['m', { fg: t.strong, bold: true }], [' adds some from Modrinth', { fg: t.muted }]], {}, r.w),
    )
  return {
    head: (y) => {
      label(c, r.x, y, 'Mods', mods.length)
      if (summary.length) buf.lineRight(rect(r.x, y, r.w, 1), summary)
    },
    items,
    min: Math.min(items.length, Math.max(1, attention.length)),
    drop: 2,
  }
}

function factsRow(c: Ctx, r: Rect, inst: Instance, st: InstState): Row {
  const { buf, t } = c
  const want = [`${inst.memory} memory`, r.w >= 50 ? inst.java : javaShort(inst.java)]
  if (st.kind === 'running' || st.kind === 'crashed') want.push(`${inst.playtime} played`)
  // Whole facts only: drop from the end rather than clip one.
  const parts: string[] = []
  for (const p of want) if (width([...parts, p].join('  ·  ')) <= r.w) parts.push(p)
  return (y) => buf.text(r.x, y, parts.join('  ·  '), { fg: t.muted }, r.w)
}

/** The preview of “+ New instance”: what ↵ ↵ ↵ will make. */
function newSheet(c: Ctx, r: Rect, firstRun: boolean) {
  const { buf, t, data } = c
  const latest = data.versions.find((v) => v.latest) ?? data.versions[0]
  const aff = affordance(c, '↵', 'create')
  const ax = buf.lineRight(rect(r.x, r.y, r.w, 1), aff)
  buf.region(rect(ax, r.y, lineWidth(aff), 1), 'key:↵')
  buf.text(r.x, r.y, firstRun ? 'Nothing to play yet' : 'New instance', { fg: t.strong, bold: true }, r.w - 10)
  let y = r.y + 1
  const intro = firstRun
    ? 'An instance is one Minecraft version with its own worlds, mods and settings. Make your first one:'
    : 'One Minecraft version with its own worlds, mods and settings.'
  wrap(intro, r.w).forEach((l) => buf.text(r.x, y++, l, { fg: t.dim }))
  y++
  buf.line(r.x, y++, [['↵ ↵ ↵', { fg: t.accent, bold: true }], ['  takes the defaults', { fg: t.dim }]], {}, r.w)
  y++
  const kv: [string, string, string][] = [
    ['Version', latest.id, 'latest release'],
    ['Loader', 'Vanilla', 'or Fabric, for mods'],
    ['Name', latest.id, 'yours to change'],
  ]
  const vw = Math.max(...kv.map(([, v]) => width(v)))
  kv.forEach(([k, v, note]) => {
    buf.text(r.x, y, k, { fg: t.muted })
    buf.text(r.x + 10, y, v, { fg: t.strong })
    const nx = r.x + 10 + vw + 3
    if (right(r) - nx >= width(note)) buf.text(nx, y, note, { fg: t.faint })
    y++
  })
  y++
  if (firstRun && y < bottom(r))
    buf.line(r.x, y, [['Signed in as ', { fg: t.muted }], [data.account.name, { fg: t.text }], [`  ·  ${data.account.kind}`, { fg: t.muted }]], {}, r.w)
}

// ── Overlays, TUI-sized ──────────────────────────────────────────────────

function overlays(c: Ctx, f: Frame, cur: Instance | null, rowY: number, menuX: number) {
  const { ui, t } = c
  if (!ui.overlay) return
  if (ui.overlay === 'help') {
    helpSheet(c, f, 'Keys', [
      ['Move', [['↑↓  j k', 'move'], ['g  G', 'first, last'], ['/', 'filter'], ['esc', 'clear filter']]],
      ['Instance', [['↵', 'play'], ['o', 'play offline'], ['e', 'edit'], ['m', 'mods'], ['x', 'more'], ['d', 'delete']]],
      ['Game', [['l', 'log'], ['s', 'stop'], ['c', 'copy report']]],
      ['Go', [['n', 'new instance'], ['a', 'accounts'], [':', 'commands'], ['q', 'quit']]],
    ])
  } else if (ui.overlay === 'confirm' && cur) {
    const detail = [
      plural(cur.worlds.length, 'world'),
      cur.mods.length ? plural(cur.mods.length, 'mod') : '',
    ]
      .filter(Boolean)
      .join(' and ')
    ask(
      c,
      f,
      [
        ['Delete ', { fg: t.text }],
        [cur.name, { fg: t.strong, bold: true }],
        ['?', { fg: t.text }],
        [detail ? `  Its ${detail} go too.` : '', { fg: t.dim }],
      ],
      [
        ['y', 'delete', 'close'],
        ['esc', 'keep', 'close'],
      ],
    )
  } else if (ui.overlay === 'menu' && cur) {
    overlayMenu(c, f, cur, rowY, menuX)
  } else if (ui.overlay === 'palette') {
    palette(c, f)
  }
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`
