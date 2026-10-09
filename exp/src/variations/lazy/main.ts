// The main view: whatever the focused panel has selected, drawn the moment
// the cursor lands on it. An instance's overview, a world, a mod, a pack,
// the account; the running game's log; and the keys, menu and commands,
// which take the main view instead of floating over the screen.

import type { Ctx } from '../../kit'
import type { Instance, InstalledMod, LogLine, World } from '../../data/mock'
import { type Line, type Rect, type Style, rect, right, bottom, wrap, truncate } from '../../term/buffer'
import * as f from './frame'
import { changes, crashFix, crashTail, folder, hitFor, keepEnd, lastSession, needs, packFacts, stateOf, worldFacts } from './data'

/** The main view's rule; returns where its content goes. */
export function mainFrame(c: Ctx, r: Rect, title: Line, o: f.RuleOpts = {}): Rect {
  f.rule(c, r.x, r.y, r.w, title, { titleSt: { fg: c.t.strong, bold: true }, ...o })
  return rect(r.x + 2, r.y + 1, r.w - 4, r.h - 1)
}

const loaderText = (inst: Instance) => (inst.loader === 'Vanilla' ? 'Vanilla' : `${inst.loader} ${inst.loaderVersion}`)

/**
 * Wrapped paragraph; returns the next free y. Keys written `[l]` are drawn
 * bright and bold, the way the key line draws them.
 */
export function para(c: Ctx, x: number, y: number, w: number, s: string, st: Style, max = 3) {
  const keys = new Set([...s.matchAll(/\[([^\]]+)\]/g)].map((m) => m[1]))
  const lines = wrap(s.replace(/\[([^\]]+)\]/g, '$1'), w).slice(0, max)
  lines.forEach((l, i) => {
    const line: Line = []
    l.split(' ').forEach((word, j) => {
      if (j) line.push([' ', st])
      line.push([word, keys.has(word) ? { fg: c.t.strong, bold: true } : st])
    })
    c.buf.line(x, y + i, line, {}, w)
  })
  return y + lines.length
}

// ── Instance ──────────────────────────────────────────────────────────────

export function instanceView(c: Ctx, m: Rect, inst: Instance) {
  const { buf, t, data } = c
  const s = stateOf(data, inst)
  if (s.running) return runningView(c, m, inst)
  if (s.crashed) return crashView(c, m, inst)
  const stateWord: Line = s.task
    ? [['↓ ', { fg: t.text }], [`downloading ${Math.round(s.task.ratio * 100)}%`, { fg: t.text }]]
    : inst.fresh
      ? [['○ ', { fg: t.faint }], ['not downloaded', { fg: t.dim }]]
      : inst.trouble
        ? [['▲ ', { fg: t.warning }], ['crashed last time', { fg: t.warning }]]
        : [['● ', { fg: t.success }], ['ready', { fg: t.success }]]
  const r = mainFrame(c, m, [inst.name], { right: stateWord })
  let y = r.y
  buf.text(r.x, y, `Minecraft ${inst.mc}  ·  ${loaderText(inst)}`, { fg: t.dim }, r.w)
  y += 2

  if (data.account.status === 'expired') {
    buf.text(r.x - 2, y, '▲', { fg: t.warning })
    buf.text(r.x, y++, 'Sign-in expired: online play needs a fresh one.', { fg: t.warning }, r.w)
    buf.line(r.x, y++, [['↵', { fg: t.strong, bold: true }], [' renews it, then plays  ·  ', { fg: t.dim }], ['o', { fg: t.strong, bold: true }], [' plays offline', { fg: t.dim }]], {}, r.w)
    y++
  }

  if (s.task) {
    const pct = `${Math.round(s.task.ratio * 100)}%`
    buf.text(r.x, y, s.task.label, { fg: t.text }, r.w)
    y++
    const bw = Math.min(32, r.w - pct.length - 16)
    const bx = f.bar(c, r.x, y, bw, s.task.ratio)
    buf.line(bx + 2, y, [[pct, { fg: t.strong, bold: true }], ['  ·  20s left', { fg: t.dim }]])
    y++
    buf.text(r.x, y, s.task.detail.replace(' · ', '  ·  '), { fg: t.dim }, r.w)
    y++
    buf.text(r.x, y, 'It opens as soon as this finishes.', { fg: t.muted }, r.w)
    y += 2
  }

  const sess = lastSession[inst.name]
  const lw = 13
  if (inst.fresh && !s.task) {
    y = para(c, r.x, y, r.w, 'Never played. The first [↵] downloads the game, about 281 MB, then starts it.', { fg: t.text })
    y++
  } else if (sess) {
    const last: Line = sess.ok
      ? [[inst.lastPlayed ?? '', { fg: t.text }], [`  ·  ${sess.length} in ${sess.world}`, { fg: t.dim }]]
      : [[inst.lastPlayed ?? '', { fg: t.text }], ['  ·  ', { fg: t.dim }], [sess.end, { fg: t.warning }]]
    f.kv(c, r.x, y++, r.w, 'Last played', last, lw)
  }
  if (!inst.fresh) f.kv(c, r.x, y++, r.w, 'Played', [[inst.playtime, { fg: t.text }], ['  in all', { fg: t.dim }]], lw)
  f.kv(c, r.x, y++, r.w, 'Java', [[inst.java.replace(/^Java /, ''), { fg: t.text }], [`  ·  ${inst.memory}`, { fg: t.dim }]], lw)
  f.kv(c, r.x, y++, r.w, 'Folder', [[keepEnd(folder(inst.name), r.w - lw), { fg: t.dim }]], lw)

  const upd = inst.mods.filter((md) => md.update)
  if (upd.length) {
    y++
    const q = s.queued
    f.kv(c, r.x, y++, r.w, q ? 'Updating' : 'Update', [
      [upd[0].name, { fg: t.text }],
      [` ${upd[0].version} → `, { fg: t.dim }],
      [upd[0].update!, { fg: t.success }],
      [q ? '  ·  queued' : '', { fg: t.dim }],
    ], lw)
  }
  if (inst.trouble && sess && !sess.ok) {
    y++
    y = para(c, r.x, y, r.w, 'It stopped 12 s in with Java 8 under Rosetta. [l] shows that log; [e] picks another Java.', { fg: t.dim })
  }
  return y
}

/** The game is running: a one-line session header over the live log. */
function runningView(c: Ctx, m: Rect, inst: Instance) {
  const { buf, t, data } = c
  const run = data.running!
  const r = mainFrame(c, m, [inst.name], {
    right: [['● ', { fg: t.success }], ['running ', { fg: t.success }], [f.elapsed(c, run.elapsed), { fg: t.success }]],
  })
  const head: Line = [
    [run.world, { fg: t.text }],
    [r.w >= 56 ? `  ·  ${run.memory}` : '', { fg: t.dim }],
    ['  ·  ', { fg: t.faint }],
    ['✗ ', { fg: t.error }],
    [`${run.errors} error`, { fg: t.text }],
    ['  ', {}],
    ['▲ ', { fg: t.warning }],
    [`${run.warnings} warnings`, { fg: t.text }],
  ]
  buf.line(r.x, r.y, head, {}, r.w)
  const body = rect(r.x, r.y + 2, r.w, r.h - 2)
  logTail(c, body, data.log, { gutter: true, wrapAll: false })
}

/** It just crashed: what broke in a sentence, the fix, then the last lines. */
function crashView(c: Ctx, m: Rect, inst: Instance) {
  const { t, data } = c
  const crash = data.crash!
  const r = mainFrame(c, m, [inst.name], { right: [['✗ ', { fg: t.error }], [`crashed ${crash.when}`, { fg: t.error }]] })
  const [tech, human] = crash.reason.split(' — ')
  let y = r.y
  y = para(c, r.x, y, r.w, `${human ?? tech}.`, { fg: t.strong, bold: true }, 2)
  y = para(c, r.x, y, r.w, `${tech}, ${crash.exit}.`, { fg: t.dim }, 2)
  y++
  f.kv(c, r.x, y++, r.w, 'Fix', [['update ', { fg: t.text }], [crashFix.mod, { fg: t.strong }], [` to ${crashFix.to}`, { fg: t.text }], ['   m', { fg: t.strong, bold: true }], [' does it', { fg: t.dim }]], 6)
  y++
  if (y + 3 >= bottom(r)) return
  f.rule(c, m.x, y, m.w, ['Last lines'], { titleSt: { fg: t.dim, bold: true }, right: [['l', { fg: t.strong, bold: true }], [' whole log', { fg: t.dim }]] })
  logTail(c, rect(r.x, y + 1, r.w, bottom(r) - y - 1), crashTail, { gutter: true, top: true })
}

// ── Log lines ─────────────────────────────────────────────────────────────

export function levelStyle(c: Ctx, l: LogLine): { glyph: string; gfg: string; msg: Style } {
  const { t } = c
  if (l.level === 'ERROR') return { glyph: '✗', gfg: t.error, msg: { fg: t.error } }
  if (l.level === 'WARN') return { glyph: '▲', gfg: t.warning, msg: { fg: t.text } }
  return { glyph: ' ', gfg: t.faint, msg: { fg: t.subtle } }
}

/**
 * Log lines filling `r` from the bottom up (the newest last), long ones
 * wrapped under their message. The level glyph sits in the column left of
 * `r` when `gutter`. Returns how many lines were hidden above.
 */
export function logTail(c: Ctx, r: Rect, log: LogLine[], o: { gutter?: boolean; q?: string; wrapAll?: boolean; mark?: number; top?: boolean } = {}) {
  const { buf, t } = c
  const timeW = 9
  // Wide: the thread gets its own quiet column.
  const threadW = r.w >= 100 ? 15 : 0
  const msgX = r.x + timeW + threadW
  const msgW = r.w - timeW - threadW
  // Lay out from the newest backwards until the room runs out.
  const rowsOut: { l: LogLine; i: number; text: string; first: boolean }[][] = []
  let used = 0
  let i = log.length - 1
  for (; i >= 0; i--) {
    const l = log[i]
    const parts = o.wrapAll === false ? [truncate(l.text, msgW)] : wrap(l.text, msgW).slice(0, 3)
    if (used + parts.length > r.h) break
    rowsOut.unshift(parts.map((p, k) => ({ l, i, text: p, first: k === 0 })))
    used += parts.length
  }
  // A log that fits starts at the top; a longer one follows its end.
  let y = o.top || i < 0 ? r.y : r.y + Math.max(0, r.h - used)
  for (const group of rowsOut) {
    for (const row of group) {
      const st = levelStyle(c, row.l)
      const on = o.mark === row.i
      if (on) buf.fill(rect(r.x - 2, y, r.w + 4, 1), { bg: t.selection })
      if (row.first) {
        buf.text(r.x, y, row.l.time, { fg: t.faint })
        if (threadW) buf.text(r.x + timeW, y, row.l.thread, { fg: t.faint }, threadW - 2)
        if (o.gutter) buf.text(r.x - 2, y, st.glyph, { fg: st.gfg })
      }
      const msg = on ? { ...st.msg, fg: row.l.level === 'INFO' ? t.strong : st.msg.fg } : st.msg
      buf.line(msgX, y, f.highlight(c, row.text, o.q ?? '', msg), {}, msgW)
      y++
    }
  }
  return i + 1
}

// ── World, mod, pack ──────────────────────────────────────────────────────

export function worldView(c: Ctx, m: Rect, inst: Instance, wd: World | undefined) {
  const { buf, t } = c
  if (!wd) {
    const r = mainFrame(c, m, ['Worlds'])
    para(c, r.x, r.y, r.w, 'No worlds yet. Play once and the worlds you make show up here, newest first.', { fg: t.dim })
    return
  }
  const facts = worldFacts[wd.name] ?? worldFacts.World
  const hard = wd.mode === 'Hardcore'
  const r = mainFrame(c, m, [wd.name], { right: [[wd.mode.toLowerCase(), { fg: hard ? t.warning : t.dim }]] })
  let y = r.y
  buf.text(r.x, y, `${wd.mode}  ·  ${facts.difficulty}  ·  ${wd.size}`, { fg: t.dim }, r.w)
  y += 2
  const lw = 13
  f.kv(c, r.x, y++, r.w, 'Last played', [[wd.lastPlayed, { fg: t.text }], [`  ·  in ${facts.version}`, { fg: t.dim }]], lw)
  f.kv(c, r.x, y++, r.w, 'Seed', [[facts.seed, { fg: t.text }]], lw)
  f.kv(c, r.x, y++, r.w, 'Backups', [[facts.backups, { fg: facts.backups === 'none' ? t.dim : t.text }]], lw)
  f.kv(c, r.x, y++, r.w, 'Folder', [[keepEnd(`${folder(inst.name)}/saves/${wd.name}`, r.w - lw), { fg: t.dim }]], lw)
  y++
  const note = hard
    ? 'Hardcore: one life. b backs it up before you play, in case.'
    : '↵ opens the game straight into this world.'
  para(c, r.x, y, r.w, note, { fg: t.muted }, 2)
}

export function modView(c: Ctx, m: Rect, inst: Instance, mod: InstalledMod | undefined) {
  const { buf, t, data } = c
  if (inst.loader === 'Vanilla') {
    const r = mainFrame(c, m, ['Mods'])
    para(c, r.x, r.y, r.w, `${inst.name} is vanilla, so it runs without mods. To play with mods, [n] makes a Fabric instance.`, { fg: t.dim })
    return
  }
  if (!mod) {
    const r = mainFrame(c, m, ['Mods'])
    para(c, r.x, r.y, r.w, 'No mods yet. [m] searches Modrinth for mods made for this version.', { fg: t.dim })
    return
  }
  const hit = hitFor(data, mod.name)
  const culprit = data.crash?.instance === inst.name && mod.name === crashFix.mod
  const stateR: Line = culprit
    ? [['▲ ', { fg: t.warning }], ['too old', { fg: t.warning }]]
    : mod.update
      ? [['↑ ', { fg: t.success }], [mod.update, { fg: t.success }]]
      : !mod.enabled
        ? [['○ ', { fg: t.faint }], ['off', { fg: t.dim }]]
        : [['on', { fg: t.dim }]]
  const r = mainFrame(c, m, [mod.name], { right: stateR })
  let y = r.y
  buf.text(r.x, y, `by ${hit.author}  ·  ${hit.category}  ·  ${hit.side}`, { fg: t.dim }, r.w)
  y += 2
  if (hit.description) y = para(c, r.x, y, r.w, hit.description, { fg: t.text }, 3) + 1
  const lw = 11
  const target = culprit ? crashFix.to : mod.update
  f.kv(c, r.x, y++, r.w, 'Installed', [[mod.version, { fg: t.text }], [mod.enabled ? '' : '  ·  turned off', { fg: t.dim }]], lw)
  if (target) f.kv(c, r.x, y++, r.w, culprit ? 'Fix' : 'Update', [[culprit ? 'update to ' : '', { fg: t.text }], [target, { fg: culprit ? t.warning : t.success }], [culprit ? '  ·  Sodium needs it' : '  ·  out 2w ago', { fg: t.dim }]], lw)
  f.kv(c, r.x, y++, r.w, 'Needs', [[needs[mod.name] ?? 'nothing', { fg: t.text }]], lw)
  f.kv(c, r.x, y++, r.w, 'Modrinth', [[`${hit.downloads} downloads`, { fg: t.text }], [`  ·  updated ${hit.updated}`, { fg: t.dim }]], lw)
  const notes = target ? changes[mod.name] : undefined
  if (notes && y + 3 < bottom(r)) {
    y++
    f.rule(c, m.x, y++, m.w, [`What’s new in ${target}`], { titleSt: { fg: t.dim, bold: true } })
    notes.forEach((n) => {
      if (y >= bottom(r)) return
      buf.text(r.x - 2, y, '·', { fg: t.faint })
      buf.text(r.x, y++, n, { fg: t.subtle }, r.w)
    })
  }
}

export function packView(c: Ctx, m: Rect, inst: Instance, pack: string | undefined, idx: number) {
  const { buf, t } = c
  if (!pack) {
    const r = mainFrame(c, m, ['Packs'])
    para(c, r.x, r.y, r.w, `No resource packs in ${inst.name}. Drop a .zip in its resourcepacks folder, or [m] finds some on Modrinth.`, { fg: t.dim })
    return
  }
  const facts = packFacts[pack] ?? { res: '16×', size: '—', about: '' }
  const r = mainFrame(c, m, [pack], { right: [['on', { fg: t.dim }]] })
  let y = r.y
  buf.text(r.x, y, `${facts.res}  ·  ${facts.size}  ·  ${ordinal(idx + 1)} of ${inst.packs.length}`, { fg: t.dim }, r.w)
  y += 2
  if (facts.about) y = para(c, r.x, y, r.w, facts.about, { fg: t.text }) + 1
  para(c, r.x, y, r.w, 'Higher packs win when two change the same texture.', { fg: t.muted }, 2)
}

const ordinal = (n: number) => `${n}${n === 1 ? 'st' : n === 2 ? 'nd' : n === 3 ? 'rd' : 'th'}`

// ── Account (the status line, focused) ───────────────────────────────────

export function accountView(c: Ctx, m: Rect) {
  const { buf, t, data } = c
  const a = data.account
  const expired = a.status === 'expired'
  const r = mainFrame(c, m, [a.name], {
    right: expired ? [['▲ ', { fg: t.warning }], ['sign-in expired', { fg: t.warning }]] : [['● ', { fg: t.success }], ['signed in', { fg: t.success }]],
  })
  let y = r.y
  buf.text(r.x, y, `${a.kind} account  ·  ${a.uuid.slice(0, 8)}…`, { fg: t.dim }, r.w)
  y += 2
  y = para(
    c,
    r.x,
    y,
    r.w,
    expired
      ? 'Microsoft wants you to sign in again before playing online. [↵] opens the sign-in page; it takes a minute.'
      : 'Signed in Sep 26. mctui renews the sign-in on its own; nothing to do.',
    { fg: t.text },
  )
  y++
  const others = data.accounts.filter((x) => x.name !== a.name)
  if (!others.length || y + 2 >= bottom(r)) return
  f.rule(c, m.x, y++, m.w, ['Other profiles'], { titleSt: { fg: t.dim, bold: true }, count: others.length })
  others.forEach((o) => {
    const st = o.status === 'expired' ? [['▲ ', { fg: t.warning }], ['expired', { fg: t.warning }]] : [[o.kind === 'Offline' ? 'offline' : 'signed in', { fg: t.dim }]]
    buf.text(r.x, y, o.name, { fg: t.text }, 14)
    buf.text(r.x + 14, y, o.kind, { fg: t.dim })
    buf.lineRight(rect(r.x, y, r.w, 1), st as Line)
    y++
  })
}

// ── First run ─────────────────────────────────────────────────────────────

export function welcomeView(c: Ctx, m: Rect) {
  const { t } = c
  const r = mainFrame(c, m, ['Welcome'])
  const y = para(c, r.x, r.y, r.w, 'No instances yet. Each keeps its own worlds and mods.', { fg: t.text })
  para(c, r.x, y, r.w, '[↵] [↵] [↵] makes one: latest release, Fabric, a name.', { fg: t.dim })
}

// ── Keys, menu, commands: they take the main view ────────────────────────

export function helpView(c: Ctx, m: Rect, context: string, keys: f.Hint[]) {
  const { buf, t } = c
  const r = mainFrame(c, m, ['Keys'], { focused: true, segs: [[[context, { fg: t.subtle }]]], right: [['esc', { fg: t.strong, bold: true }], [' close', { fg: t.dim }]] })
  const everywhere: f.Hint[] = [
    ['1-4', 'jump to a panel'],
    ['tab', 'next panel'],
    ['↑↓ j k', 'move'],
    ['/', 'filter the panel'],
    ['n', 'new instance'],
    [':', 'commands'],
    ['?', 'these keys'],
    ['q', 'quit'],
  ]
  const two = r.w >= 52
  const cw = two ? Math.floor((r.w - 3) / 2) : r.w
  const block = (x: number, y: number, title: string, list: f.Hint[]) => {
    buf.text(x, y, title, { fg: t.dim, bold: true })
    list.forEach(([k, d], i) => {
      if (y + 1 + i >= bottom(r)) return
      buf.text(x, y + 1 + i, k, { fg: t.strong, bold: true }, 8)
      buf.text(x + 8, y + 1 + i, d, { fg: t.text }, cw - 8)
    })
    return y + list.length + 2
  }
  if (two) {
    block(r.x, r.y + 1, context, keys)
    block(r.x + cw + 3, r.y + 1, 'Everywhere', everywhere)
  } else {
    const y = block(r.x, r.y + 1, context, keys)
    block(r.x, y, 'Everywhere', everywhere)
  }
  buf.region(m, 'close')
}

export function menuView(c: Ctx, m: Rect, subject: string, items: [string, string, boolean?][]) {
  const { buf, t } = c
  const r = mainFrame(c, m, [subject], { focused: true, segs: [[['actions', { fg: t.subtle }]]], right: [['esc', { fg: t.strong, bold: true }], [' close', { fg: t.dim }]] })
  const sel = c.osel(items.length)
  items.forEach(([label, key, danger], i) => {
    const y = r.y + i
    if (y >= bottom(r)) return
    const on = i === sel
    if (on) buf.fill(rect(m.x, y, m.w, 1), { bg: danger ? t.errorBg : t.selection })
    else if (c.hovered(`osel:${i}`)) buf.fill(rect(m.x, y, m.w, 1), { bg: t.surface })
    buf.text(r.x, y, label, danger ? { fg: t.error, bold: on } : on ? { fg: t.strong, bold: true } : { fg: t.text })
    buf.lineRight(rect(r.x, y, r.w, 1), [[key, { fg: danger ? t.error : on ? t.strong : t.dim, bold: true }]])
    buf.region(rect(m.x, y, m.w, 1), `osel:${i}`)
  })
}

export const COMMANDS: [string, string, string][] = [
  ['Play', 'Survival 1.21', '↵'],
  ['Play offline', 'Survival 1.21', 'o'],
  ['New instance', '', 'n'],
  ['Add mods', 'Survival 1.21', 'm'],
  ['Update all mods', '1 update', 'u'],
  ['Edit instance', 'Survival 1.21', 'e'],
  ['Open folder', 'Survival 1.21', 'f'],
  ['Accounts', 'Steve', ''],
  ['Settings', '', 's'],
  ['Copy last game log', '', ''],
  ['Quit', '', 'q'],
]

export function paletteView(c: Ctx, m: Rect) {
  const { buf, t, ui } = c
  const q = ui.oquery.toLowerCase()
  const hits = COMMANDS.filter(([n, ctx]) => `${n} ${ctx}`.toLowerCase().includes(q))
  const r = mainFrame(c, m, ['Commands'], { focused: true, count: q ? `${hits.length}/${COMMANDS.length}` : COMMANDS.length, right: [['esc', { fg: t.strong, bold: true }], [' close', { fg: t.dim }]] })
  const sel = c.osel(Math.max(1, hits.length))
  if (!hits.length) buf.text(r.x, r.y, 'No command matches.', { fg: t.faint })
  hits.forEach(([name, ctx, key], i) => {
    const y = r.y + i
    if (y >= bottom(r)) return
    const on = i === sel
    if (on) buf.fill(rect(m.x, y, m.w, 1), { bg: t.selection })
    const x = buf.line(r.x, y, f.highlight(c, name, ui.oquery, on ? { fg: t.strong, bold: true } : { fg: t.text }))
    if (ctx) buf.text(x + 2, y, ctx, { fg: t.dim }, right(r) - x - 6)
    if (key) buf.lineRight(rect(r.x, y, r.w, 1), [[key, { fg: on ? t.strong : t.dim, bold: true }]])
    buf.region(rect(m.x, y, m.w, 1), `osel:${i}`)
  })
}

/** The `:` prompt in the key line while the command palette is open. */
export function promptLine(c: Ctx, r: Rect) {
  const { buf, t, ui } = c
  buf.text(r.x + 2, r.y, ':', { fg: t.accent, bold: true })
  const end = buf.text(r.x + 4, r.y, ui.oquery, { fg: t.strong })
  if (!ui.oquery) buf.text(r.x + 4, r.y, 'type a command', { fg: t.muted })
  buf.cursor = { x: end, y: r.y }
  buf.lineRight(rect(r.x, r.y, r.w, 1), [['↵', { fg: t.strong, bold: true }], [' run   ', { fg: t.dim }], ['esc', { fg: t.strong, bold: true }], [' close', { fg: t.dim }]])
}

/** A delete confirmation, asked in the key line (vim's way, not a dialog). */
export function confirmLine(c: Ctx, r: Rect, question: string, detail: string) {
  const { buf, t } = c
  buf.fill(r, { bg: t.errorBg })
  let x = buf.text(r.x + 2, r.y, question, { fg: t.error, bold: true, bg: t.errorBg })
  x = buf.text(x + 2, r.y, detail, { fg: t.text, bg: t.errorBg }, right(r) - x - 24)
  const keys: Line = [['y', { fg: t.error, bold: true }], [' delete   ', { fg: t.text }], ['esc', { fg: t.strong, bold: true }], [' keep  ', { fg: t.text }]]
  const kx = buf.lineRight(r, keys, { bg: t.errorBg })
  buf.region(rect(kx, r.y, 8, 1), 'key:y')
  buf.region(rect(kx + 10, r.y, 10, 1), 'close')
  void x
}

