// Mods: home with the Mods panel open and the main view in search mode.
// `/` searches Modrinth, results list in the main view, the selected one's
// details sit under them; the installed list stays on the left.
//
// Panes: 0 Modrinth results (the main view) · 1 the installed Mods panel.

import type { Ctx } from '../../kit'
import type { InstalledMod, ModHit } from '../../data/mock'
import { type Line, type Rect, rect, bottom, wrap } from '../../term/buffer'
import * as f from './frame'
import { sideStack } from './side'
import { statusLine } from './home'
import * as mv from './main'
import { hitFor, moreHits, needs, sortedMods } from './data'

const num = (s: string) => parseFloat(s)

export function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const L = f.layout(c)
  const focus = c.panes(2)
  const inst = data.instances[0] ?? null
  const q = f.query(c)

  const all = [...data.modHits, ...moreHits].sort((a, b) => num(b.downloads) - num(a.downloads))
  const hits = focus === 0 && q ? all.filter((h) => h.name.toLowerCase().includes(q.toLowerCase())) : all
  const hsel = c.sel(Math.max(1, hits.length), 0)
  const installed = sortedMods(data, inst)
  const msel = c.sel(installed.length, 1)

  statusLine(c, L.status, false)
  sideStack(c, L.side, 2, data.instances, inst, [
    { focused: false, sel: 0, pane: null },
    { focused: false, sel: -1, pane: null },
    { focused: focus === 1, sel: msel, pane: 1 },
    { focused: false, sel: -1, pane: null },
  ])

  const m = L.main
  if (ui.overlay === 'help') {
    mv.helpView(c, m, focus === 0 ? 'Modrinth' : 'Installed', hints(c, focus, hits[hsel], installed[msel]))
    f.keyLine(c, L.keys, [['esc', 'close']], [])
    return
  }
  if (!inst) {
    const r = mv.mainFrame(c, m, ['Modrinth'])
    buf.text(r.x, r.y, 'Mods go into an instance. n makes one first.', { fg: t.dim }, r.w)
    f.keyLine(c, L.keys, [['n', 'new instance'], ['esc', 'back']])
    return
  }

  const total = q ? `${hits.length}` : `${hits.length} of ${data.modTotal}`
  const r = mv.mainFrame(c, m, ['Modrinth'], {
    focused: focus === 0,
    count: total,
    right: [[`${inst.mc} · ${inst.loader}`, { fg: t.dim }]],
    region: 'focus:0',
  })

  // The search field: one row, a band only while it has the cursor.
  const typing = f.typing(c) && focus === 0
  const sr = rect(m.x, r.y, m.w, 1)
  buf.fill(sr, { bg: typing ? t.selection : c.hovered('query') ? t.surfaceHi : t.surface })
  buf.text(r.x, r.y, '/', { fg: typing ? t.accent : t.muted, bold: true })
  if (q) {
    const end = buf.text(r.x + 2, r.y, q, { fg: t.strong }, r.w - 2)
    if (typing) buf.cursor = { x: end, y: r.y }
  } else {
    buf.text(r.x + 2, r.y, 'Search Modrinth', { fg: t.muted })
    if (typing) buf.cursor = { x: r.x + 2, y: r.y }
  }
  buf.lineRight(rect(r.x, r.y, r.w, 1), q ? [['esc', { fg: t.dim, bold: true }], [' clear', { fg: t.muted }]] : [['by downloads', { fg: t.muted }]])
  buf.region(sr, 'query')

  // Results in a fixed area; the details under them never move.
  const detailH = r.h >= 24 ? 6 : 5
  const list = rect(m.x, r.y + 2, m.w, r.h - 2 - detailH - 1)
  const start = f.windowStart(hits.length, hsel, list.h)
  const catW = r.w >= 48 ? 14 : 0
  const byW = r.w >= 72 ? 16 : 0
  hits.slice(start, start + list.h).forEach((h, j) => {
    const i = start + j
    const y = list.y + j
    const key = focus === 0 ? `sel:${i}` : `sel:${i}@0`
    const st = f.rowState(c, i === hsel, focus === 0, key)
    f.band(c, list, y, st)
    const mine = installed.find((x) => x.name === h.name)
    const [g, gfg] = mine?.update ? ['↑', t.success] : mine ? ['✓', t.success] : [' ', t.faint]
    buf.text(r.x - 2, y, g, { fg: gfg })
    buf.lineRight(rect(r.x, y, r.w, 1), [[h.downloads, { fg: st === 'focus' ? t.subtle : t.muted }]])
    const dx = r.x + r.w - 7
    if (catW) buf.text(dx - catW - 1, y, h.category, { fg: t.faint }, catW)
    if (byW) buf.text(dx - catW - byW - 2, y, h.author, { fg: t.faint }, byW - 1)
    buf.line(r.x, y, f.highlight(c, h.name, q, f.rowText(c, st, false)), {}, dx - catW - byW - 3 - r.x)
    buf.region(rect(m.x, y, m.w, 1), key)
  })
  if (!hits.length) buf.text(r.x, list.y, `Nothing on Modrinth matches “${q}” for ${inst.mc}.`, { fg: t.faint }, r.w)
  const below = hits.length - start - list.h
  if (below > 0) buf.lineRight(rect(r.x, bottom(list), r.w, 1), [[`↓ ${below} more`, { fg: t.faint }]])

  // Details follow whichever list has the cursor.
  const dr = rect(m.x, bottom(r) - detailH, m.w, detailH)
  if (focus === 1 && installed[msel]) detail(c, dr, hitFor(data, installed[msel].name), installed[msel])
  else if (hits[hsel]) detail(c, dr, hits[hsel], installed.find((x) => x.name === hits[hsel].name))

  c.enter(null)
  f.keyLine(c, L.keys, typing ? [['↵', 'done'], ['esc', 'clear']] : hints(c, focus, hits[hsel], installed[msel]))
}

function hints(c: Ctx, focus: number, hit: ModHit | undefined, mod: InstalledMod | undefined): f.Hint[] {
  void c
  if (focus === 1) {
    if (!mod) return [['tab', 'results'], ['esc', 'done']]
    return [...(mod.update ? ([['↵', 'update']] as f.Hint[]) : []), ['space', mod.enabled ? 'turn off' : 'turn on'], ['d', 'remove'], ['tab', 'results'], ['esc', 'done']]
  }
  const mine = hit && c.data.instances[0]?.mods.find((x) => x.name === hit.name)
  const first: f.Hint[] = !hit ? [] : mine?.update ? [['↵', 'update']] : mine ? [['d', 'remove']] : [['↵', 'install']]
  return [...first, ['/', 'search'], ['o', 'open page'], ['tab', 'installed'], ['esc', 'done']]
}

/** The selected mod under the results: a quiet rule, who, what, and what it needs. */
function detail(c: Ctx, r: Rect, h: ModHit, mine: InstalledMod | undefined) {
  const { buf, t, data } = c
  const state: Line = mine?.update
    ? [['↑ ', { fg: t.success }], [`${mine.version} → ${mine.update}`, { fg: t.success }]]
    : mine
      ? [['✓ ', { fg: t.success }], [`installed ${mine.version}`, { fg: t.success }]]
      : [[`${h.downloads} downloads`, { fg: t.dim }]]
  f.rule(c, r.x, r.y, r.w, [h.name], { titleSt: { fg: t.strong, bold: true }, right: state })
  const x = r.x + 2
  const w = r.w - 4
  const meta = [`by ${h.author}`, h.category, h.side.toLowerCase()]
  while (meta.length > 1 && meta.join('  ·  ').length > w) meta.pop()
  buf.text(x, r.y + 1, meta.join('  ·  '), { fg: t.dim }, w)
  const lines = wrap(h.description, w).slice(0, r.h - 3)
  lines.forEach((l, i) => buf.text(x, r.y + 2 + i, l, { fg: t.text }, w))
  const need = needs[h.name] ?? 'nothing'
  const have = (n: string) => !!data.instances[0]?.mods.find((x2) => x2.name === n)
  const needLine: Line =
    need === 'nothing'
      ? [['Needs nothing else', { fg: t.muted }], [`  ·  updated ${h.updated}`, { fg: t.muted }]]
      : [
          ['Needs ', { fg: t.muted }],
          ...need.split(', ').flatMap((n, i): Line => [
            [i ? ', ' : '', { fg: t.muted }],
            [n, { fg: t.text }],
            have(n) ? [' ✓', { fg: t.success }] : [' (added with it)', { fg: t.muted }],
          ]),
          [`  ·  updated ${h.updated}`, { fg: t.muted }],
        ]
  buf.line(x, r.y + 2 + lines.length, needLine, {}, w)
}
