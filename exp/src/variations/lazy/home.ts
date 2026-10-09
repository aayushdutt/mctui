// Home: lazygit's model. Four side panels about the selected instance
// (Instances, Worlds, Mods, Packs), the focused one open; the main view shows
// what the focused panel has selected; the account is one line on top.
//
// Panes: 0 Instances · 1 Worlds · 2 Mods · 3 Packs · 4 the status line.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { rect } from '../../term/buffer'
import * as f from './frame'
import { sideStack } from './side'
import * as mv from './main'
import { sortedMods, stateOf } from './data'

export const PANEL_NAMES = ['Instances', 'Worlds', 'Mods', 'Packs', 'Account']

export function home(c: Ctx) {
  const { data, ui } = c
  const L = f.layout(c)
  const focus = c.panes(5)
  for (const n of [1, 2, 3, 4]) c.bind(String(n), `focus:${n - 1}`)
  if (c.data.running) c.bind('s', 'key:')
  const q = f.query(c)

  const all = data.instances
  const iq = focus === 0 ? q : ''
  const shown = iq ? all.filter((i) => i.name.toLowerCase().includes(iq.toLowerCase())) : all
  const isel = c.sel(Math.max(1, shown.length), 0)
  const inst: Instance | null = shown[isel] ?? null
  const mq = focus === 2 ? q : ''
  const sorted = sortedMods(data, inst)
  const mods = mq ? sorted.filter((m) => m.name.toLowerCase().includes(mq.toLowerCase())) : sorted
  const wsel = c.sel(inst?.worlds.length ?? 0, 1)
  const msel = c.sel(inst?.loader === 'Vanilla' ? 0 : mods.length, 2)
  const psel = c.sel(inst?.packs.length ?? 0, 3)

  statusLine(c, L.status, focus === 4)
  const open = focus <= 3 ? focus : 0
  // Help, the menu and the commands take the main view, and the focus with it.
  const away = ui.overlay === 'help' || ui.overlay === 'menu' || ui.overlay === 'palette'
  const on = (p: number) => focus === p && !away
  sideStack(c, L.side, open, shown, inst, [
    { focused: on(0), sel: isel, pane: 0, q: iq },
    { focused: on(1), sel: wsel, pane: 1 },
    { focused: on(2), sel: msel, pane: 2, q: mq },
    { focused: on(3), sel: psel, pane: 3 },
  ])

  // The main view follows the cursor.
  const m = L.main
  const ctxName = PANEL_NAMES[focus]
  if (ui.overlay === 'help') mv.helpView(c, m, ctxName, helpKeys(focus))
  else if (ui.overlay === 'menu' && inst) mv.menuView(c, m, inst.name, menu(inst))
  else if (ui.overlay === 'palette') mv.paletteView(c, m)
  else if (focus === 4) mv.accountView(c, m)
  else if (!all.length) mv.welcomeView(c, m)
  else if (!inst) mv.mainFrame(c, m, ['Instances'])
  else if (focus === 0) mv.instanceView(c, m, inst)
  else if (focus === 1) mv.worldView(c, m, inst, inst.worlds[wsel])
  else if (focus === 2) mv.modView(c, m, inst, mods[msel])
  else mv.packView(c, m, inst, inst.packs[psel], psel)

  // What ↵ does.
  if (!all.length) c.enter('wizard')
  else if (focus === 0 || focus === 1) c.enter(inst ? 'play' : null)
  else c.enter(null)

  if (ui.overlay === 'help' || ui.overlay === 'menu') f.keyLine(c, L.keys, ui.overlay === 'menu' ? [['↵', 'run'], ['esc', 'close']] : [['esc', 'close']], [])
  else if (ui.overlay === 'palette') mv.promptLine(c, L.keys)
  else if (ui.overlay === 'confirm' && inst && focus <= 3) confirm(c, L.keys, focus, inst, { wsel, msel, psel, mods })
  else f.keyLine(c, L.keys, verbs(c, focus, inst))
}

/** The top-left status: the account, quiet unless it needs something. */
export function statusLine(c: Ctx, r: ReturnType<typeof rect>, focused: boolean) {
  const { buf, t, data } = c
  const a = data.account
  if (focused) buf.fill(rect(r.x, r.y, r.w, 1), { bg: t.selection })
  else if (c.hovered('focus:4')) buf.fill(rect(r.x, r.y, r.w, 1), { bg: t.surface })
  if (a.status === 'expired') {
    buf.text(r.x, r.y, '▲', { fg: t.warning })
    buf.line(r.x + 2, r.y, [['sign-in expired', { fg: t.warning, bold: focused }], ['  ↵', { fg: t.strong, bold: true }], [' renew', { fg: t.dim }]], {}, r.w - 2)
  } else {
    buf.line(r.x + 2, r.y, [[a.name, { fg: focused ? t.strong : t.text, bold: focused }], [' · signed in', { fg: t.muted }]], {}, r.w - 2)
  }
  buf.region(rect(r.x, r.y, r.w, 1), 'focus:4')
}

/** The verbs that work right now, per focused panel and state. */
export function verbs(c: Ctx, focus: number, inst: Instance | null): f.Hint[] {
  const { data } = c
  if (!data.instances.length) return [['↵', 'new instance']]
  if (f.typing(c)) return [['↵', 'keep filter'], ['esc', 'clear']]
  const expired = data.account.status === 'expired'
  const tail: f.Hint[] = []
  if (focus === 4) return expired ? [['↵', 'renew sign-in'], ['a', 'accounts'], ...tail] : [['a', 'accounts'], ['tab', 'panels'], ...tail]
  if (!inst) return tail
  const s = stateOf(data, inst)
  if (focus === 0) {
    if (s.running) return [['↵', 'full log'], ['s', 'stop'], ['c', 'copy log'], ['n', 'new'], ['/', 'filter'], ...tail]
    if (s.crashed) return [['↵', 'try again'], ['m', 'fix mods'], ['l', 'log'], ['c', 'copy report'], ...tail]
    const play: f.Hint = expired ? ['↵', 'renew & play'] : s.task ? ['↵', 'play when ready'] : ['↵', 'play']
    return [play, ['o', 'offline'], ['n', 'new'], ['e', 'edit'], ['/', 'filter'], ['x', 'more'], ...tail]
  }
  if (focus === 1) return inst.worlds.length ? [['↵', 'play this world'], ['b', 'back up'], ['r', 'rename'], ['d', 'delete'], ...tail] : tail
  if (focus === 2) {
    if (inst.loader === 'Vanilla') return [['n', 'new Fabric instance'], ...tail]
    const mod = sortedMods(data, inst)[c.sel(inst.mods.length, 2)]
    if (!mod) return [['m', 'add mods'], ...tail]
    const culprit = data.crash?.instance === inst.name && mod.name === 'Fabric API'
    const first: f.Hint[] = culprit || mod.update ? [['↵', 'update']] : []
    return [...first, ['space', mod.enabled ? 'turn off' : 'turn on'], ['d', 'remove'], ['m', 'add mods'], ['/', 'filter'], ...tail]
  }
  return inst.packs.length ? [['space', 'turn off'], ['[ ]', 'order'], ['d', 'remove'], ['m', 'add packs'], ...tail] : [['m', 'add packs'], ...tail]
}

/** Every key for the focused panel, for the help view. */
function helpKeys(focus: number): f.Hint[] {
  return [
    [['↵', 'play'], ['o', 'play offline'], ['e', 'edit'], ['f', 'open folder'], ['x', 'more actions'], ['d', 'delete'], ['s', 'stop the game'], ['l', 'last log']],
    [['↵', 'play this world'], ['b', 'back up'], ['r', 'rename'], ['f', 'open folder'], ['d', 'delete']],
    [['↵', 'update'], ['u', 'update all'], ['space', 'turn on / off'], ['m', 'add from Modrinth'], ['d', 'remove']],
    [['space', 'turn on / off'], ['[ ]', 'move up / down'], ['m', 'add from Modrinth'], ['d', 'remove']],
    [['↵', 'renew sign-in'], ['a', 'all accounts']],
  ][focus] as f.Hint[]
}

function menu(inst: Instance): [string, string, boolean?][] {
  return [
    ['Play', '↵'],
    ['Play offline', 'o'],
    ['Edit', 'e'],
    ['Add mods', 'm'],
    ['Open folder', 'f'],
    ['Duplicate', 'c'],
    ['Export as .mrpack', ''],
    [`Delete ${inst.name}`, 'd', true],
  ]
}

function confirm(
  c: Ctx,
  r: ReturnType<typeof rect>,
  focus: number,
  inst: Instance,
  s: { wsel: number; msel: number; psel: number; mods: Instance['mods'] },
) {
  if (focus === 0) {
    const n = inst.worlds.length
    return mv.confirmLine(c, r, `Delete ${inst.name}?`, n ? `Its ${n} world${n > 1 ? 's' : ''} go too.` : 'Nothing to lose yet.')
  }
  if (focus === 1 && inst.worlds[s.wsel]) return mv.confirmLine(c, r, `Delete ${inst.worlds[s.wsel].name}?`, `${inst.worlds[s.wsel].size}, no undo.`)
  if (focus === 2 && s.mods[s.msel]) return mv.confirmLine(c, r, `Remove ${s.mods[s.msel].name}?`, 'Its config stays.')
  if (focus === 3 && inst.packs[s.psel]) return mv.confirmLine(c, r, `Remove ${inst.packs[s.psel]}?`, '')
  f.keyLine(c, r, [['esc', 'back']])
}
