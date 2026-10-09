// Home: the sidebar of instances and the selected one's page. The page is
// calm and content-first (name, version, worlds, mods, the one action), and
// it becomes the live session while that instance's game runs.

import type { Ctx } from '../../kit'
import type { Crash, Instance, Task } from '../../data/mock'
import { type Line, type Rect, rect, right, bottom, wrap, width, lineWidth } from '../../term/buffer'
import * as p from './parts'
import { modStates } from './data'
import { helpPage, confirm } from './help'

export function home(c: Ctx) {
  const { data, ui } = c
  const L = p.layout(c)
  const insts = data.instances
  const focus = c.panes(2)
  const sel = c.sel(insts.length + 1, { pane: 0, numbered: true })
  const query = ui.overlay ? '' : ui.query
  p.sidebar(c, L, { sel, focused: focus === 0, query })
  p.account(c, L)

  if (ui.overlay && ui.overlay !== 'confirm') return helpPage(c, L)

  if (!insts.length) {
    welcome(c, L)
    c.enter('wizard')
    return p.footer(c, L, [['↵', 'new instance'], ['a', 'accounts'], ['?', 'help']])
  }
  if (sel === insts.length) {
    newPreview(c, L)
    c.enter('wizard')
    return p.footer(c, L, [['↵', 'new instance'], ['1–9', 'jump'], ['?', 'help']])
  }

  const inst = insts[sel]
  const run = data.running
  if (run?.instance === inst.name) {
    live(c, L, inst)
    c.enter('play')
    p.footer(c, L, [['↵', 'full log'], ['s', 'stop'], ['c', 'copy log'], ['1–9', 'jump'], ['?', 'help']])
  } else {
    page(c, L, inst, focus === 1)
    const crashed = data.crash?.instance === inst.name
    const task = data.tasks.find((k) => k.instance === inst.name && k.ratio > 0)
    const hints: [string, string][] = crashed
      ? [['↵', 'try again'], ['l', 'log'], ['c', 'copy report'], ['m', 'fix mods'], ['?', 'help']]
      : task
        ? [['x', 'cancel download'], ['1–9', 'jump'], ['?', 'help']]
        : focus === 1
          ? [['↵', 'play this world'], ['tab', 'instances'], ['?', 'help']]
          : data.account.status === 'expired'
            ? [['↵', 'sign in & play'], ['o', 'offline'], ['a', 'accounts'], ['m', 'mods'], ['?', 'help']]
            : [['↵', 'play'], ['o', 'offline'], ['m', 'mods'], ...(inst.mods.some((m) => m.update) ? ([['u', 'update']] as [string, string][]) : []), ['e', 'edit'], ['tab', 'worlds'], ['?', 'help']]
    p.footer(c, L, hints)
    c.enter(task ? null : 'play')
  }
  confirm(c, L, inst)
}

/** The instance page: what it is, the one action, what's in it. */
function page(c: Ctx, L: p.Layout, inst: Instance, focused: boolean) {
  const { buf, t, data } = c
  const P = L.page
  const crash = data.crash?.instance === inst.name ? data.crash : undefined
  const task = data.tasks.find((k) => k.instance === inst.name && k.ratio > 0)
  const played: Line = inst.lastPlayed
    ? [[' · played ', { fg: t.muted }], [inst.lastPlayed, { fg: t.dim }]]
    : [[' · never played', { fg: t.muted }]]
  p.title(c, P, [inst.name], [[`${inst.mc} · ${p.loaderName(inst)}`, { fg: t.dim }], ...played])

  let y = P.y + 3
  if (crash) y = crashBlock(c, P, y, crash)
  else if (task) y = downloadBlock(c, P, y, task)
  else if (inst.trouble) {
    buf.line(P.x, y, [['▲ ', { fg: t.warning }], ['It crashed on its last launch.', { fg: t.text }], ['  l', { fg: t.subtle, bold: true }], [' shows why', { fg: t.muted }]], {}, P.w)
    y += 2
  }

  if (!task) {
    const expired = data.account.status === 'expired'
    const b = p.pill(c, P.x, y, crash ? 'Try again' : expired ? 'Sign in & play' : 'Play', '↵', 'key:↵')
    // Beside the pill, the longest version of the note that fits.
    const offline: Line = [['o', { fg: t.subtle, bold: true }], [' plays offline', { fg: t.muted }]]
    const asides: Line[] = crash
      ? [[['or ', { fg: t.faint }], ['m', { fg: t.subtle, bold: true }], [' to fix mods first', { fg: t.muted }]]]
      : expired
        ? [[['sign-in expired', { fg: t.warning }], [' · ', { fg: t.faint }], ...offline], offline]
        : inst.fresh
          ? [[['first launch downloads the game, about 280 MB', { fg: t.muted }]], [['downloads about 280 MB first', { fg: t.muted }]]]
          : []
    const room = right(P) - right(b) - 3
    buf.line(right(b) + 3, y, asides.find((a) => lineWidth(a) <= room) ?? asides[asides.length - 1] ?? [], {}, room)
    y += 2
  }

  // What's in it. Worlds are the page's own list (tab focuses it, ↵ plays straight into one).
  const states = modStates(c, inst.name, inst.mods)
  const modded = inst.loader !== 'Vanilla'
  const names = [...inst.worlds.map((w) => w.name), ...inst.mods.map((m) => m.name)]
  const nameW = Math.min(26, Math.max(16, ...names.map((n) => width(n) + 3)))
  const avail = bottom(P) - y - (otherRunning(c, inst) ? 2 : 0)

  if (L.wide && modded) {
    const colW = Math.floor((P.w - 4) / 2)
    const left = rect(P.x, y, colW, avail)
    const rightCol = rect(P.x + colW + 4, y, P.w - colW - 4, avail)
    const wy = worlds(c, left, inst, nameW, focused)
    packs(c, rect(left.x, wy + 1, left.w, bottom(left) - wy - 1), inst)
    mods(c, rightCol, states, nameW)
  } else {
    y = worlds(c, rect(P.x, y, P.w, avail), inst, nameW, focused) + 1
    if (modded) y = mods(c, rect(P.x, y, P.w, bottom(P) - y), states, nameW) + 1
    packs(c, rect(P.x, y, P.w, bottom(P) - y), inst)
  }

  otherRunningLine(c, L, inst)
}

/** Worlds, one row each; the page's selectable list. Returns the y after it. */
function worlds(c: Ctx, r: Rect, inst: Instance, nameW: number, focused: boolean): number {
  const { buf, t } = c
  if (r.h < 2) return r.y
  const sel = c.sel(Math.max(1, inst.worlds.length), 1)
  p.label(c, r.x, r.y, 'worlds', inst.worlds.length)
  if (!inst.worlds.length) {
    buf.text(r.x, r.y + 1, 'None yet: they show up after you play.', { fg: t.faint }, r.w)
    return r.y + 2
  }
  // A left-packed grid shared with mods: name, then facts in fixed columns.
  const full = r.w >= nameW + 32
  inst.worlds.slice(0, r.h - 1).forEach((w, i) => {
    const y = r.y + 1 + i
    const on = focused && sel === i
    p.band(c, rect(r.x, y, r.w, 1), on, focused, `sel:${i}@1`)
    buf.text(r.x, y, w.name, on ? { fg: t.strong, bold: true } : { fg: t.text }, nameW - 2)
    buf.text(r.x + nameW, y, w.lastPlayed, { fg: t.dim }, 10)
    if (full) {
      buf.text(r.x + nameW + COL, y, w.mode.toLowerCase(), { fg: t.muted })
      buf.lineRight(rect(r.x + nameW + COL * 2, y, 6, 1), [[w.size, { fg: t.muted }]])
    }
  })
  return r.y + 1 + Math.min(inst.worlds.length, r.h - 1)
}

/** The grid's column step after the name. */
const COL = 12

/** Mods, trouble first, one row each. Returns the y after it. */
function mods(c: Ctx, r: Rect, states: ReturnType<typeof modStates>, nameW: number): number {
  const { buf, t } = c
  if (r.h < 2) return r.y
  p.label(c, r.x, r.y, 'mods', states.length)
  if (!states.length) {
    buf.line(r.x, r.y + 1, [['None yet · ', { fg: t.faint }], ['m', { fg: t.subtle, bold: true }], [' adds some from Modrinth', { fg: t.faint }]], {}, r.w)
    return r.y + 2
  }
  const room = r.h - 1
  const show = states.length > room ? room - 1 : states.length
  states.slice(0, show).forEach(({ mod, note }, i) => {
    const y = r.y + 1 + i
    buf.text(r.x, y, mod.name, { fg: mod.enabled ? t.text : t.muted }, nameW - 2)
    buf.text(r.x + nameW, y, mod.version, { fg: t.muted }, COL - 2)
    if (note) {
      const fg = { update: t.success, warn: t.warning, error: t.error, quiet: t.muted }[note.tone]
      buf.text(r.x + nameW + COL, y, note.text, { fg }, right(r) - (r.x + nameW + COL))
    }
  })
  if (show < states.length)
    buf.line(r.x, r.y + 1 + show, [[`+ ${states.length - show} more  `, { fg: t.faint }], ['m', { fg: t.subtle, bold: true }], [' all mods', { fg: t.faint }]])
  return r.y + 1 + Math.min(states.length, room)
}

function packs(c: Ctx, r: Rect, inst: Instance) {
  const { buf, t } = c
  if (!inst.packs.length || r.h < 2) return
  p.label(c, r.x, r.y, 'resource packs', inst.packs.length)
  inst.packs.slice(0, r.h - 1).forEach((name, i) => buf.text(r.x, r.y + 1 + i, name, { fg: t.text }, r.w))
}

/** The last launch ended badly: what broke in a sentence, then the details. */
function crashBlock(c: Ctx, P: Rect, y: number, crash: Crash): number {
  const { buf, t } = c
  const [tech, human] = crash.reason.includes(' — ') ? crash.reason.split(' — ') : ['', crash.reason]
  const head = p.pageRow(P, y)
  buf.fill(head, { bg: t.errorBg })
  buf.line(P.x, y, [['✗ ', { fg: t.error, bold: true }], ['Crashed ', { fg: t.strong, bold: true }], [`${crash.when} · ${crash.exit}`, { fg: t.text }]], {}, P.w - 1)
  let ry = y + 2
  for (const l of wrap(`${human}.`, P.w).slice(0, 2)) buf.text(P.x, ry++, l, { fg: t.strong })
  if (tech) buf.text(P.x, ry++, tech, { fg: t.muted }, P.w)
  return ry + 1
}

/** A first launch fetching the game: how far, how much, and the way out. */
function downloadBlock(c: Ctx, P: Rect, y: number, task: Task): number {
  const { buf, t } = c
  const pct = `${Math.round(task.ratio * 100)}%`
  buf.line(P.x, y, [[`${c.spinner()} `, { fg: t.accent }], [task.label, { fg: t.text }]], {}, P.w)
  const bw = Math.min(40, P.w - 6)
  p.bar(c, P.x, y + 1, bw, task.ratio)
  buf.text(P.x + bw + 2, y + 1, pct, { fg: t.strong, bold: true })
  buf.text(P.x, y + 2, task.detail, { fg: t.muted }, P.w)
  buf.line(P.x, y + 4, [['It starts by itself when the download is done · ', { fg: t.muted }], ['x', { fg: t.subtle, bold: true }], [' cancels', { fg: t.muted }]], {}, P.w)
  return y + 6
}

/** The live session: the page itself, while this instance's game runs. */
function live(c: Ctx, L: p.Layout, inst: Instance) {
  const { buf, t, data } = c
  const P = L.page
  const run = data.running!
  p.title(c, P, [inst.name], [[`${inst.mc} · ${p.loaderName(inst)}`, { fg: t.dim }], [` · ${run.world} as ${data.account.name}`, { fg: t.muted }]])
  status(c, P, P.y + 3)

  const y = P.y + 5
  p.label(c, P.x, y, 'log')
  buf.lineRight(rect(P.x, y, P.w, 1), [['following', { fg: t.faint }]])
  const body = rect(P.x, y + 1, P.w, bottom(P) - y - 1)
  data.log.slice(-body.h).forEach((l, i) => p.logLine(c, body.x, body.y + i, body.w, l, L.wide))
  buf.region(rect(P.x, P.y + 3, P.w, bottom(P) - P.y - 3), 'screen:play')
}

/** `● running 18m · 1 error · 3 warnings            2.1 of 4 GB`. */
export function status(c: Ctx, P: Rect, y: number) {
  const { buf, t, data } = c
  const run = data.running!
  const parts: Line = [['● ', { fg: t.success }], ['running ', { fg: t.strong, bold: true }], [run.elapsed, { fg: t.strong }]]
  if (run.errors) parts.push([' · ', { fg: t.faint }], [`${run.errors} error${run.errors > 1 ? 's' : ''}`, { fg: t.error }])
  if (run.warnings) parts.push([' · ', { fg: t.faint }], [`${run.warnings} warnings`, { fg: t.warning }])
  // Memory on the right while there's air for it; the state never gives way.
  const mem = lineWidth(parts) + 4 + width(run.memory) <= P.w
  const mx = mem ? buf.lineRight(rect(P.x, y, P.w, 1), [[run.memory, { fg: t.muted }]]) : right(P) + 2
  buf.line(P.x, y, parts, {}, mx - P.x - 2)
}

const otherRunning = (c: Ctx, inst: Instance) => !!c.data.running && c.data.running.instance !== inst.name

/** Another instance's game is running: one quiet line at the foot of the page. */
function otherRunningLine(c: Ctx, L: p.Layout, inst: Instance) {
  const { buf, t, data } = c
  if (!otherRunning(c, inst)) return
  const run = data.running!
  const i = data.instances.findIndex((x) => x.name === run.instance)
  const y = bottom(L.page) - 1
  const line: Line = [['● ', { fg: t.success }], [run.instance, { fg: t.text }], [` is running · ${run.elapsed}`, { fg: t.muted }]]
  if (i >= 0 && i < 9) line.push(['   ', {}], [String(i + 1), { fg: t.subtle, bold: true }], [' shows it', { fg: t.muted }])
  buf.line(L.page.x, y, line, {}, L.page.w)
  buf.region(rect(L.page.x, y, L.page.w, 1), `sel:${i}@0`)
}

/** First run: nothing in the sidebar but `+ new`, and a page that says what to do. */
function welcome(c: Ctx, L: p.Layout) {
  const { buf, t, data } = c
  const P = L.page
  p.title(c, P, [`Welcome, ${data.account.name}.`], [['Nothing here yet. Let’s make your first instance.', { fg: t.dim }]])
  let y = P.y + 3
  for (const l of wrap('An instance is one Minecraft setup: a version, a loader, and its own worlds and mods. Make as many as you like; each gets a number in the sidebar, and 1–9 jump straight to it.', Math.min(P.w, 64)))
    buf.text(P.x, y++, l, { fg: t.text })
  y += 1
  const latest = data.versions.find((v) => v.latest)?.id ?? ''
  const b = p.pill(c, P.x, y, 'New instance', '↵', 'key:↵')
  buf.line(right(b) + 3, y, [['↵ ↵ ↵ ', { fg: t.subtle, bold: true }], [`makes ${latest} with Fabric`, { fg: t.muted }]], {}, right(P) - right(b) - 3)
}

/** `+ new` selected: the page previews what ↵ is about to make. */
function newPreview(c: Ctx, L: p.Layout) {
  const { buf, t, data } = c
  const P = L.page
  const latest = data.versions.find((v) => v.latest)?.id ?? ''
  p.title(c, P, ['New instance'], [['A version, a loader and a name. The defaults are good.', { fg: t.dim }]])
  const rows: [string, string][] = [
    ['version', `${latest}, the latest release`],
    ['loader', 'Fabric, so mods work'],
    ['name', `${latest} Fabric`],
  ]
  rows.forEach(([k, v], i) => {
    buf.text(P.x, P.y + 3 + i, k, { fg: t.muted })
    buf.text(P.x + 10, P.y + 3 + i, v, { fg: t.text }, P.w - 10)
  })
  const b = p.pill(c, P.x, P.y + 7, 'Start', '↵', 'key:↵')
  buf.text(right(b) + 3, P.y + 7, 'then ↵ ↵ to take each default', { fg: t.muted }, right(P) - right(b) - 3)
}
