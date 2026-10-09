// Overlays, kept small and anchored: a sheet that rises above the footer
// (help, commands, the row menu) and questions asked in the footer itself
// (confirm). The columns stay where they are, so you never lose your place.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { type Line, rect } from '../../term/buffer'
import type { Frame } from './ui'
import type { Ref } from './previews'
import { fuzzy } from './fuzzy'
import { lit } from './ui'

const HELP: [string, [string, string][]][] = [
  ['Move', [['↑↓ jk', 'up, down'], ['←→ hl', 'out, in'], ['g G', 'top, bottom'], ['/', 'filter']]],
  ['Act', [['↵', 'play, or act'], ['p', 'play, always'], ['n', 'new instance'], ['esc', 'clear, back']]],
  ['Change', [['space', 'turn on, off'], ['d', 'remove'], ['e', 'rename'], ['c', 'copy']]],
  ['Go', [['m', 'mods'], ['a', 'accounts'], [':', 'commands'], ['q', 'quit']]],
]

/** Clears `h` rows above the footer and draws the sheet's rule. Returns the first content row. */
function sheet(c: Ctx, f: Frame, h: number, label: string, right?: Line): number {
  const { buf, t } = c
  const top = f.footer.y - 1 - h
  buf.fill(rect(0, top, buf.cols, f.footer.y - top))
  buf.hline(f.body.x, top, f.body.w, { fg: t.rule })
  buf.line(f.body.x + 2, top, [' ', [label, { fg: t.text, bold: true }], ' '])
  if (right) buf.lineRight(rect(f.body.x, top, f.body.w - 2, 1), [' ', ...right, ' '])
  buf.region(rect(0, 0, buf.cols, top), 'close')
  return top + 2
}

export function overlays(c: Ctx, f: Frame, inst: Instance | null, ref: Ref | undefined) {
  const { buf, t, ui } = c
  if (!ui.overlay) return

  if (ui.overlay === 'help') {
    const y = sheet(c, f, 7, 'Keys', [['?', { fg: t.text, bold: true }], [' or esc closes', { fg: t.dim }]])
    const colW = Math.floor(f.body.w / HELP.length)
    HELP.forEach(([group, rows], i) => {
      const x = f.body.x + i * colW
      buf.text(x, y, group, { fg: t.muted, bold: true })
      rows.forEach(([k, label], j) => {
        buf.text(x, y + 1 + j, k, { fg: t.text, bold: true }, 6)
        buf.text(x + 6, y + 1 + j, label, { fg: t.dim }, colW - 7)
      })
    })
    return
  }

  if (ui.overlay === 'confirm') {
    const r = f.footer
    buf.fill(r)
    const [q, more]: [string, string] = !inst
      ? ['Nothing to remove here.', '']
      : ref?.k === 'mod'
        ? [`Remove ${ref.m.name} from ${inst.name}?`, 'Its config file stays.']
        : ref?.k === 'world'
          ? [`Delete the world “${ref.w.name}”?`, 'This can’t be undone.']
          : [`Delete “${inst.name}”?`, 'Its worlds and mods go too. This can’t be undone.']
    const line: Line = [[q, { fg: t.error, bold: true }], [`  ${more}`, { fg: t.dim }]]
    const keysW = 22
    buf.line(r.x, r.y, line, {}, r.w - keysW)
    buf.lineRight(r, [['y', { fg: t.text, bold: true }], [' yes', { fg: t.dim }], ['   esc', { fg: t.text, bold: true }], [' keep', { fg: t.dim }]])
    buf.region(rect(r.x + r.w - 20, r.y, 5, 1), 'key:y')
    return
  }

  // Commands and the row menu: a short list in the sheet, typed into from the footer.
  const name = inst?.name ?? 'instance'
  const cmds =
    ui.overlay === 'palette'
      ? [`Play ${name}`, `Play ${name} offline`, 'New instance', 'Mods', 'Accounts', 'Settings', 'Open folder', 'Copy game log']
      : [`Play ${name}`, 'Play offline', 'Rename', 'Duplicate', 'Open folder', 'Delete']
  const shown = cmds.map((cmd) => ({ cmd, hits: ui.overlay === 'palette' ? fuzzy(ui.oquery, cmd) : [] })).filter((x) => x.hits)
  const sel = c.osel(Math.max(1, shown.length))
  const h = Math.min(shown.length, f.body.h - 4) + 2
  const y = sheet(c, f, h, ui.overlay === 'palette' ? 'Commands' : name, [[`${shown.length}`, { fg: t.muted }]])
  shown.slice(0, h - 2).forEach(({ cmd, hits }, i) => {
    const row = rect(f.body.x - 1, y + i, Math.min(f.body.w + 2, 48), 1)
    const on = i === sel
    if (on) buf.fill(row, { bg: t.selection })
    else if (c.hovered(`osel:${i}`)) buf.fill(row, { bg: t.surface })
    const st = cmd === 'Delete' ? { fg: t.error, bold: on } : on ? { fg: t.strong, bold: true } : { fg: t.text }
    buf.line(row.x + 1, row.y, lit(c, cmd, hits, st), {}, row.w - 2)
    buf.region(row, `osel:${i}`)
  })
  if (ui.overlay === 'palette') {
    const r = f.footer
    buf.fill(r)
    const x = buf.text(r.x, r.y, ':', { fg: t.accent, bold: true })
    const end = buf.text(x + 1, r.y, ui.oquery, { fg: t.strong })
    buf.cursor = { x: end, y: r.y }
    buf.lineRight(r, [['↵', { fg: t.text, bold: true }], [' run', { fg: t.dim }], ['   esc', { fg: t.text, bold: true }], [' close', { fg: t.dim }]])
  }
}
