// Ledger's overlays never float: help, confirmations and menus take the
// drawer's place under the same rule, and the command palette is a `:` line
// in the footer (vim, less). The table above stays put and readable.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { type Line, type Rect, rect, width } from '../../term/buffer'
import { fuzzy } from './fuzzy'
import type { State } from './data'
import { modsOf } from './data'
import { type Frame, type Hint, band, footer, ledger, lit, prompt, rule } from './ui'

export type Where = 'home' | 'mods' | 'play' | 'wizard'

const HELP: Record<Where, [string, [string, string][]][]> = {
  home: [
    ['move', [['↑↓ j k', 'move'], ['g G', 'top, bottom'], ['/', 'filter'], ['s', 'next sort'], ['esc', 'clear, back']]],
    ['play', [['↵', 'play, open'], ['o', 'play offline'], ['x', 'stop the game'], ['l', 'last log'], ['c', 'copy crash']]],
    ['manage', [['n', 'new instance'], ['e', 'edit'], ['m', 'mods'], ['u', 'update mods'], ['d', 'delete']]],
    ['mctui', [[':', 'commands'], ['a', 'accounts'], [',', 'settings'], ['?', 'these keys'], ['q', 'quit']]],
  ],
  mods: [
    ['move', [['↑↓ j k', 'move'], ['g G', 'top, bottom'], ['esc', 'back']]],
    ['installed', [['↵', 'update'], ['U', 'update all'], ['space', 'turn on/off'], ['d', 'remove']]],
    ['modrinth', [['/', 'search'], ['↵', 'install'], ['o', 'open page'], ['esc', 'installed']]],
    ['mctui', [[':', 'commands'], ['?', 'these keys'], ['q', 'quit']]],
  ],
  play: [
    ['log', [['↑↓ j k', 'scroll'], ['G', 'follow the tail'], ['/', 'find'], ['v', 'level']]],
    ['game', [['x', 'stop'], ['c', 'copy log'], ['o', 'open folder'], ['esc', 'home']]],
  ],
  wizard: [
    ['steps', [['↵', 'next'], ['←→', 'step'], ['esc', 'cancel']]],
    ['list', [['↑↓ j k', 'move'], ['/', 'filter'], ['space', 'snapshots']]],
  ],
}

/** Draws the open overlay into the drawer (or the footer, for `:`). */
export function overlay(c: Ctx, f: Frame, ruleY: number, drawer: Rect, where: Where, inst: Instance | null, state: State | null) {
  const { buf, t, ui } = c
  const area = rect(drawer.x, drawer.y, drawer.w, Math.max(drawer.h, 4))
  buf.fill(rect(0, ruleY, buf.cols, area.y + area.h - ruleY))
  buf.fill(rect(0, f.foot, buf.cols, 1))

  if (ui.overlay === 'help') {
    rule(c, f, ruleY, [['Keys', { fg: t.strong, bold: true }]], [['esc closes', { fg: t.muted }]])
    const groups = HELP[where]
    const colW = Math.floor(area.w / Math.max(groups.length, 3))
    groups.forEach(([title, keys], g) => {
      const x = area.x + g * colW
      const kw = Math.max(...keys.map(([k]) => width(k))) + 2
      buf.text(x, area.y, title.toUpperCase(), { fg: t.muted })
      keys.slice(0, area.h - 1).forEach(([k, label], i) => {
        const y = area.y + 1 + i
        buf.text(x, y, k, { fg: t.text, bold: true })
        buf.text(x + kw, y, label, { fg: t.dim }, colW - kw - 2)
      })
    })
    footer(c, f, [['esc', 'close'], ['?', 'close']])
    return
  }

  if (ui.overlay === 'confirm' && inst) {
    rule(c, f, ruleY, [[`Delete ${inst.name}?`, { fg: t.error, bold: true }]])
    const n = modsOf(inst).length
    const parts = [`${inst.worlds.length} world${inst.worlds.length === 1 ? '' : 's'}`, n ? `${n} mods` : '', 'its settings'].filter(Boolean)
    ledger(c, area, [
      { label: 'removes', line: [[parts.join(', '), { fg: t.text }], [' — this can’t be undone', { fg: t.dim }]] },
      { label: 'keeps', line: [['screenshots are moved to ~/Pictures/mctui first', { fg: t.dim }]] },
    ])
    footer(c, f, [['y', 'delete'], ['esc', 'keep it']])
    return
  }

  if (ui.overlay === 'menu' && inst && state?.kind === 'running') {
    rule(c, f, ruleY, [[`Stop ${inst.name}?`, { fg: t.warning, bold: true }]])
    ledger(c, area, [
      { label: 'stops', line: [['the game closes at once; anything since the last autosave is lost', { fg: t.text }]] },
      { label: 'better', line: [['Save and Quit from the game’s menu, and mctui notices', { fg: t.dim }]] },
    ])
    footer(c, f, [['y', 'stop'], ['esc', 'keep playing']])
    return
  }

  if (ui.overlay === 'menu' && inst) {
    const items: [string, string][] = [
      ['↵', 'Play'],
      ['o', 'Play offline'],
      ['e', 'Edit name, memory, Java'],
      ['m', 'Mods'],
      ['f', 'Open folder'],
      ['y', 'Duplicate'],
      ['d', 'Delete'],
    ]
    rule(c, f, ruleY, [[inst.name, { fg: t.strong, bold: true }]], [['actions', { fg: t.muted }]])
    const sel = c.osel(items.length)
    const perCol = Math.max(1, area.h)
    const colW = 30
    items.forEach(([k, label], i) => {
      const x = area.x + Math.floor(i / perCol) * colW
      const y = area.y + (i % perCol)
      if (x + colW > area.x + area.w + 2) return
      const on = i === sel
      const r = rect(x - 1, y, colW - 2, 1)
      if (on) buf.fill(r, { bg: t.selection })
      else if (c.hovered(`osel:${i}`)) buf.fill(r, { bg: t.surface })
      buf.text(x, y, k, { fg: t.text, bold: true })
      buf.text(x + 4, y, label, label === 'Delete' ? { fg: t.error } : on ? { fg: t.strong, bold: true } : { fg: t.dim })
      buf.region(r, `osel:${i}`)
    })
    footer(c, f, [['↵', 'do it'], ['esc', 'close']])
    return
  }

  if (ui.overlay === 'palette') {
    const cmds = [
      'play Survival 1.21',
      'new instance',
      'mods of Survival 1.21',
      'sort by name',
      'accounts',
      'settings',
      'open instance folder',
      'copy crash report',
      'check for mod updates',
    ]
    const shown = cmds
      .map((s) => ({ s, m: fuzzy(ui.oquery, s) }))
      .filter((x) => x.m)
      .sort((a, b) => b.m!.score - a.m!.score)
    rule(c, f, ruleY, [['Commands', { fg: t.strong, bold: true }]], [[`${shown.length} of ${cmds.length}`, { fg: t.muted }]])
    const sel = c.osel(Math.max(1, shown.length))
    const perCol = Math.max(1, area.h)
    const colW = Math.max(28, ...shown.map((x) => width(x.s) + 4))
    shown.forEach(({ s, m }, i) => {
      const x = area.x + Math.floor(i / perCol) * colW
      const y = area.y + (i % perCol)
      if (x + colW > area.x + area.w + 2) return
      const on = i === sel
      if (on) band(c, { ...f, x: x, w: colW - 2 }, y, true, false)
      buf.line(x, y, lit(c, s, m!.hits, on ? { fg: t.strong, bold: true } : { fg: t.dim }) as Line)
      buf.region(rect(x - 1, y, colW, 1), `osel:${i}`)
    })
    prompt(c, f, ':', ui.oquery, 'type a command', '', [['↵', 'run'], ['esc', 'close']] as Hint[])
  }
}
