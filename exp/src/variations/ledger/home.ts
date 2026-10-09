// Home: one table of instances, a drawer about the selected row.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { type Line, type Style, rect } from '../../term/buffer'
import { fuzzy } from './fuzzy'
import {
  ageMinutes,
  ageText,
  cmpVersion,
  modsOf,
  playMinutes,
  sessionOf,
  stateOf,
  updatesOf,
  versionFit,
  type State,
} from './data'
import { type Col, type Entry, type Frame, type Hint, bar, band, body, cell, fit, footer, frame, head, ledger, lit, moreTail, pad, padL, prompt, rule } from './ui'
import { overlay } from './overlays'

interface Row {
  inst: Instance
  state: State
  match?: { col: 'name' | 'loader' | 'version'; hits: number[] }
}

const loaderText = (i: Instance, full: boolean) => (i.loader === 'Vanilla' ? 'Vanilla' : full ? `Fabric ${i.loaderVersion}` : 'Fabric')

/** The sorts `s` cycles through (also ←→, or a click on a header). */
const SORTS: { key: string; desc: boolean; cmp: (a: Row, b: Row) => number }[] = [
  { key: 'last', desc: true, cmp: (a, b) => lastOf(a) - lastOf(b) },
  { key: 'name', desc: false, cmp: (a, b) => a.inst.name.localeCompare(b.inst.name) },
  { key: 'version', desc: true, cmp: (a, b) => cmpVersion(b.inst.mc, a.inst.mc) },
  { key: 'played', desc: true, cmp: (a, b) => playMinutes(b.inst.playtime) - playMinutes(a.inst.playtime) },
  { key: 'mods', desc: true, cmp: (a, b) => modsOf(b.inst).length - modsOf(a.inst).length },
]
const lastOf = (r: Row) => (r.state.kind === 'running' ? -1 : ageMinutes(r.inst.lastPlayed))

function match(q: string, inst: Instance): Row['match'] | null {
  if (!q) return undefined
  const name = fuzzy(q, inst.name)
  if (name) return { col: 'name', hits: name.hits }
  const loader = fuzzy(q, loaderText(inst, true))
  if (loader) return { col: 'loader', hits: loader.hits }
  const version = fuzzy(q, inst.mc)
  if (version) return { col: 'version', hits: version.hits }
  return null
}

export function stateLine(c: Ctx, s: State, on = false): Line {
  const { t } = c
  switch (s.kind) {
    case 'running':
      return [[`● running ${s.detail}`, { fg: t.success }]]
    case 'downloading':
      return [...bar(c, 7, s.ratio ?? 0), [padL(`${Math.round((s.ratio ?? 0) * 100)}%`, 5), { fg: on ? t.strong : t.text }]]
    case 'queued':
      return [['queued', { fg: on ? t.subtle : t.muted }]]
    case 'crashed':
      return [['✗ crashed', { fg: t.error }]]
    case 'trouble':
      return [['▲ crashed', { fg: t.warning }]]
    case 'new':
      return [['new', { fg: t.accent }]]
    default:
      return [['ready', { fg: on ? t.dim : t.faint }]]
  }
}

const javaText = (i: Instance) => {
  const m = i.java.match(/Java (\d+)/)
  return m ? `${m[1]}${/Rosetta|Intel/.test(i.java) ? ' x64' : ''}` : i.java
}

function columns(c: Ctx, nameW: number, wide: boolean): Col<Row>[] {
  const { t } = c
  const sec = (on: boolean): Style => ({ fg: on ? t.subtle : t.dim })
  const none: Line = [['–', { fg: t.faint }]]
  return [
    {
      key: 'name',
      label: 'NAME',
      w: nameW,
      min: Math.min(nameW, 16),
      flex: true,
      max: nameW,
      sort: 1,
      cell: (r, w, on) => lit(c, r.inst.name, r.match?.col === 'name' ? r.match.hits : undefined, on ? { fg: t.strong, bold: true } : { fg: t.text }, w),
    },
    { key: 'state', label: 'STATE', w: 15, min: 13, grow: 5, cell: (r, _w, on) => stateLine(c, r.state, on) },
    {
      key: 'version',
      label: 'VERSION',
      w: 15,
      min: 11,
      drop: 2,
      sort: 2,
      cell: (r, w, on) => {
        const v = versionFit(r.inst.mc, w)
        return lit(c, v, r.match?.col === 'version' && v === r.inst.mc ? r.match.hits : undefined, sec(on))
      },
    },
    {
      key: 'loader',
      label: 'LOADER',
      w: 14,
      min: 7,
      drop: 3,
      spacer: true,
      cell: (r, w, on) => lit(c, loaderText(r.inst, w >= 14), r.match?.col === 'loader' ? r.match.hits : undefined, sec(on)),
    },
    ...(wide
      ? ([
          { key: 'java', label: 'JAVA', w: 6, cell: (r: Row, _w: number, on: boolean) => [[javaText(r.inst), sec(on)]] },
          { key: 'memory', label: 'MEMORY', w: 6, right: true, cell: (r: Row, _w: number, on: boolean) => [[r.inst.memory, sec(on)]] },
        ] as Col<Row>[])
      : []),
    {
      key: 'mods',
      label: 'MODS',
      w: 5,
      right: true,
      drop: 4,
      sort: 4,
      cell: (r, w, on) => {
        if (r.inst.loader === 'Vanilla') return none
        const n = String(modsOf(r.inst).length)
        const up = updatesOf(r.inst).length > 0
        return up ? [['↑ ', { fg: t.warning }], [n, sec(on)]] : [[padL(n, w), sec(on)]]
      },
    },
    { key: 'worlds', label: 'WORLDS', w: 6, right: true, drop: 6, cell: (r, _w, on) => (r.inst.worlds.length ? [[String(r.inst.worlds.length), sec(on)]] : none) },
    {
      key: 'played',
      label: 'PLAYED',
      w: 7,
      right: true,
      drop: 5,
      sort: 3,
      cell: (r, _w, on) => (playMinutes(r.inst.playtime) ? [[r.inst.playtime, sec(on)]] : none),
    },
    {
      key: 'last',
      label: 'LAST',
      w: 7,
      right: true,
      drop: 1,
      sort: 0,
      cell: (r, _w, on) =>
        r.state.kind === 'running'
          ? [['now', { fg: on ? t.strong : t.text }]]
          : r.inst.lastPlayed
            ? [[ageText(r.inst.lastPlayed), sec(on)]]
            : [['never', { fg: t.faint }]],
    },
  ]
}

export function home(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  const H = buf.rows
  const all = data.instances
  const sortIdx = c.tabs(SORTS.length)
  c.bind('s', 'tab:+1')
  const sort = SORTS[sortIdx]
  const q = ui.query

  const rows: Row[] = []
  for (const inst of all) {
    const m = match(q, inst)
    if (m === null) continue
    rows.push({ inst, state: stateOf(data, inst), match: m })
  }
  rows.sort((a, b) => sort.cmp(a, b) || a.inst.name.localeCompare(b.inst.name))
  const empty = all.length === 0
  const sel = c.sel(Math.max(1, rows.length))
  const row = rows[sel]

  // Geometry: the drawer hangs under the table, which is sized for every
  // instance (not the filtered few), so typing a filter never moves it.
  const lastRow = f.foot - 2
  const dMin = H >= 36 ? 9 : H >= 28 ? 7 : 5
  const dMax = H >= 36 ? 11 : H >= 28 ? 8 : 7
  const fitRows = lastRow - dMin - 1 - f.body
  const tableRows = Math.max(1, Math.min(empty ? 1 : all.length, fitRows))
  const ruleY = f.body + tableRows + 1
  const drawer = rect(f.x, ruleY + 1, f.w, Math.min(lastRow, ruleY + dMax) - ruleY)

  // ── Top line ──
  let x = buf.text(f.x, f.top, 'mctui', { fg: t.accent, bold: true })
  if (q && !ui.typing) {
    x = buf.text(x + 3, f.top, '/', { fg: t.accent, bold: true })
    x = buf.text(x, f.top, q, { fg: t.text })
    buf.text(x + 2, f.top, `${rows.length} of ${all.length}`, { fg: t.muted })
  } else if (!empty) buf.text(x + 3, f.top, `${all.length} instances`, { fg: t.muted })
  topRight(c, f)

  // ── Table ──
  // NAME is as wide as the longest name (16–30), like any table.
  const nameW = Math.max(16, Math.min(30, Math.max(0, ...all.map((i) => i.name.length + 1))))
  const cols = columns(c, nameW, f.w >= 124)
  const placed = fit(cols, f.x, f.w, 2, 5)
  head(c, f.head, placed, sort.key, sort.desc)
  let tail: Line = []
  if (empty) {
    band(c, f, f.body, true, false)
    const ghost: Record<string, Line> = {
      name: [['New instance', { fg: t.strong, bold: true }]],
      state: [['↵ create', { fg: t.accent }]],
      version: [['26.3', { fg: t.subtle }]],
      loader: [['Fabric', { fg: t.subtle }]],
    }
    for (const p of placed) if (ghost[p.col.key]) cell(c, p, f.body, ghost[p.col.key])
    buf.region(rect(f.x - 1, f.body, f.w + 2, 1), 'sel:0')
  } else if (!rows.length) {
    const ex = buf.text(f.x, f.body, `Nothing matches “${q}”.`, { fg: t.dim })
    buf.text(ex + 2, f.body, 'esc clears the filter', { fg: t.faint })
  } else {
    const { above, below } = body(c, f, f.body, tableRows, placed, rows, sel)
    tail = moreTail(c, above, below)
  }

  // ── Drawer ──
  const d = ui.overlay ? null : row ? drawerFor(c, row, drawer.h, f.w >= 124) : empty ? welcome(c) : null
  if (d) {
    rule(c, f, ruleY, d.title, tail)
    ledger(c, drawer, d.entries)
  } else if (!ui.overlay) rule(c, f, ruleY, [], tail)

  // ── Footer ──
  const hints = hintsFor(c, row, empty)
  if (ui.typing) prompt(c, f, '/', q, 'filter by name, loader or version', `${rows.length} of ${all.length}`, [['↵', 'keep'], ['esc', 'clear']])
  else footer(c, f, hints)

  c.enter(empty ? 'wizard' : !row ? null : row.state.kind === 'downloading' ? null : 'play')
  if (ui.overlay) overlay(c, f, ruleY, drawer, 'home', row?.inst ?? null, row?.state ?? null)
}

function topRight(c: Ctx, f: Frame) {
  const { buf, t, data } = c
  const a = data.account
  const r = rect(f.x, f.top, f.w, 1)
  const line: Line =
    a.status === 'expired'
      ? [['▲ sign-in expired', { fg: t.warning }], ['  ', {}], [a.name, { fg: t.text }]]
      : [[a.name, { fg: c.hovered('screen:accounts') ? t.text : t.muted }]]
  const start = buf.lineRight(r, line)
  buf.region(rect(start, f.top, r.x + r.w - start, 1), 'screen:accounts')
}

// ── Drawer contents ───────────────────────────────────────────────────────

interface Drawer {
  title: Line
  entries: Entry[]
}

const key = (c: Ctx, k: string): [string, Style] => [k, { fg: c.t.text, bold: true }]
const say = (c: Ctx, s: string): [string, Style] => [s, { fg: c.t.muted }]

function drawerFor(c: Ctx, row: Row, h: number, wide: boolean): Drawer {
  const { t, data } = c
  const { inst, state } = row
  const name: [string, Style] = [inst.name, { fg: t.strong, bold: true }]
  const out: Entry[] = []
  let title: Line = [name]

  if (data.account.status === 'expired' && state.kind !== 'running')
    out.push({
      label: 'account',
      labelFg: t.warning,
      line: [['online play needs a fresh sign-in', { fg: t.text }], ['   ', {}], key(c, 'a'), say(c, ' renews it  ·  '), key(c, 'o'), say(c, ' plays offline')],
    })

  if (state.kind === 'running') {
    const s = sessionOf(data)
    title = [name, [`  running ${s.elapsed}`, { fg: t.success }]]
    out.push({
      label: 'session',
      line: [['● ', { fg: t.success }], [`${s.elapsed} since ${s.since}`, { fg: t.text }], [`  ·  ${s.world}  ·  ${s.memory}`, { fg: t.dim }]],
    })
    out.push({
      label: 'problems',
      line: [
        [`${s.errors} error${s.errors === 1 ? '' : 's'}`, { fg: s.errors ? t.error : t.dim }],
        ['  ·  ', { fg: t.faint }],
        [`${s.warnings} warnings`, { fg: s.warnings ? t.warning : t.dim }],
        ['   ', {}],
        key(c, '↵'),
        say(c, ' opens the log'),
      ],
    })
    // The newest lines that fit, oldest first, like `tail`.
    const room = Math.max(1, h - out.length)
    data.log.slice(-room).forEach((l, i) => {
      const fg = l.level === 'ERROR' ? t.error : l.level === 'WARN' ? t.warning : t.dim
      out.push({ label: i ? undefined : 'log', line: [[l.time, { fg: t.faint }], ['  ', {}], [l.text, { fg }]] })
    })
    return { title, entries: out }
  }

  if (state.kind === 'crashed' && data.crash) {
    const [tech, human] = data.crash.reason.split(' — ')
    title = [name, [`  crashed ${data.crash.when}`, { fg: t.error }]]
    out.push({ label: 'crash', labelFg: t.error, line: [[human ?? tech, { fg: t.strong }]] })
    out.push({ line: [[`${tech}  ·  ${data.crash.exit}`, { fg: t.faint }]] })
    const need = modsOf(inst).find((m) => m.name === 'Fabric API')
    out.push({
      label: 'fix',
      line: [key(c, 'u'), say(c, ` updates Fabric API to ${need?.update ?? '0.112'}, then `), key(c, '↵'), say(c, ' tries again  ·  '), key(c, 'm'), say(c, ' opens mods')],
    })
    out.push({ label: 'report', line: [key(c, 'l'), say(c, ' shows the log  ·  '), key(c, 'c'), say(c, ' copies the crash report')] })
  }

  if (state.kind === 'downloading') {
    const task = data.tasks.find((k) => k.instance === inst.name)!
    const pct = Math.round(task.ratio * 100)
    title = [name, [`  downloading ${pct}%`, { fg: t.accent }]]
    out.push({ label: 'download', line: [...bar(c, 24, task.ratio), [`  ${pct}%`, { fg: t.text }], [`   ${task.detail}`, { fg: t.dim }]] })
    out.push({ line: [[`${task.label.replace('Downloading ', '')}; the game starts when it’s done  ·  `, { fg: t.muted }], key(c, 'x'), say(c, ' cancels')] })
  }

  if (state.kind === 'queued') {
    const task = data.tasks.find((k) => k.instance === inst.name)!
    out.push({ label: 'queued', line: [[task.label, { fg: t.text }], ['  after the download in progress', { fg: t.muted }]] })
  }

  if (state.kind === 'new') {
    title = [name, ['  new', { fg: t.accent }]]
    out.push({ label: 'first run', line: [key(c, '↵'), say(c, ` downloads Minecraft ${inst.mc} (about 280 MB) and starts it`)] })
  }

  if (state.kind === 'trouble')
    out.push({ label: 'last run', labelFg: t.warning, line: [['▲ crashed on launch, exit code 1', { fg: t.warning }], ['  ', {}], key(c, 'l'), say(c, ' shows the log')] })

  const ups = updatesOf(inst)
  if (ups.length && state.kind !== 'queued')
    out.push({
      label: 'updates',
      line: [
        ['↑ ', { fg: t.warning }],
        [
          ups.length > 2 ? `${ups.length} mods: ${ups.map((m) => m.name).join(', ')}` : ups.map((m) => `${m.name} ${m.version} → ${m.update}`).join('  ·  '),
          { fg: t.text },
        ],
        ['   ', {}],
        key(c, 'u'),
        say(c, ups.length > 1 ? ' updates all' : ' updates it'),
      ],
    })

  // What's in it.
  if (!inst.worlds.length) out.push({ label: 'worlds', line: [['none yet, they show up after you play', { fg: t.faint }]] })
  inst.worlds.forEach((wd, i) =>
    out.push({
      label: i ? undefined : 'worlds',
      line: [
        [pad(wd.name, 16), { fg: t.text }],
        [pad(wd.mode.toLowerCase(), 10), { fg: t.dim }],
        [padL(wd.size, 7), { fg: t.dim }],
        [padL(ageText(wd.lastPlayed), 9), { fg: t.dim }],
      ],
    }),
  )
  const mods = modsOf(inst)
  if (inst.loader !== 'Vanilla') {
    const off = mods.filter((m) => !m.enabled).length
    out.push({
      label: 'mods',
      line: [
        [`${mods.length} installed`, { fg: t.text }],
        [off ? `  ·  ${off} turned off` : '', { fg: t.dim }],
        [`  ·  Fabric ${inst.loaderVersion}`, { fg: t.dim }],
        ['   ', {}],
        key(c, 'm'),
        say(c, ' manages them'),
      ],
    })
  }
  if (inst.packs.length) out.push({ label: 'packs', line: [[inst.packs.join(', '), { fg: t.text }]] })
  if (!wide) out.push({ label: 'java', line: [[inst.java, { fg: t.dim }], [`  ·  ${inst.memory} memory`, { fg: t.dim }]] })
  return { title, entries: out }
}

function welcome(c: Ctx): Drawer {
  const { t, data } = c
  const v = data.versions.find((x) => x.latest) ?? data.versions[0]
  return {
    title: [['New instance', { fg: t.strong, bold: true }], ['  what ↵ makes', { fg: t.muted }]],
    entries: [
      { label: 'version', line: [[`Minecraft ${v.id}`, { fg: t.text }], [`  latest release, ${v.date}`, { fg: t.dim }]] },
      { label: 'loader', line: [['Fabric 0.17.2', { fg: t.text }], ['  so mods from Modrinth just work; vanilla is one ↓ away', { fg: t.dim }]] },
      { label: 'name', line: [[`${v.id} Fabric`, { fg: t.text }], ['  ↵ ↵ ↵ takes all three; change any of them on the way', { fg: t.dim }]] },
      { label: 'account', line: [[data.account.name, { fg: t.text }], [`  ${data.account.kind}, signed in`, { fg: t.dim }]] },
    ],
  }
}

function hintsFor(c: Ctx, row: Row | undefined, empty: boolean): Hint[] {
  const { data } = c
  if (empty) return [['↵', 'new instance'], ['a', 'accounts'], ['?', 'help'], ['q', 'quit']]
  if (!row) return [['esc', 'clear filter'], ['/', 'edit filter'], ['n', 'new'], ['?', 'help']]
  const expired: Hint[] = data.account.status === 'expired' ? [['a', 'sign in']] : []
  const tail: Hint[] = [['/', 'filter'], ['s', 'sort'], ['n', 'new'], ['?', 'help']]
  switch (row.state.kind) {
    case 'running':
      return [['↵', 'log'], ['x', 'stop'], ...tail]
    case 'crashed':
      return [['↵', 'try again'], ['u', 'fix'], ['l', 'log'], ['c', 'copy report'], ['m', 'mods'], ['?', 'help']]
    case 'downloading':
      return [['x', 'cancel'], ...tail]
  }
  const ups = updatesOf(row.inst).length > 0
  return [
    ...expired,
    ['↵', 'play'],
    ['o', 'offline'],
    ...(row.inst.loader === 'Fabric' ? ([['m', 'mods']] as Hint[]) : []),
    ...(ups && !expired.length ? ([['u', 'update']] as Hint[]) : []),
    ...tail,
  ]
}
