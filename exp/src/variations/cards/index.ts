// Cards: home is a dashboard of instance tiles in a responsive grid; mods,
// versions and loaders are cards too; the running game is one big card.
// Edges are whitespace plus a horizontal rule under each title; selection
// tints only the title row and turns its rule into the accent.

import { defineVariation, type Ctx } from '../../kit'
import { type Line, type Rect, cols, rect, right, bottom, wrap, width, lineWidth, inset, centered } from '../../term/buffer'
import type { Instance, ModHit, World } from '../../data/mock'
import * as w from '../../term/widgets'
import { GAP, type Look, type Act, look, columns, place, fitsAll, more, head, matched, actionBar, filter, sheet, count } from './parts'

// ── Cards ────────────────────────────────────────────────────────────────

/** Height of an instance card: title, rule, version, played, facts, note. */
const CARD_H = 6
const CARD_MIN = 27

function statusGlyph(c: Ctx, inst: Instance): Line {
  const { t } = c
  if (inst.trouble) return [['●', { fg: t.warning }]]
  if (inst.fresh) return [['○', { fg: t.dim }]]
  return [['●', { fg: t.success }]]
}

/** The one thing worth knowing about an instance, if any: its quiet last line. */
function note(c: Ctx, inst: Instance): Line | null {
  const { t } = c
  if (inst.trouble === 'crashed') return [['▲ Crashed last time', { fg: t.warning }]]
  if (inst.trouble) return [["▲ Couldn't start last time", { fg: t.warning }]]
  const upd = inst.mods.find((m) => m.update)
  if (upd) return [['↑ ', { fg: t.secondary }], [`${upd.name} ${upd.update}`, { fg: t.subtle }], [' is out', { fg: t.dim }]]
  const off = inst.mods.filter((m) => !m.enabled)
  if (off.length === 1) return [[`${off[0].name} is turned off`, { fg: t.muted }]]
  if (off.length) return [[`${off.length} mods turned off`, { fg: t.muted }]]
  if (inst.fresh) return [['Downloads on first play', { fg: t.muted }]]
  return null
}

function facts(c: Ctx, inst: Instance, lift: boolean): Line {
  const { t } = c
  const parts: [number, string][] = [
    [inst.worlds.length, 'world'],
    [inst.mods.length, 'mod'],
    [inst.packs.length, 'pack'],
  ]
  const shown = parts.filter(([n]) => n > 0)
  if (!shown.length) return [['No worlds yet', { fg: t.faint }]]
  const out: Line = []
  shown.forEach(([n, label], i) => {
    if (i) out.push([' · ', { fg: t.faint }])
    out.push([String(n), { fg: lift ? t.strong : t.text }], [count(n, label).slice(String(n).length), { fg: t.dim }])
  })
  return out
}

const loaderName = (inst: Instance) => (inst.loader === 'Vanilla' ? 'Vanilla' : `${inst.loader} ${inst.loaderVersion ?? ''}`.trim())

function instanceCard(c: Ctx, r: Rect, inst: Instance, lk: Look, query = '') {
  const { buf, t } = c
  const body = head(c, r, matched(c, inst.name, query), statusGlyph(c, inst), lk)
  const lift = lk.on
  const dot: Line[number] = [' · ', { fg: t.faint }]
  buf.line(body.x, body.y, [[inst.mc, { fg: lift ? t.text : t.subtle }], dot, [loaderName(inst), { fg: t.dim }]], {}, body.w)
  const played: Line = inst.lastPlayed
    ? [[inst.lastPlayed, { fg: lift ? t.text : t.subtle }], dot, [inst.playtime, { fg: t.dim }], [' played', { fg: t.muted }]]
    : [['New', { fg: t.secondary }], [' · never played', { fg: t.dim }]]
  buf.line(body.x, body.y + 1, played, {}, body.w)
  buf.line(body.x, body.y + 2, facts(c, inst, lift), {}, body.w)
  const n = note(c, inst)
  if (n && body.h > 3) buf.line(body.x, body.y + 3, n, {}, body.w)
}

function newCard(c: Ctx, r: Rect, lk: Look) {
  const { buf, t } = c
  const lit = lk.on || lk.hover
  const body = head(c, r, [['+ ', { fg: lit ? t.accent : t.muted }], ['New instance', { fg: lit ? t.strong : t.dim, bold: lk.on }]], null, lk, '╌')
  buf.text(body.x, body.y, 'Pick a version and a', { fg: t.faint }, body.w)
  buf.text(body.x, body.y + 1, 'loader, then name it.', { fg: t.faint }, body.w)
}

// ── Home ─────────────────────────────────────────────────────────────────

/** Rough hours since a `2h ago` / `yesterday` / `3d ago` label, for sorting. */
function ago(s: string | null): number {
  if (!s) return Infinity
  if (s === 'yesterday') return 24
  const m = /^(\d+)([hdw]) ago$/.exec(s)
  if (!m) return 1e6
  return Number(m[1]) * { h: 1, d: 24, w: 168 }[m[2] as 'h' | 'd' | 'w']
}

interface Recent {
  world: World
  inst: Instance
}

const WORLD_H = 3

function worldCard(c: Ctx, r: Rect, { world, inst }: Recent, lk: Look) {
  const { buf, t } = c
  const body = head(c, r, [world.name], [[world.lastPlayed, { fg: lk.on ? t.text : t.dim }]], lk)
  const line: Line = [[inst.name, { fg: lk.on ? t.text : t.subtle }]]
  if (world.mode !== 'Survival') line.push([' · ', { fg: t.faint }], [world.mode, { fg: t.muted }])
  buf.line(body.x, body.y, line, {}, body.w)
}

type Tile = { kind: 'inst'; inst: Instance } | { kind: 'new' } | { kind: 'world'; recent: Recent }

function home(c: Ctx) {
  const { buf, data, ui } = c
  const p = w.page(c)
  w.header(c, p.header)
  const focus = c.panes(2)
  const all = data.instances
  const q = ui.query.trim().toLowerCase()
  const insts = all.filter((i) => i.name.toLowerCase().includes(q))
  const grid: Tile[] = [...insts.map((inst): Tile => ({ kind: 'inst', inst })), { kind: 'new' }]

  // tab opens the details drawer, which pushes the grid narrower.
  const drawerW = focus === 1 ? Math.max(30, Math.min(42, Math.floor(p.body.w * 0.3))) : 0
  const [pane, , drawerPane] = drawerW ? cols(p.body, ['1fr', GAP, drawerW]) : [p.body]
  const area = rect(pane.x, pane.y + 2, pane.w, pane.h - 2)

  // With room to spare, a row of recent worlds under the grid: one click back in.
  const ncols = columns(area, CARD_MIN).length
  const gridRows = Math.ceil(grid.length / ncols)
  const gridH = gridRows * CARD_H + (gridRows - 1) * 2
  const stripY = area.y + gridH + 2
  const recents: Recent[] =
    q || stripY + 2 + WORLD_H > bottom(area)
      ? []
      : all
          .flatMap((inst) => inst.worlds.map((world) => ({ world, inst })))
          .sort((a, b) => ago(a.world.lastPlayed) - ago(b.world.lastPlayed))
          .slice(0, ncols)
  const tiles: Tile[] = [...grid, ...recents.map((recent): Tile => ({ kind: 'world', recent }))]
  const sel = c.sel(tiles.length, { grid: ncols })

  const sx = w.section(c, pane.x + 1, pane.y, 'Instances', q ? `${insts.length} of ${all.length}` : all.length, focus === 0)
  filter(c, rect(sx + 4, pane.y, right(pane) - sx - 4, 1), { placeholder: 'filter', maxW: 30 })

  let anchor: Rect | undefined
  const gridArea = recents.length ? rect(area.x, area.y, area.w, gridH) : area
  const L = place(gridArea, grid, Math.min(sel, grid.length - 1), CARD_MIN, CARD_H)
  for (const { item, i, r } of L.placed) {
    const lk = look(c, i, sel, focus === 0)
    if (item.kind === 'inst') instanceCard(c, r, item.inst, lk, q)
    else newCard(c, r, lk)
    buf.region(r, `sel:${i}`)
    if (i === sel) anchor = r
  }
  if (L.moreY < bottom(gridArea)) more(c, area.x + 1, L.moreY, L.above, L.below)

  if (recents.length) {
    w.section(c, pane.x + 1, stripY, 'Recent worlds', undefined, false)
    columns(area, CARD_MIN).forEach((col, j) => {
      const recent = recents[j]
      if (!recent) return
      const i = grid.length + j
      const r = rect(col.x, stripY + 2, col.w, WORLD_H)
      worldCard(c, r, recent, look(c, i, sel, focus === 0))
      buf.region(r, `sel:${i}`)
      if (i === sel) anchor = r
    })
  }

  const tile = tiles[sel]
  const cur = tile.kind === 'inst' ? tile.inst : tile.kind === 'world' ? tile.recent.inst : undefined
  if (drawerPane) drawer(c, drawerPane, cur)

  const trailing: Act[] = [['Help', '?']]
  const details: Act = ['Details', 'tab']
  if (tile.kind === 'inst') {
    const acts: Act[] = [['Play offline', 'o']]
    if (tile.inst.trouble) acts.push(['Crash log', 'l'])
    acts.push(['Mods', 'm'], ['Edit', 'e'], details)
    actionBar(c, p.footer, ['▶ Play', '↵'], acts, trailing)
  } else if (tile.kind === 'world') {
    const acts: Act[] = [['Play offline', 'o'], ['Open folder', 'f'], details]
    actionBar(c, p.footer, [`▶ Play ${tile.recent.world.name}`, '↵'], acts, trailing)
  } else {
    c.enter('wizard')
    actionBar(c, p.footer, ['+ Create', '↵', 'screen:wizard'], [details], trailing)
  }
  overlays(c, cur, anchor, tile.kind === 'world' ? tile.recent.world.name : undefined)
}

/** The details drawer: what a card has no room for. */
function drawer(c: Ctx, r: Rect, inst: Instance | undefined) {
  const { buf, t } = c
  const x = r.x
  const dw = r.w
  let y = r.y
  w.section(c, x, y, 'Details', undefined, true)
  const hov = c.hovered('focus:0')
  const cx = buf.lineRight(rect(x, y, dw, 1), [['close ', { fg: hov ? t.text : t.muted }], ['tab', { fg: t.accent, bold: true }]])
  buf.region(rect(cx, y, right(r) - cx, 1), 'focus:0')
  y += 2
  const room = () => y < bottom(r)
  if (!inst) {
    const steps = ['Pick a Minecraft version', 'Choose vanilla or Fabric', 'Name it — then play']
    steps.forEach((s, i) => {
      buf.line(x, y + i * 2, [[`${i + 1}  `, { fg: t.accent, bold: true }], [s, { fg: t.text }]], {}, dw)
    })
    return
  }
  const block = (label: string, n: number, rowsIn: [Line, Line][], empty: string) => {
    if (!room()) return
    w.section(c, x, y, label, n || undefined)
    y += 1
    if (!rowsIn.length)
      for (const l of wrap(empty, dw)) {
        if (!room()) break
        buf.text(x, y, l, { fg: t.faint }, dw)
        y += 1
      }
    rowsIn.forEach(([l, rt], i) => {
      if (!room()) return
      if (y === bottom(r) - 1 && i < rowsIn.length - 1) {
        buf.text(x, y, `↓ ${rowsIn.length - i} more`, { fg: t.faint })
        y += 1
        return
      }
      const rx = buf.lineRight(rect(x, y, dw, 1), rt)
      buf.line(x, y, l, {}, rx - x - 1)
      y += 1
    })
    y += 1
  }
  block(
    'Worlds',
    inst.worlds.length,
    inst.worlds.map((wd) => [
      [[wd.name, { fg: t.text }], ...(wd.mode === 'Survival' ? [] : ([[`  ${wd.mode}`, { fg: t.muted }]] as Line))],
      [[wd.lastPlayed, { fg: t.dim }]],
    ]),
    'None yet — they show up after you play.',
  )
  if (inst.loader !== 'Vanilla')
    block(
      'Mods',
      inst.mods.length,
      inst.mods.map((m) => [
        [[m.name, { fg: m.enabled ? t.text : t.faint }]],
        m.update
          ? [[m.version, { fg: t.dim }], [' → ', { fg: t.secondary }], [m.update, { fg: t.text }]]
          : m.enabled
            ? [[m.version, { fg: t.dim }]]
            : [['off', { fg: t.faint }]],
      ]),
      'None yet — press m to add some.',
    )
  block('Resource packs', inst.packs.length, inst.packs.map((pk) => [[[pk, { fg: t.text }]], []]), 'None')
  if (room()) w.kv(c, rect(x, y, dw, 1), 'Memory', [[inst.memory, { fg: t.text }]], 8)
  y += 1
  if (room()) w.kv(c, rect(x, y, dw, 1), 'Java', [[inst.java, { fg: t.text }]], 8)
}

// ── Mods ─────────────────────────────────────────────────────────────────

const MOD_H = 5
const MOD_MIN = 34

function modCard(c: Ctx, r: Rect, hit: ModHit, lk: Look, query: string, inst: Instance) {
  const { buf, t } = c
  const dl: Line = [['↓ ', { fg: t.faint }], [hit.downloads, { fg: lk.on ? t.text : t.dim }]]
  const body = head(c, r, matched(c, hit.name, query), dl, lk)
  const mine = inst.mods.find((m) => m.name === hit.name)
  const state: Line | null = !hit.installed
    ? null
    : mine?.update
      ? [['↑ ', { fg: t.secondary }], [mine.update, { fg: t.subtle }]]
      : [['✓ Installed', { fg: t.success }]]
  const row = rect(body.x, body.y, body.w, 1)
  const sx = state ? buf.lineRight(row, state) : right(row) + 1
  const meta: Line = [[hit.category, { fg: t.subtle }], [' · by ', { fg: t.faint }], [hit.author, { fg: t.dim }]]
  buf.line(body.x, body.y, lineWidth(meta) <= sx - body.x - 2 ? meta : meta.slice(0, 1), {}, sx - body.x - 1)
  const room = Math.max(0, body.h - 1)
  const lines = wrap(hit.description, body.w - 1)
  lines.slice(0, room).forEach((l, i) => {
    const s = i === room - 1 && lines.length > room ? `${l.replace(/[\s.,;:]+$/, '')}…` : l
    buf.text(body.x, body.y + 1 + i, s, { fg: lk.on ? t.text : t.dim }, body.w)
  })
}

function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  const inst = data.instances[0]
  w.header(c, p.header, [inst.name, 'Mods'])
  const focus = c.panes(2) // 0 discover, 1 installed
  const b = p.body
  const labelX = b.x + 1
  const colX = b.x + 17

  // Installed: one row of chips.
  w.section(c, labelX, b.y, 'Installed', inst.mods.length, focus === 1)
  const isel = focus === 1 ? c.sel(inst.mods.length, { across: true }) : -1
  let x = colX - 1
  let shownChips = 0
  for (const [i, m] of inst.mods.entries()) {
    const on = i === isel
    const line: Line = [[m.name, { fg: on ? t.strong : t.text, bold: on }], [` ${m.version}`, { fg: on ? t.subtle : t.muted }]]
    if (m.update) line.push([' ↑', { fg: t.secondary, bold: true }])
    const need = lineWidth(line) + 2
    const left = inst.mods.length - i - 1
    if (x + need + (left ? 5 : 0) > right(b)) break
    const chip = w.chip(c, x, b.y, line, on ? 'selected' : 'plain', focus === 1 ? `sel:${i}` : 'focus:1')
    x = right(chip) + 1
    shownChips++
  }
  if (shownChips < inst.mods.length) buf.text(x + 1, b.y, `+${inst.mods.length - shownChips}`, { fg: t.muted })

  // Discover: the search row, then result cards.
  w.section(c, labelX, b.y + 2, 'Discover', undefined, focus === 0)
  const q = ui.query.trim().toLowerCase()
  const hits = data.modHits.filter((h) => h.name.toLowerCase().includes(q) || h.category.toLowerCase().includes(q))
  filter(c, rect(colX - 1, b.y + 2, right(b) - colX + 1, 1), {
    placeholder: `Search Modrinth for ${inst.mc} · ${inst.loader}`,
    count: `${hits.length} of ${data.modTotal}`,
    always: true,
  })
  const area = rect(b.x, b.y + 4, b.w, b.h - 4)
  const hsel = c.sel(Math.max(1, hits.length), { pane: 0, grid: columns(area, MOD_MIN, 4).length })
  // Two lines of description when everything fits, else one (and scroll).
  const modH = fitsAll(area, hits.length, MOD_MIN, MOD_H, 4) ? MOD_H : MOD_H - 1
  const L = place(area, hits, Math.max(0, hsel), MOD_MIN, modH, 4)
  for (const { item, i, r } of L.placed) {
    modCard(c, r, item, look(c, i, hsel, focus === 0), q, inst)
    buf.region(r, focus === 0 ? `sel:${i}` : 'focus:0')
  }
  if (!hits.length) buf.text(area.x + 1, area.y, `Nothing on Modrinth matches “${ui.query}”.`, { fg: t.faint })
  if (L.moreY < bottom(area)) more(c, area.x + 1, L.moreY, L.above, L.below)

  const trailing: Act[] = [
    ['Back', 'esc'],
    ['Help', '?'],
  ]
  if (focus === 1) {
    const m = inst.mods[isel]
    const acts: Act[] = [[m.enabled ? 'Turn off' : 'Turn on', 'space'], ['Remove', 'd'], ['Discover', 'tab']]
    actionBar(c, p.footer, m.update ? [`↑ Update to ${m.update}`, '↵'] : null, acts, trailing)
  } else {
    const hit = hits[hsel]
    const mine = hit && inst.mods.find((m) => m.name === hit.name)
    if (!hit) actionBar(c, p.footer, null, [['Clear search', 'esc']], [['Help', '?']])
    else if (hit.installed)
      actionBar(c, p.footer, mine?.update ? [`↑ Update to ${mine.update}`, '↵'] : null, [['Remove', 'd'], ['Modrinth page', 'o'], ['Installed', 'tab']], trailing)
    else actionBar(c, p.footer, ['Install', '↵'], [['Modrinth page', 'o'], ['Versions', 'v'], ['Installed', 'tab']], trailing)
  }
  overlays(c, c.data.instances[0])
}

// ── Play ─────────────────────────────────────────────────────────────────

const SPARK = '▁▂▃▄▅▆▇'

/** A made-up memory trace that drifts while the lab animates. */
function spark(n: number, t0: number): string {
  let s = ''
  for (let i = 0; i < n; i++) {
    const k = i + t0
    const v = 0.5 + 0.22 * Math.sin(k * 0.7) + 0.14 * Math.sin(k * 1.9 + 1) + i / (n * 5)
    s += SPARK[Math.max(0, Math.min(SPARK.length - 1, Math.round(v * (SPARK.length - 1))))]
  }
  return s
}

function play(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  const inst = data.instances[0]
  w.header(c, p.header, [inst.name, 'Playing'])
  const b = p.body

  // Now playing: the instance's card, grown to the full width.
  const run: Line = [[`${c.spinner()} Running`, { fg: t.success }], ['  ·  18m', { fg: t.subtle }]]
  const body = head(c, rect(b.x, b.y, b.w, 7), [inst.name], run, { on: true, focused: true, hover: false })
  const dot: Line[number] = ['  ·  ', { fg: t.faint }]
  buf.line(
    body.x,
    body.y,
    [[`Minecraft ${inst.mc}`, { fg: t.text }], dot, [loaderName(inst), { fg: t.dim }], dot, ['as ', { fg: t.dim }], [data.account.name, { fg: t.text }]],
    {},
    body.w,
  )
  const world = inst.worlds[0]
  const tiles: [Line, string][] = [
    [[['18m', { fg: t.strong, bold: true }]], 'this session'],
    [[[world.name, { fg: t.strong, bold: true }]], `${world.mode.toLowerCase()} world`],
    [[['41', { fg: t.strong, bold: true }]], 'mods loaded'],
    [[['2.1 GB', { fg: t.strong, bold: true }], ['  ', {}], [spark(Math.max(0, Math.min(12, Math.floor(body.w / 4) - 10)), Math.floor(ui.tick / 8)), { fg: t.accent }]], `of ${inst.memory} memory`],
  ]
  const tileW = Math.floor(body.w / tiles.length)
  tiles.forEach(([v, label], i) => {
    const tx = body.x + i * tileW
    buf.line(tx, body.y + 2, v, {}, tileW - 2)
    buf.text(tx, body.y + 3, label, { fg: t.muted }, tileW - 2)
  })

  // The log under it.
  const ly = body.y + 5
  const tab = c.tabs(3)
  const warns = data.log.filter((l) => l.level === 'WARN').length
  const errs = data.log.filter((l) => l.level === 'ERROR').length
  w.section(c, b.x + 1, ly, 'Game log', data.log.length, true)
  const tabItems: [string, (string | number)?][] = [['All'], ['Warnings', warns], ['Errors', errs]]
  const tabsW = tabItems.reduce((s, [l, n]) => s + width(l) + (n === undefined ? 0 : String(n).length + 1) + 3, -1)
  w.tabs(c, rect(right(b) - tabsW, ly, tabsW, 1), tabItems, tab, true)
  const logArea = rect(b.x + 1, ly + 2, b.w - 2, bottom(b) - ly - 2)
  const lines = data.log.filter((l) => tab === 0 || l.level === (tab === 1 ? 'WARN' : 'ERROR'))
  const shown = lines.slice(-logArea.h)
  const wide = logArea.w >= 110
  shown.forEach((l, i) => {
    const y = logArea.y + i
    const fg = l.level === 'ERROR' ? t.error : l.level === 'WARN' ? t.warning : t.subtle
    let x = buf.text(logArea.x, y, l.time, { fg: t.faint })
    buf.text(x + 1, y, l.level === 'ERROR' ? '✗' : l.level === 'WARN' ? '▲' : ' ', { fg })
    x += 3
    if (wide) {
      buf.text(x, y, l.thread, { fg: t.muted }, 14)
      x += 15
    }
    buf.text(x, y, l.text, { fg }, right(logArea) - x)
  })
  if (lines.length > shown.length && logArea.h > 0)
    buf.lineRight(rect(logArea.x, ly, right(b) - tabsW - 3 - logArea.x, 1), [[`↑ ${lines.length - shown.length} earlier`, { fg: t.faint }]])

  actionBar(
    c,
    p.footer,
    null,
    [
      ['■ Stop', 's', 'stop'],
      ['Copy log', 'c'],
      ['Open folder', 'f'],
      ['Level', '←→'],
    ],
    [['Back', 'esc']],
  )
  overlays(c, c.data.instances[0])
}

// ── Wizard ───────────────────────────────────────────────────────────────

const PICKED = { version: '1.21.4', loader: 'Fabric', loaderVersion: '0.16.10' }

function wizard(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  w.header(c, p.header, ['New instance'])
  const b = p.body
  const step = c.tabs(3)

  // Steps as chips; done ones show what was picked.
  const labels = [step > 0 ? PICKED.version : 'Version', step > 1 ? PICKED.loader : 'Loader', 'Name']
  let x = b.x
  labels.forEach((label, i) => {
    if (i) x = buf.text(x + 1, b.y, '›', { fg: t.faint }) + 1
    const on = i === step
    const done = i < step
    const line: Line = [
      [done ? '✓ ' : `${i + 1} `, { fg: done ? t.success : on ? t.accent : t.faint, bold: true }],
      [label, { fg: on ? t.strong : done ? t.text : t.dim, bold: on }],
    ]
    x = right(w.chip(c, x, b.y, line, on ? 'selected' : 'plain', `tab:${i}`))
  })

  const area = rect(b.x, b.y + 2, b.w, b.h - 2)
  if (step === 0) {
    const versions = data.versions.filter((v) => !v.snapshot && v.id.includes(ui.query.trim()))
    const sel = c.sel(Math.max(1, versions.length))
    const sx = w.section(c, area.x + 1, area.y, 'Releases', versions.length, true)
    const tog = w.toggle(c, right(area) - 9, area.y, false, false, 'snapshots')
    buf.text(tog.x - 11, area.y, 'Snapshots', { fg: t.dim })
    filter(c, rect(sx + 4, area.y, tog.x - 11 - sx - 7, 1), { placeholder: 'filter', maxW: 20 })
    const grid = rect(area.x, area.y + 2, area.w, area.h - 2)
    const L = place(grid, versions, sel, 16, 3, 8)
    const inUse = new Set(data.instances.map((i) => i.mc))
    for (const { item, i, r } of L.placed) {
      const lk = look(c, i, sel, true)
      const tag: Line | null = item.latest ? [['latest', { fg: t.success }]] : inUse.has(item.id) ? [['in use', { fg: t.muted }]] : null
      const body = head(c, r, matched(c, item.id, ui.query), tag, lk)
      buf.text(body.x, body.y, item.date, { fg: lk.on ? t.text : t.dim }, body.w)
      buf.region(r, `sel:${i}`)
    }
    if (L.moreY < bottom(grid)) more(c, grid.x + 1, L.moreY, L.above, L.below)
    actionBar(c, p.footer, ['Next: loader', '↵', 'tab:1'], [['Snapshots', 'space', 'snapshots']], [['Cancel', 'esc']])
  } else if (step === 1) {
    const loaders = [
      {
        name: 'Vanilla',
        version: '',
        text: 'The game exactly as Mojang ships it.',
        adds: 'Worlds · resource packs',
      },
      {
        name: 'Fabric',
        version: PICKED.loaderVersion,
        text: `A light mod loader: Sodium, Lithium, Iris and ${data.modTotal} more on Modrinth.`,
        adds: 'Mods · shaders · resource packs',
      },
    ]
    const sel = c.sel(loaders.length)
    w.section(c, area.x + 1, area.y, 'Loader', `for ${PICKED.version}`, true)
    const grid = rect(area.x, area.y + 2, area.w, area.h - 2)
    const L = place(grid, loaders, sel, 34, 6, 2)
    for (const { item, i, r } of L.placed) {
      const lk = look(c, i, sel, true)
      const body = head(c, r, [item.name], item.version ? [[item.version, { fg: t.dim }]] : null, lk)
      wrap(item.text, body.w)
        .slice(0, 2)
        .forEach((l, j) => buf.text(body.x, body.y + j, l, { fg: lk.on ? t.text : t.subtle }, body.w))
      buf.text(body.x, body.y + 3, item.adds, { fg: t.muted }, body.w)
      buf.region(r, `sel:${i}`)
    }
    actionBar(c, p.footer, ['Next: name', '↵', 'tab:2'], [['Back', '←', 'tab:0']], [['Cancel', 'esc']])
  } else {
    const sel = c.sel(2)
    const [form, , prev] = cols(area, ['1fr', GAP, Math.max(CARD_MIN + 2, Math.min(34, Math.floor(area.w * 0.36)))])
    const name = ui.query || `${PICKED.version} ${PICKED.loader}`
    w.section(c, form.x + 1, form.y, 'Name it', undefined, true)
    const fx = form.x + 10
    const fw = Math.min(40, right(form) - fx)
    buf.text(form.x + 1, form.y + 2, 'Name', { fg: sel === 0 ? t.strong : t.dim, bold: sel === 0 })
    w.field(c, rect(fx, form.y + 2, fw, 1), name, 'Instance name', sel === 0, 'query')
    if (sel !== 0) buf.cursor = null
    buf.text(form.x + 1, form.y + 4, 'Memory', { fg: sel === 1 ? t.strong : t.dim, bold: sel === 1 })
    w.choice(c, fx, form.y + 4, '4 GB', sel === 1, 'sel:1')
    buf.text(form.x + 1, form.y + 6, 'Java', { fg: t.dim })
    buf.text(fx + 1, form.y + 6, 'Java 21 (automatic)', { fg: t.text }, right(form) - fx - 1)
    wrap('Everything here can change later in Edit.', fw + 9).forEach((l, i) => buf.text(form.x + 1, form.y + 8 + i, l, { fg: t.muted }))

    w.section(c, prev.x + 1, prev.y, 'On your home', undefined, false)
    const draft: Instance = {
      name,
      mc: PICKED.version,
      loader: 'Fabric',
      loaderVersion: PICKED.loaderVersion,
      lastPlayed: null,
      playtime: '0m',
      worlds: [],
      mods: [],
      packs: [],
      memory: '4 GB',
      java: 'Java 21 (automatic)',
      fresh: true,
    }
    instanceCard(c, rect(prev.x, prev.y + 2, prev.w, CARD_H), draft, { on: true, focused: false, hover: false })
    actionBar(c, p.footer, ['Create instance', '↵', 'screen:home'], [['Back', '←', 'tab:1']], [['Cancel', 'esc']])
  }
  overlays(c, c.data.instances[0])
}

// ── Overlays ─────────────────────────────────────────────────────────────

const MENU: [string, string, boolean?][] = [
  ['▶ Play', '↵'],
  ['Play offline', 'o'],
  ['Mods', 'm'],
  ['Edit', 'e'],
  ['Open folder', 'f'],
  ['Duplicate', ''],
  ['Delete', 'd', true],
]

const WORLD_MENU: [string, string, boolean?][] = [
  ['▶ Play', '↵'],
  ['Play offline', 'o'],
  ['Open folder', 'f'],
  ['Back up', 'b'],
  ['Delete world', 'd', true],
]

const NEW_MENU: [string, string, boolean?][] = [
  ['+ Create', 'n'],
  ['Import a modpack', 'i'],
  ['Open instances folder', 'f'],
]

function overlays(c: Ctx, inst: Instance | undefined, anchor?: Rect, title?: string) {
  const { buf, t, ui, data } = c
  if (!ui.overlay) return
  const area = buf.area()
  if (ui.overlay === 'help') {
    const r = centered(area, Math.min(64, area.w - 4), Math.min(17, area.h - 2))
    const body = sheet(c, r, 'Keys', { hint: 'esc closes' })
    const [a, , bcol] = cols(body, ['1fr', 4, '1fr'])
    const block = (x: number, y: number, bw: number, title: string, keys: [string, string][]) => {
      if (y >= bottom(body)) return y
      buf.text(x, y, title.toUpperCase(), { fg: t.muted, bold: true })
      keys.forEach(([k, d], i) => {
        if (y + 1 + i >= bottom(body)) return
        buf.text(x, y + 1 + i, k, { fg: t.accent, bold: true }, 8)
        buf.text(x + 8, y + 1 + i, d, { fg: t.dim }, bw - 8)
      })
      return y + keys.length + 2
    }
    const y = block(a.x, a.y, a.w, 'Move', [
      ['↑↓←→', 'between cards'],
      ['g G', 'first · last card'],
      ['tab', 'details drawer'],
      ['/', 'filter the cards'],
    ])
    block(a.x, y, a.w, 'Act on a card', [
      ['↵', 'play'],
      ['o', 'play offline'],
      ['e', 'edit'],
      ['x', 'all actions'],
      ['d ⌫', 'delete'],
    ])
    const y2 = block(bcol.x, bcol.y, bcol.w, 'Go to', [
      ['n', 'new instance'],
      ['m', 'mods'],
      ['a', 'accounts'],
      ['s', 'settings'],
    ])
    block(bcol.x, y2, bcol.w, 'Anywhere', [
      [':', 'commands'],
      ['?', 'this help'],
      ['q', 'quit'],
    ])
  } else if (ui.overlay === 'palette') {
    const cmds: [string, string][] = [
      ...data.instances.slice(0, 3).map((i): [string, string] => [`Play ${i.name}`, i.lastPlayed ?? 'new']),
      ['New instance', 'n'],
      ['Browse mods', 'm'],
      ['Accounts', 'a'],
      ['Settings', 's'],
      ['Open folder', 'f'],
    ]
    const list = cmds.filter(([s]) => s.toLowerCase().includes(ui.oquery.toLowerCase()))
    const r = centered(area, Math.min(60, area.w - 4), Math.min(area.h - 2, 6 + Math.max(1, list.length)))
    sheet(c, r, 'Commands', { hint: '↵ run' })
    w.field(c, rect(r.x, r.y + 2, r.w, 1), ui.oquery, 'Type a command or an instance…', true)
    const sel = c.osel(Math.max(1, list.length))
    list.slice(0, Math.max(0, r.h - 6)).forEach(([cmd, hint], i) => {
      const row = rect(r.x, r.y + 4 + i, r.w, 1)
      const on = sel === i
      if (on) buf.fill(row, { bg: t.selection })
      else if (c.hovered(`osel:${i}`)) buf.fill(row, { bg: t.surface })
      buf.lineRight(inset(row, 0, 1), [[hint, { fg: on ? t.subtle : t.muted }]])
      buf.line(row.x + 1, row.y, matched(c, cmd, ui.oquery, { fg: on ? t.strong : t.text, bold: on }), {}, row.w - 12)
      buf.region(row, `osel:${i}`)
    })
    if (!list.length) buf.text(r.x + 1, r.y + 4, 'No command matches.', { fg: t.faint })
  } else if (ui.overlay === 'menu') {
    menu(c, inst, anchor, title)
  } else if (ui.overlay === 'confirm') {
    const name = inst?.name ?? 'Survival 1.21'
    const bits = inst ? [count(inst.worlds.length, 'world'), count(inst.mods.length, 'mod')].filter((s) => !s.startsWith('0')) : []
    const msg = `${bits.length ? `Its ${bits.join(' and ')} go too.` : 'It goes for good.'} This can’t be undone.`
    const rw = Math.min(56, area.w - 4)
    const lines = wrap(msg, rw - 2)
    const r = centered(area, rw, lines.length + 7)
    const body = sheet(c, r, `Delete “${name}”?`, { danger: true })
    lines.forEach((l, i) => buf.text(body.x, body.y + i, l, { fg: t.text }))
    const k = w.button(c, body.x, bottom(body) - 1, 'Keep', '↵', 'secondary', 'close')
    w.button(c, right(k) + 2, bottom(body) - 1, 'Delete', 'y', 'danger', 'delete')
  }
}

/** All actions for a card, dropped down from its title (or centred off home). */
function menu(c: Ctx, inst: Instance | undefined, anchor?: Rect, title?: string) {
  const { buf, t } = c
  const area = buf.area()
  const items = title ? WORLD_MENU : inst ? MENU : NEW_MENU
  const mw = Math.max(anchor?.w ?? 0, 28)
  const mh = 2 + items.length + (items.some(([, , d]) => d) ? 1 : 0) + 1
  let r: Rect
  if (anchor) {
    const x = Math.max(2, Math.min(anchor.x, area.w - 2 - mw))
    const y = Math.max(1, Math.min(anchor.y, area.h - 1 - mh))
    r = rect(x, y, mw, mh)
  } else r = centered(area, mw, mh)
  buf.fade(t.backdrop)
  buf.region(area, 'close')
  buf.clear(inset(r, -1, -2))
  const name = title ?? inst?.name ?? 'New instance'
  if (anchor) head(c, r, [name], inst && !title ? statusGlyph(c, inst) : null, { on: true, focused: true, hover: false })
  else sheet(c, r, name, { clear: false })
  buf.region(r, 'card')
  const sel = c.osel(items.length)
  let y = r.y + 2
  items.forEach(([label, key, danger], i) => {
    if (danger) {
      buf.hline(r.x + 1, y, r.w - 2, { fg: t.rule }, '╌')
      y++
    }
    const row = rect(r.x, y, r.w, 1)
    const on = sel === i
    if (on) buf.fill(row, { bg: danger ? t.errorBg : t.selection })
    else if (c.hovered(`osel:${i}`)) buf.fill(row, { bg: t.surface })
    buf.text(r.x + 1, y, label, danger ? { fg: t.error, bold: on } : { fg: on ? t.strong : t.text, bold: on })
    if (key) buf.lineRight(inset(row, 0, 1), [[key, { fg: on ? t.accent : t.muted, bold: on }]])
    buf.region(row, `osel:${i}`)
    y++
  })
  buf.hline(r.x, bottom(r) - 1, r.w, { fg: t.rule })
}

export default defineVariation({
  id: 'cards',
  archived: 'Set aside: big cards are a GUI idea; a TUI wants dense rows.',
  name: 'Cards',
  order: 20,
  blurb:
    'Home is a dashboard of instance tiles: everything you have, readable at a glance, with the selected card’s actions in one bar at the bottom. It trades the always-on detail pane (worlds, mods) for overview, so those move into a drawer on tab.',
  notes: [
    'A card = a bold title row over a rule, then 3–4 quiet lines; the grid’s whitespace is the edge. Selection tints only the title row and turns its rule into a heavy accent rule, so nothing fills two rows and nothing is vertical.',
    'Columns follow the width (2 at 80, 3 at 100, 4 at 140; tab’s drawer pushes it to 1–3). ↑↓ walks cards in reading order here; the real thing would add ←→ across columns.',
    'Each instance card’s last line says only what matters: a crash, a mod update, a mod turned off, or nothing at all. The status dot (green, amber, hollow) repeats it at a glance.',
    'Mods, versions and loaders reuse the same card; the wizard’s last step previews the new card as it will sit on home. The running game is that card grown to full width over its log.',
    'x drops the actions menu down from the selected card rather than centring it; help, commands and delete use the same title-band-over-rule sheet.',
  ],
  screens: { home, mods, wizard, play },
})
