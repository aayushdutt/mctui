// Overlays in the Panes look: framed popups (titled rule, closing rule) over
// the faded screen. The instance menu hangs off its row like a right-click.

import type { Ctx } from '../../kit'
import { type Rect, rect, right, bottom, inset, centered, cols } from '../../term/buffer'
import * as f from './frame'

export interface OverlayCtx {
  /** The screen name for the help title. */
  screen: string
  /** Where the instance menu hangs (its top-left). */
  anchor?: Rect
  /** What the menu and the confirm act on. */
  subject: string
}

/** Fades the screen, clears a popup with a 1-column margin, frames it. Returns the content area. */
function popup(c: Ctx, r: Rect, title: string, o: f.PanelOpts = {}): Rect {
  const { buf, t } = c
  buf.fade(t.backdrop)
  buf.region(buf.area(), 'close')
  buf.clear(inset(r, -1, -3))
  buf.region(r, 'card')
  return f.panel(c, r, title, { focused: true, ...o })
}

export function overlays(c: Ctx, o: OverlayCtx) {
  const { ui } = c
  if (ui.overlay === 'help') help(c, o)
  else if (ui.overlay === 'palette') palette(c, o)
  else if (ui.overlay === 'menu') menu(c, o)
  else if (ui.overlay === 'confirm') confirm(c, o)
}

function help(c: Ctx, o: OverlayCtx) {
  const { buf, t } = c
  const W = Math.min(buf.cols - 6, 86)
  const n = W - 2 >= 80 ? 3 : 2
  const H = Math.min(buf.rows - 2, n === 3 ? 11 : 19)
  const r = centered(buf.area(), W, H)
  const inner = popup(c, r, `Keys · ${o.screen}`, { right: [['esc', { fg: t.accent, bold: true }], [' close', { fg: t.dim }]] })
  const screenKeys: Record<string, [string, [string, string][]]> = {
    Home: ['Instance', [['↵', 'play'], ['o', 'play offline'], ['e', 'edit'], ['f', 'open folder'], ['x', 'actions menu'], ['d ⌫', 'delete']]],
    Mods: ['Mod', [['↵', 'install / update'], ['space', 'enable / disable'], ['d ⌫', 'remove'], ['o', 'Modrinth page'], ['←→', 'category'], ['/', 'search']]],
    'Game log': ['Log', [['s', 'stop the game'], ['←→', 'level'], ['/', 'find'], ['G', 'follow the end'], ['↵', 'show in log'], ['c', 'copy log']]],
    'New instance': ['Steps', [['↵', 'next / create'], ['tab', 'next step'], ['space', 'snapshots'], ['←→', 'change value'], ['/', 'filter'], ['esc', 'cancel']]],
  }
  const groups: [string, [string, string][]][] = [
    ['Move', [['↑↓ j k', 'move in panel'], ['g G', 'first / last'], ['pgup pgdn', 'page'], ['tab', 'next panel'], ['←→ h l', 'tab / value'], ['esc', 'back']]],
    screenKeys[o.screen] ?? screenKeys.Home,
    ['Go to', [['n', 'new instance'], ['m', 'mods'], ['a', 'accounts'], ['s', 'settings'], [':', 'commands'], ['q', 'quit']]],
  ]
  const cs = cols(rect(inner.x + 1, inner.y + 1, inner.w - 2, inner.h - 1), Array(n).fill('1fr'), 3)
  groups.forEach(([title, keys], gi) => {
    const col = cs[gi % n]
    const y0 = col.y + (gi >= n ? keys.length + 2 : 0)
    if (y0 >= bottom(inner)) return
    buf.hline(col.x, y0, col.w, { fg: t.rule })
    buf.line(col.x + 1, y0, [' ', [title, { fg: t.subtle, bold: true }], ' '])
    const kw = Math.max(...keys.map(([k]) => k.length)) + 3
    keys.forEach(([k, d], i) => {
      const y = y0 + 1 + i
      if (y >= bottom(inner)) return
      buf.text(col.x, y, k, { fg: t.accent, bold: true }, kw)
      buf.text(col.x + kw, y, d, { fg: t.text }, col.w - kw)
    })
  })
  buf.lineRight(rect(r.x, bottom(r) - 1, r.w - 2, 1), [[' mouse: click focuses, double-click plays, right-click opens the menu ', { fg: t.muted }]])
}

const COMMANDS: [string, string, string][] = [
  ['Play', 'Survival 1.21', '↵'],
  ['Play offline', 'Survival 1.21', 'o'],
  ['New instance', '', 'n'],
  ['Browse mods', 'Survival 1.21', 'm'],
  ['Edit instance', 'Survival 1.21', 'e'],
  ['Open folder', 'Survival 1.21', 'f'],
  ['Accounts', '', 'a'],
  ['Settings', '', 's'],
  ['Switch theme', 'dark', ''],
  ['Copy game log', 'last session', ''],
  ['Quit', '', 'q'],
]

function palette(c: Ctx, _o: OverlayCtx) {
  const { buf, t, ui } = c
  const q = ui.oquery.toLowerCase()
  const hits = COMMANDS.filter(([n, ctx]) => (n + ' ' + ctx).toLowerCase().includes(q))
  const W = Math.min(buf.cols - 6, 64)
  const H = Math.min(buf.rows - 4, hits.length + 5, 17)
  const r = rect(Math.floor((buf.cols - W) / 2), Math.max(1, Math.floor(buf.rows * 0.18)), W, Math.max(6, H))
  const inner = popup(c, r, 'Commands', { count: `${hits.length}/${COMMANDS.length}` })
  const fieldRow = rect(r.x, inner.y, r.w, 1)
  buf.fill(fieldRow, { bg: t.selection })
  buf.text(inner.x, inner.y, ':', { fg: t.accent, bold: true })
  const end = ui.oquery ? buf.text(inner.x + 2, inner.y, ui.oquery, { fg: t.strong }) : inner.x + 2
  if (!ui.oquery) buf.text(inner.x + 2, inner.y, 'Type a command', { fg: t.muted })
  buf.cursor = { x: end, y: inner.y }
  buf.region(fieldRow, 'query')
  const sel = c.osel(Math.max(1, hits.length))
  const fit = inner.h - 2
  const start = f.scrollStart(hits.length, sel, fit)
  hits.slice(start, start + fit).forEach(([name, ctx, key], j) => {
    const i = start + j
    const y = inner.y + 2 + j
    const row = rect(r.x, y, r.w, 1)
    const state = f.rowState(c, i === sel, true, `osel:${i}`)
    f.band(c, row, state)
    const x = buf.line(inner.x, y, f.highlight(c, name, ui.oquery, f.rowText(c, state)))
    if (ctx) buf.text(x + 2, y, ctx, { fg: t.dim }, right(inner) - x - 6)
    if (key) buf.lineRight(rect(inner.x, y, inner.w, 1), [[key, { fg: t.accent, bold: true }]])
    buf.region(row, `osel:${i}`)
  })
  if (!hits.length) buf.text(inner.x + 2, inner.y + 2, 'No command matches.', { fg: t.faint })
}

function menu(c: Ctx, o: OverlayCtx) {
  const { buf, t } = c
  const items: [string, string, boolean?][] = [
    ['▶ Play', '↵'],
    ['Play offline', 'o'],
    ['Edit', 'e'],
    ['Mods', 'm'],
    ['Open folder', 'f'],
    ['Duplicate', 'c'],
    ['Delete', 'd', true],
  ]
  const W = 30
  const H = items.length + 2
  const a = o.anchor ?? centered(buf.area(), W, H)
  const x = Math.min(a.x, buf.cols - W - 2)
  const y = Math.min(a.y, buf.rows - 1 - H)
  const r = rect(x, y, W, H)
  const inner = popup(c, r, o.subject)
  const sel = c.osel(items.length)
  items.forEach(([label, key, danger], i) => {
    const ry = inner.y + i
    const row = rect(r.x, ry, r.w, 1)
    const state = f.rowState(c, i === sel, true, `osel:${i}`)
    f.band(c, row, state)
    buf.text(inner.x, ry, label, danger ? { fg: t.error, bold: state === 'focus' } : f.rowText(c, state))
    buf.lineRight(rect(inner.x, ry, inner.w, 1), [[key, { fg: danger ? t.error : t.accent, bold: true }]])
    buf.region(row, `osel:${i}`)
  })
}

function confirm(c: Ctx, o: OverlayCtx) {
  const { buf, t, data } = c
  const inst = data.instances.find((i) => i.name === o.subject) ?? data.instances[0]
  const r = centered(buf.area(), Math.min(buf.cols - 6, 56), 9)
  const inner = popup(c, r, 'Delete instance', { tone: 'error', focused: false })
  buf.line(inner.x + 1, inner.y + 1, [['Delete ', { fg: t.text }], [`“${inst.name}”`, { fg: t.strong, bold: true }], [' and everything in it?', { fg: t.text }]], {}, inner.w - 1)
  const size = inst.worlds.length ? inst.worlds.map((w) => w.size).join(' + ') : 'none'
  const facts: [string, string][] = [
    ['Worlds', `${inst.worlds.length}  ·  ${size}`],
    ['Mods', String(inst.mods.length)],
  ]
  facts.forEach(([k, v], i) => {
    buf.text(inner.x + 3, inner.y + 3 + i, k, { fg: t.dim })
    buf.text(inner.x + 12, inner.y + 3 + i, v, { fg: t.text }, inner.w - 13)
  })
  const by = bottom(inner) - 1
  buf.text(inner.x + 1, by, 'This can’t be undone.', { fg: t.muted })
  // ` Keep  ↵ ` and ` Delete  y ` flush right; the safe one first.
  const delW = 'Delete'.length + 5
  const keepW = 'Keep'.length + 5
  const k = f.button(c, right(inner) - 1 - delW - 2 - keepW, by, 'Keep', '↵', 'secondary', 'close')
  f.button(c, right(k) + 2, by, 'Delete', 'y', 'danger', 'delete')
}
