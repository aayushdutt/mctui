// Mods: what's installed, the download queue and the search filters stacked
// on the left; Modrinth results as a table on the right with the selected
// mod's details under it. Categories are tabs (←→), so the filters work.

import type { Ctx } from '../../kit'
import type { ModHit, InstalledMod } from '../../data/mock'
import { type Line, type Rect, type Style, rect, cols, rows, right, bottom, wrap } from '../../term/buffer'
import * as f from './frame'
import { moreHits, modFacts } from './data'
import { overlays } from './overlays'

const num = (s: string) => parseFloat(s)
const CATEGORIES = ['All', 'Optimization', 'Library', 'Utility', 'Decoration', 'Adventure']

export function mods(c: Ctx) {
  const { buf, t, data } = c
  const W = buf.cols
  const inst = data.instances[0]
  const body = rect(1, 0, W - 2, buf.rows - 1)
  const focus = c.panes(3)
  const cat = c.tabs(CATEGORIES.length)

  const all = [...data.modHits, ...moreHits].sort((a, b) => num(b.downloads) - num(a.downloads))
  const byCat = all.filter((h) => cat === 0 || h.category === CATEGORIES[cat])
  const hits = byCat.filter((h) => h.name.toLowerCase().includes(f.query(c).toLowerCase()))

  const leftW = W >= 124 ? 46 : W >= 96 ? 38 : 32
  const [left, , rightR] = cols(body, [leftW, 2, '1fr'])

  // Left: Installed, Queue, Filters.
  const installedH = 3 + inst.mods.length
  const queueH = 4
  const [instR, , queueR, , filterR] = rows(left, [installedH, 1, queueH, 1, '1fr'])
  const selInstalled = focus === 1 ? c.sel(inst.mods.length) : -1
  installed(c, instR, inst.mods, selInstalled, focus === 1)
  queue(c, queueR)
  const filterRows = CATEGORIES.length + 3
  const selFilter = focus === 2 ? c.sel(filterRows) : -1
  if (filterR.h >= 5) filters(c, filterR, all, cat, selFilter, focus === 2)

  // Right: results, then the selected one.
  const detailH = rightR.h >= 26 ? 10 : 8
  const [resR, , detR] = rows(rightR, ['1fr', 1, detailH])
  const selHit = c.sel(Math.max(1, hits.length), 0)
  results(c, resR, hits, selHit, focus === 0, `${hits.length} of ${cat === 0 && !f.query(c) ? data.modTotal : hits.length}`)
  if (focus === 1) installedDetail(c, detR, inst.mods[selInstalled])
  else detail(c, detR, hits[selHit], inst.mods)

  const hints: [string, string][] =
    focus === 0
      ? [['↵', 'install'], ['/', 'search'], ['←→', 'category'], ['tab', 'panel'], ['o', 'page'], ['esc', 'back'], ['?', 'help']]
      : focus === 1
        ? [['↵', 'update'], ['space', 'on/off'], ['d', 'remove'], ['tab', 'panel'], ['esc', 'back'], ['?', 'help']]
        : [['space', 'pick'], ['←→', 'category'], ['tab', 'panel'], ['esc', 'back'], ['?', 'help']]
  f.statusBar(c, f.typing(c) ? { label: 'SEARCH' } : { label: 'MODS' }, [[inst.name, { fg: t.text }]], hints)
  overlays(c, { screen: 'Mods', subject: inst.name })
}

function installed(c: Ctx, r: Rect, mods: InstalledMod[], sel: number, focused: boolean) {
  const { buf, t } = c
  const updates = mods.filter((m) => m.update).length
  const inner = f.panel(c, r, 'Installed', {
    focused,
    count: mods.length,
    region: 'focus:1',
    right: updates ? [[`↑ ${updates} update${updates > 1 ? 's' : ''}`, { fg: t.secondary }]] : undefined,
  })
  const p = f.place(inner, [{ label: 'Mod', w: 'fr' }, { label: 'Version', w: 8 }, { label: 'Status', w: 7 }])
  f.header(c, inner.y, p)
  mods.forEach((m, i) => {
    const y = inner.y + 1 + i
    if (y >= bottom(inner)) return
    const row = rect(r.x, y, r.w, 1)
    const state = f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${i}` : 'focus:1')
    f.cell(c, p[0], y, [[m.name, f.rowText(c, state)]])
    f.cell(c, p[1], y, [[m.version, { fg: t.subtle }]])
    f.cell(c, p[2], y, m.update ? [['↑ ' + m.update, { fg: t.secondary }]] : m.enabled ? [['● on', { fg: t.success }]] : [['○ off', { fg: t.dim }]])
  })
}

function queue(c: Ctx, r: Rect) {
  const { buf, t } = c
  const inner = f.panel(c, r, 'Queue', { count: 2, right: [['2.1 MB/s', { fg: t.dim }]] })
  const y0 = inner.y
  const x = buf.line(inner.x, y0, [[c.spinner(), { fg: t.accent }], ' ', ['Sodium 0.6.5', { fg: t.text }]], {}, inner.w)
  const tail = '  64%'
  const barW = right(inner) - x - 2 - tail.length
  if (barW >= 4) {
    const bx = f.bar(c, x + 2, y0, barW, 0.64)
    buf.text(bx, y0, tail, { fg: t.subtle })
  }
  buf.line(inner.x, y0 + 1, [['○', { fg: t.muted }], ' ', ['Mod Menu 12.0.0', { fg: t.dim }]], {}, inner.w)
  buf.lineRight(rect(inner.x, y0 + 1, inner.w, 1), [['queued', { fg: t.muted }]])
}

function filters(c: Ctx, r: Rect, all: ModHit[], cat: number, sel: number, focused: boolean) {
  const { buf, t } = c
  const inner = f.panel(c, r, 'Filters', { focused, region: 'focus:2' })
  let y = inner.y
  let row = 0
  const line = (label: string, on: boolean, count: string, region: string) => {
    if (y >= bottom(inner)) return
    const band = rect(r.x, y, r.w, 1)
    const state = f.rowState(c, row === sel, focused, `sel:${row}`)
    f.band(c, band, state ?? (c.hovered(region) ? 'hover' : null))
    buf.line(inner.x, y, [[on ? '● ' : '○ ', { fg: on ? t.accent : t.faint }], [label, on ? { fg: t.strong, bold: !!state } : f.rowText(c, state)]])
    buf.lineRight(rect(inner.x, y, inner.w, 1), [[count, { fg: t.muted }]])
    buf.region(band, focused ? `sel:${row}` : region)
    y++
    row++
  }
  buf.text(inner.x, y++, 'Category', { fg: t.muted })
  CATEGORIES.forEach((name, i) => {
    const n = i === 0 ? all.length : all.filter((h) => h.category === name).length
    line(name, i === cat, String(n), `tab:${i}`)
  })
  if (y + 2 >= bottom(inner)) return
  y++
  buf.text(inner.x, y++, 'Runs on', { fg: t.muted })
  line('Client', true, '', 'side:client')
  line('Server', true, '', 'side:server')
}

function results(c: Ctx, r: Rect, hits: ModHit[], sel: number, focused: boolean, count: string) {
  const { buf, t } = c
  const inner = f.panel(c, r, 'Modrinth', { focused, count, region: 'focus:0', right: [['by downloads ↓', { fg: t.dim }]] })
  // The search row.
  const sy = inner.y
  const srow = rect(r.x, sy, r.w, 1)
  const typing = f.typing(c)
  buf.fill(srow, { bg: typing ? t.selection : c.hovered('query') ? t.surfaceHi : t.surface })
  buf.text(inner.x, sy, '/', { fg: typing ? t.accent : t.muted, bold: typing })
  const end = f.query(c) ? buf.text(inner.x + 2, sy, f.query(c), { fg: t.strong }) : inner.x + 2
  if (!f.query(c)) buf.text(inner.x + 2, sy, 'Search Modrinth', { fg: t.muted })
  if (typing) buf.cursor = { x: end, y: sy }
  buf.lineRight(rect(inner.x, sy, inner.w, 1), [['1.21.4', { fg: t.dim }], ['  ·  ', { fg: t.faint }], ['Fabric', { fg: t.dim }]])
  buf.region(srow, 'query')

  const iw = inner.w
  const spec: f.Col[] = [{ label: '  Mod', w: 'fr' }]
  if (iw >= 76) spec.push({ label: 'Author', w: 12 })
  if (iw >= 50) spec.push({ label: 'Category', w: 12 })
  if (iw >= 90) spec.push({ label: 'Updated', w: 8, align: 'right' })
  spec.push({ label: 'Downloads', w: 9, align: 'right' })
  const list = rect(inner.x, sy + 1, inner.w, bottom(inner) - sy - 1)
  const p = f.place(list, spec)
  f.header(c, list.y, p)
  const fit = list.h - 1
  const start = f.scrollStart(hits.length, sel, fit)
  hits.slice(start, start + fit).forEach((h, j) => {
    const i = start + j
    const y = list.y + 1 + j
    const row = rect(r.x, y, r.w, 1)
    const state = f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${i}` : 'focus:0')
    const glyph: Line = h.name === 'Sodium' ? [['↑ ', { fg: t.secondary }]] : h.installed ? [['✓ ', { fg: t.success }]] : ['  ']
    for (const col of p) {
      const v: Line =
        col.label === '  Mod'
          ? [...glyph, ...f.highlight(c, h.name, f.query(c), f.rowText(c, state))]
          : col.label === 'Author'
            ? [[h.author, { fg: t.dim }]]
            : col.label === 'Category'
              ? [[h.category, { fg: t.dim }]]
              : col.label === 'Updated'
                ? [[h.updated, { fg: t.muted }]]
                : [[h.downloads, { fg: state ? t.text : t.subtle }]]
      f.cell(c, col, y, v)
    }
  })
  if (!hits.length) buf.text(list.x + 2, list.y + 1, `Nothing on Modrinth matches “${f.query(c)}”.`, { fg: t.faint }, list.w - 2)
  const below = hits.length - start - fit
  if (below > 0) buf.lineRight(rect(r.x, bottom(r) - 1, r.w - 2, 1), [[` ↓ ${below} more `, { fg: t.muted }]])
}

function detail(c: Ctx, r: Rect, hit: ModHit | undefined, mods: InstalledMod[]) {
  const { buf, t } = c
  if (!hit) {
    f.panel(c, r, 'Nothing selected')
    return
  }
  const mine = mods.find((m) => m.name === hit.name)
  const status: Line = mine?.update
    ? [['↑ ', { fg: t.secondary }], [`${mine.version} → ${mine.update}`, { fg: t.subtle }]]
    : mine
      ? [['✓ Installed ', { fg: t.success }], [mine.version, { fg: t.subtle }]]
      : [[`${hit.downloads} downloads`, { fg: t.dim }]]
  const inner = f.panel(c, r, [[hit.name, { fg: t.strong }]], { right: status })
  let y = inner.y
  buf.line(inner.x, y++, meta(c, [`by ${hit.author}`, hit.category, hit.side]), {}, inner.w)
  // Description, then facts, then a blank row and the buttons.
  const room = inner.h - 3
  const desc = wrap(hit.description, inner.w)
  const facts: [string, string][] = [
    ['Updated', hit.updated],
    ['Needs', modFacts[hit.name]?.needs ?? 'Fabric API'],
    ['License', modFacts[hit.name]?.license ?? 'MIT'],
  ]
  const descRows = Math.min(desc.length, Math.max(1, room - (room > 3 ? facts.length : 0)))
  desc.slice(0, descRows).forEach((l, i) => buf.text(inner.x, y++, i === descRows - 1 && desc.length > descRows ? l + '…' : l, { fg: t.text }, inner.w))
  const kvRows = room - descRows
  if (kvRows > 0) {
    if (inner.w >= 72) {
      // One line: `Updated 6d ago   Needs nothing   License MIT`.
      const line: Line = []
      facts.forEach(([k, v], i) => line.push([(i ? '     ' : '') + k + ' ', { fg: t.dim }], [v, { fg: t.text }]))
      buf.line(inner.x, ++y - 0, line, {}, inner.w)
    } else
      facts.slice(0, kvRows).forEach(([k, v]) => {
        buf.text(inner.x, y, k, { fg: t.dim })
        buf.text(inner.x + 9, y++, v, { fg: t.text }, inner.w - 9)
      })
  }
  const by = bottom(inner) - 1
  let x = inner.x
  if (mine?.update) x = right(f.button(c, x, by, `Update to ${mine.update}`, '↵', 'primary', 'install')) + 2
  else if (!mine) x = right(f.button(c, x, by, 'Install', '↵', 'primary', 'install')) + 2
  if (mine) x = right(f.button(c, x, by, 'Remove', 'd', 'secondary', 'key:d')) + 2
  f.button(c, x, by, 'Modrinth page', 'o', 'secondary', 'page')
}

function installedDetail(c: Ctx, r: Rect, m: InstalledMod | undefined) {
  const { buf, t } = c
  if (!m) return
  const inner = f.panel(c, r, [[m.name, { fg: t.strong }]], {
    right: m.update ? [['↑ update ', { fg: t.secondary }], [m.update, { fg: t.subtle }]] : [['● enabled', { fg: t.success }]],
  })
  const facts: [string, string][] = [
    ['Version', m.update ? `${m.version}  →  ${m.update}` : m.version],
    ['Needs', modFacts[m.name]?.needs ?? 'Fabric API'],
    ['Added', 'Sep 24'],
    ['Size', m.name === 'Fabric API' ? '2.3 MB' : m.name === 'Sodium' ? '1.1 MB' : '640 KB'],
    ['From', 'Modrinth'],
  ]
  facts.forEach(([k, v], i) => {
    if (inner.y + i >= bottom(inner) - 1) return
    buf.text(inner.x, inner.y + i, k, { fg: t.dim })
    buf.text(inner.x + 10, inner.y + i, v, { fg: t.text }, inner.w - 10)
  })
  const by = bottom(inner) - 1
  let x = inner.x
  if (m.update) x = right(f.button(c, x, by, `Update to ${m.update}`, '↵', 'primary', 'install')) + 2
  x = right(f.button(c, x, by, m.enabled ? 'Disable' : 'Enable', 'space', 'secondary', 'toggle')) + 2
  f.button(c, x, by, 'Remove', 'd', 'secondary', 'key:d')
}

/** `a · b · c` in dim with faint dots. */
function meta(c: Ctx, parts: string[]): Line {
  const out: Line = []
  parts.forEach((p, i) => {
    if (i) out.push(['  ·  ', { fg: c.t.faint } as Style])
    out.push([p, { fg: c.t.dim }])
  })
  return out
}
