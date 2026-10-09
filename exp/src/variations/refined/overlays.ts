// Refined overlays: small and anchored to what they act on. The `x` menu
// opens beside the selected row; the `:` palette is a prompt in the footer
// row with its matches stacked just above it, like a shell's completion.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { rect, width } from '../../term/buffer'
import type { Frame } from './frame'
import { filter, hints, hintsWidth, lit, type Hint } from './bits'

export function overlayMenu(c: Ctx, f: Frame, inst: Instance, rowY: number, x: number) {
  const { buf, t } = c
  const items: [string, string][] = [
    ['↵', 'Play'],
    ['o', 'Play offline'],
    ['e', 'Edit'],
    ['m', 'Mods'],
    ['f', 'Open folder'],
    ['r', 'Rename'],
    ['y', 'Duplicate'],
    ['d', 'Delete'],
  ]
  buf.fade(t.backdrop)
  buf.region(buf.area(), 'close')
  const w = 28
  const h = items.length + 2
  const y0 = Math.max(f.body.y, Math.min(rowY, f.footer.y - 1 - h))
  // The menu takes whole rows of the sheet beside the list: nothing peeks around it.
  buf.clear(rect(x - 2, y0, buf.cols - x + 2, h))
  buf.region(rect(x - 2, y0, w + 4, h), 'card')
  buf.text(x, y0, inst.name, { fg: t.strong, bold: true }, w)
  buf.hline(x, y0 + 1, w, { fg: t.rule })
  const sel = c.osel(items.length)
  items.forEach(([k, label], i) => {
    const y = y0 + 2 + i
    const on = sel === i
    const hov = c.hovered(`osel:${i}`)
    if (on) buf.fill(rect(x - 2, y, w + 4, 1), { bg: t.selection })
    else if (hov) buf.fill(rect(x - 2, y, w + 4, 1), { bg: t.surface })
    const danger = label === 'Delete'
    buf.text(x, y, label, danger ? { fg: t.error, bold: on } : on ? { fg: t.strong, bold: true } : { fg: t.text })
    buf.text(x + w - width(k), y, k, { fg: on ? t.subtle : t.faint })
    buf.region(rect(x - 2, y, w + 4, 1), `osel:${i}`)
  })
}

const COMMANDS: [string, string][] = [
  ['Play Survival 1.21', '↵'],
  ['New instance', 'n'],
  ['Mods for Survival 1.21', 'm'],
  ['Game log', 'l'],
  ['Accounts', 'a'],
  ['Settings', ''],
  ['Open instance folder', 'f'],
  ['Check for updates', ''],
  ['Quit', 'q'],
]

export function palette(c: Ctx, f: Frame) {
  const { buf, t, ui } = c
  buf.fade(t.backdrop)
  buf.region(buf.area(), 'close')
  const matches = filter(COMMANDS, ui.oquery, ([name]) => name)
  const sel = c.osel(Math.max(1, matches.length))
  const max = Math.min(matches.length, f.footer.y - f.body.y - 2)
  const top = f.footer.y - 1 - max
  const w = Math.min(56, f.w)
  buf.clear(rect(0, top - 1, buf.cols, buf.rows - top + 1))
  buf.region(rect(0, top - 1, buf.cols, buf.rows - top + 1), 'card')
  buf.hline(f.x - 1, top - 1, f.w + 2, { fg: t.rule })
  matches.slice(0, max).forEach(({ item: [name, key], at }, i) => {
    const y = top + i
    const on = sel === i
    if (on) buf.fill(rect(f.x - 2, y, w + 4, 1), { bg: t.selection })
    lit(c, f.x, y, name, at, on ? { fg: t.strong, bold: true } : { fg: t.text }, w - 4)
    if (key) buf.text(f.x + w - width(key), y, key, { fg: t.faint })
    buf.region(rect(f.x - 2, y, w + 4, 1), `osel:${i}`)
  })
  if (!matches.length) buf.text(f.x, f.footer.y - 1, 'No command matches', { fg: t.dim })
  // The prompt sits where the footer was.
  const r = f.footer
  const x = buf.text(r.x, r.y, ':', { fg: t.accent, bold: true }) + 1
  const end = buf.text(x, r.y, ui.oquery, { fg: t.strong })
  if (!ui.oquery) buf.text(x, r.y, 'type a command', { fg: t.faint })
  buf.cursor = { x: end, y: r.y }
  const keys: Hint[] = [['↵', 'run', 'close'], ['esc', 'close', 'close']]
  const hw = hintsWidth(keys)
  hints(c, r.x + r.w - hw, r.y, keys, hw)
}
