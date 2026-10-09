// New instance: the side panels become the steps (Version, Loader, Name) and
// the main view previews what ↵ will make. Every step starts on a good pick,
// so ↵ ↵ ↵ creates the latest release with Fabric under a sensible name.
//
// Panes: 0 Version · 1 Loader · 2 Name.

import type { Ctx } from '../../kit'
import type { Version } from '../../data/mock'
import { type Line, type Rect, rect } from '../../term/buffer'
import * as f from './frame'
import { statusLine } from './home'
import * as mv from './main'
import { defaultName, downloadSize, folder, keepEnd, loaders } from './data'

const javaFor = (id: string) => (id.startsWith('26.') ? '25' : id.startsWith('1.21') ? '21' : id.startsWith('1.20') || id.startsWith('1.18') ? '17' : '8')
const fabricOk = (id: string) => id.startsWith('26.') || Number(id.split('.')[1]) >= 14

export function wizard(c: Ctx) {
  const { buf, t, data, ui } = c
  const L = f.layout(c)
  const focus = c.panes(3)
  const q = focus === 0 ? f.query(c) : ''
  const releases = data.versions.filter((v) => !v.snapshot)
  const shown = q ? releases.filter((v) => v.id.includes(q)) : releases
  const vsel = c.sel(Math.max(1, shown.length), 0)
  const ver: Version = shown[vsel] ?? releases[0]
  const ok = fabricOk(ver.id)
  // Fabric first where it runs; vanilla is the only choice for old versions.
  const opts = ok ? loaders : [loaders[1], loaders[0]]
  const lsel = c.sel(opts.length, 1)
  const loader = opts[lsel]
  const usable = loader.name === 'Vanilla' || ok
  c.sel(1, 2)
  const typed = focus === 2 && (f.typing(c) || ui.query) ? ui.query : ''
  const name = typed || defaultName(ver.id, usable ? loader.name : 'Vanilla')

  statusLine(c, L.status, false)
  const cap = f.foldRows(c.buf.rows)
  const fold = [cap, 2, 1]
  const need = [Math.max(1, shown.length), 2, 1][focus]
  const [vr, lr, nr] = f.stack(L.side, focus, fold, need)

  // 1 Version
  const done = (i: number): Line[] => (focus > i ? [[['✓', { fg: t.success }]]] : [])
  const vx = f.rule(c, vr.x, vr.y, vr.w, ['Version'], {
    focused: focus === 0,
    num: '1',
    count: q ? `${shown.length}/${releases.length}` : releases.length,
    segs: focus === 0 && (f.typing(c) || q) ? [] : done(0),
    region: 'focus:0',
  })
  if (focus === 0 && (f.typing(c) || q)) f.ruleFilter(c, rect(vr.x, vr.y, vr.w, 1), vx + 1, q, f.typing(c))
  list(c, vr, shown.length, vsel, focus === 0, 0, (i, y, st) => {
    const v = shown[i]
    const tail: Line = v.latest ? [['latest', { fg: t.success }]] : [[v.date, { fg: st === 'focus' ? t.subtle : t.muted }]]
    const tx = buf.lineRight(rect(vr.x + 2, y, vr.w - 4, 1), tail)
    buf.line(vr.x + 2, y, f.highlight(c, v.id, q, f.rowText(c, st)), {}, tx - vr.x - 3)
  })

  // 2 Loader
  f.rule(c, lr.x, lr.y, lr.w, ['Loader'], { focused: focus === 1, num: '2', segs: done(1), region: 'focus:1' })
  list(c, lr, opts.length, lsel, focus === 1, 1, (i, y, st) => {
    const l = opts[i]
    const off = l.name === 'Fabric' && !ok
    const tail: Line = off ? [[`not for ${ver.id}`, { fg: t.faint }]] : [[l.version ? `${l.version}  ·  ${l.note}` : l.note, { fg: st === 'focus' ? t.subtle : t.muted }]]
    const tx = buf.lineRight(rect(lr.x + 2, y, lr.w - 4, 1), tail)
    buf.text(lr.x + 2, y, l.name, off ? { fg: t.faint } : f.rowText(c, st), tx - lr.x - 3)
  })

  // 3 Name: a field, the cursor in it once it has focus.
  f.rule(c, nr.x, nr.y, nr.w, ['Name'], { focused: focus === 2, num: '3', region: 'focus:2' })
  const ny = nr.y + 1
  if (focus === 2) buf.fill(rect(nr.x, ny, nr.w, 1), { bg: t.selection })
  const end = buf.text(nr.x + 2, ny, name, focus === 2 ? { fg: t.strong, bold: true } : { fg: t.text }, nr.w - 4)
  if (focus === 2) buf.cursor = { x: end, y: ny }
  buf.region(rect(nr.x, ny, nr.w, 1), focus === 2 ? 'query' : 'focus:2')

  // The main view: what ↵ will make.
  const m = L.main
  if (ui.overlay === 'help') {
    mv.helpView(c, m, 'New instance', hints(focus))
    f.keyLine(c, L.keys, [['esc', 'close']], [])
    return
  }
  preview(c, m, ver, usable ? loader : loaders[1], name, focus)
  // ↵ takes the step's pick and moves on; on Name it creates.
  c.enter(focus === 2 ? 'home' : `focus:${focus + 1}`)
  f.keyLine(c, L.keys, f.typing(c) ? [['↵', 'done'], ['esc', 'undo']] : hints(focus))
}

/** A panel's rows with the selection kept in view. */
function list(c: Ctx, r: Rect, n: number, sel: number, focused: boolean, pane: number, row: (i: number, y: number, st: f.RowState) => void) {
  const fit = r.h - 1
  const start = f.windowStart(n, sel, fit)
  const end = Math.min(n, start + fit)
  const more: Line = []
  if (start > 0) more.push([`↑ ${start}`, { fg: c.t.muted }])
  if (end < n) more.push([`${start > 0 ? '  ' : ''}↓ ${n - end}`, { fg: c.t.muted }])
  if (more.length && !(focused && (f.typing(c) || f.query(c)))) c.buf.lineRight(rect(r.x, r.y, r.w - 1, 1), [' ', ...more, ' '])
  for (let i = start; i < Math.min(n, start + fit); i++) {
    const y = r.y + 1 + (i - start)
    const key = focused ? `sel:${i}` : `sel:${i}@${pane}`
    const st = f.rowState(c, i === sel, focused, key)
    f.band(c, r, y, st)
    row(i, y, st)
    c.buf.region(rect(r.x, y, r.w, 1), key)
  }
}

function preview(c: Ctx, m: Rect, ver: Version, loader: (typeof loaders)[number], name: string, step: number) {
  const { buf, t } = c
  const r = mv.mainFrame(c, m, ['New instance'], { right: [[`step ${step + 1} of 3`, { fg: t.dim }]] })
  let y = r.y
  buf.text(r.x, y++, name, { fg: t.strong, bold: true }, r.w)
  const loaderText = loader.name === 'Vanilla' ? 'Vanilla' : `Fabric ${loader.version}`
  buf.text(r.x, y++, `Minecraft ${ver.id}  ·  ${loaderText}`, { fg: t.dim }, r.w)
  y++
  const lw = 11
  f.kv(c, r.x, y++, r.w, 'Released', [[ver.date, { fg: t.text }], [ver.latest ? '  ·  the latest' : '', { fg: t.dim }]], lw)
  f.kv(c, r.x, y++, r.w, 'Java', [[javaFor(ver.id), { fg: t.text }], ['  ·  fetched when needed', { fg: t.dim }]], lw)
  f.kv(c, r.x, y++, r.w, 'Download', [[downloadSize[ver.id] ?? 'about 290 MB', { fg: t.text }], ['  ·  on first play', { fg: t.dim }]], lw)
  f.kv(c, r.x, y++, r.w, 'Folder', [[keepEnd(folder(name), r.w - lw), { fg: t.dim }]], lw)
  y++
  const why = [
    'Releases only; [space] shows snapshots too.',
    'Fabric runs mods, and plays like vanilla until you add some.',
    'Type to rename it. The folder follows the name.',
  ][step]
  y = mv.para(c, r.x, y, r.w, why, { fg: t.dim }, 2) + 1
  const next = [
    [['↵', { fg: t.strong, bold: true }], [` takes ${ver.id}, then the loader.`, { fg: t.muted }]],
    [['↵', { fg: t.strong, bold: true }], [` takes ${loader.name}, then the name.`, { fg: t.muted }]],
    [['↵', { fg: t.strong, bold: true }], [' creates it; ↵ again plays.', { fg: t.muted }]],
  ][step] as Line
  buf.line(r.x, y, next, {}, r.w)
}

function hints(focus: number): f.Hint[] {
  if (focus === 0) return [['↵', 'next'], ['space', 'snapshots'], ['/', 'filter'], ['tab', 'step'], ['esc', 'cancel']]
  if (focus === 1) return [['↵', 'next'], ['tab', 'step'], ['esc', 'cancel']]
  return [['↵', 'create'], ['/', 'rename'], ['tab', 'step'], ['esc', 'cancel']]
}
