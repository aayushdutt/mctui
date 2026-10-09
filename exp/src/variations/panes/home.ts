// Home: instances, the selected instance's details and the Activity feed on
// one screen, lazygit style. Three focusable panels (tab cycles them).

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { type Line, type Rect, type Style, rect, cols, rows, right, bottom } from '../../term/buffer'
import * as f from './frame'
import { activity, javas, extra, type Activity } from './data'
import { overlays } from './overlays'

export function home(c: Ctx) {
  const { buf, data } = c
  const W = buf.cols
  const body = rect(1, 0, W - 2, buf.rows - 1)
  const wide = W >= 124
  const focus = c.panes(3)
  const tab = c.tabs(3)

  const topH = Math.min(body.h - 7, Math.max(15, Math.round(body.h * 0.4)))
  const [top, , bot] = rows(body, [topH, 1, '1fr'])
  const listW = wide ? 54 : W >= 96 ? 42 : 32
  const [left, , detailR] = cols(top, [listW, 2, '1fr'])
  const [actR, , sysR] = wide ? cols(bot, ['1fr', 2, 40]) : [bot, bot, null]

  // ── Instances (+ accounts under them when there's room) ──
  const insts = data.instances
  const q = f.query(c)
  const shown = insts.filter((i) => i.name.toLowerCase().includes(q.toLowerCase()))
  // The list keeps its selection while another panel has focus.
  const sel = c.sel(shown.length + 1, 0)
  const current: Instance | null = shown[sel] ?? null
  const curIdx = current ? shown.indexOf(current) : shown.length
  // Sized for every instance, so filtering never moves the panels below.
  const instH = 4 + insts.length
  const acctH = left.h - instH - 1
  const listR = acctH >= 5 ? rect(left.x, left.y, left.w, instH) : left
  instances(c, listR, shown, curIdx, focus === 0)
  if (acctH >= 5) accounts(c, rect(left.x, bottom(listR) + 1, left.w, acctH))

  // ── Details ──
  const detailSel = focus === 1 ? detailRows(current, tab) : 0
  const dsel = focus === 1 ? c.sel(Math.max(1, detailSel)) : -1
  details(c, detailR, current, tab, focus === 1, dsel)

  // ── Activity (+ accounts & Java when wide) ──
  const asel = focus === 2 ? c.sel(activity.length) : -1
  activityPanel(c, actR, asel, focus === 2)
  if (sysR) machine(c, sysR)

  const hints: [string, string][] =
    focus === 0
      ? [['↵', 'play'], ['n', 'new'], ['/', 'filter'], ['tab', 'panel'], ['o', 'offline'], ['m', 'mods'], ['x', 'menu'], [':', 'commands'], ['?', 'help']]
      : focus === 1
        ? [['←→', 'tab'], ['↵', 'open'], ['tab', 'panel'], ['m', 'mods'], [':', 'commands'], ['?', 'help']]
        : [['↵', 'open'], ['c', 'clear'], ['tab', 'panel'], [':', 'commands'], ['?', 'help']]
  const mode: f.Mode = f.typing(c) ? { label: 'FILTER' } : { label: 'HOME' }
  f.statusBar(c, mode, [], hints)

  // The row the instance menu hangs off.
  const menuY = listR.y + 2 + Math.max(0, curIdx)
  overlays(c, { screen: 'Home', anchor: rect(listR.x + 4, Math.min(menuY + 1, bottom(listR)), 30, 1), subject: current?.name ?? insts[0].name })
}

function instances(c: Ctx, r: Rect, shown: Instance[], sel: number, focused: boolean) {
  const { buf, t, data } = c
  const total = data.instances.length
  const q = f.query(c)
  const inner = f.panel(c, r, 'Instances', {
    focused,
    count: q ? `${shown.length}/${total}` : total,
    region: 'focus:0',
    foot: [['66h 19m played', { fg: t.muted }]],
  })
  let y = inner.y
  if (f.typing(c) || q) {
    // The filter lives in the title rule: ` / sur▏      `, one row.
    const fw = Math.min(18, r.w - 18)
    const fr = rect(right(r) - 2 - fw, r.y, fw, 1)
    buf.fill(fr, { bg: f.typing(c) ? t.selection : t.surface })
    buf.text(fr.x + 1, r.y, '/', { fg: t.accent, bold: true })
    const end = buf.text(fr.x + 3, r.y, q, { fg: t.strong }, fw - 4)
    if (f.typing(c)) buf.cursor = { x: end, y: r.y }
    buf.region(fr, 'query')
  }
  const iw = inner.w
  const spec: f.Col[] =
    iw >= 50
      ? [{ label: '  Name', w: 'fr' }, { label: 'Version', w: 9 }, { label: 'Loader', w: 7 }, { label: 'Played', w: 9, align: 'right' }]
      : iw >= 38
        ? [{ label: '  Name', w: 'fr' }, { label: 'Version', w: 9 }, { label: 'Played', w: 9, align: 'right' }]
        : [{ label: '  Name', w: 'fr' }, { label: 'Played', w: 9, align: 'right' }]
  const placed = f.place(inner, spec)
  f.header(c, y, placed)
  y++
  const fit = bottom(inner) - y
  const items = shown.length + 1
  const start = f.scrollStart(items, Math.max(0, sel), fit)
  for (let i = start; i < Math.min(items, start + fit); i++) {
    const ry = y + (i - start)
    const row = rect(r.x, ry, r.w, 1)
    const key = focused ? `sel:${i}` : 'focus:0'
    const state = f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, key)
    if (i === shown.length) {
      buf.text(inner.x, ry, '+ New instance', state ? { fg: t.strong, bold: state === 'focus' } : { fg: t.dim })
      buf.lineRight(rect(inner.x, ry, inner.w, 1), [['n', { fg: t.muted }]])
      continue
    }
    const inst = shown[i]
    const st = f.rowText(c, state)
    const glyph: Line = inst.trouble ? [['▲ ', { fg: t.warning }]] : inst.fresh ? [['○ ', { fg: t.faint }]] : ['  ']
    placed.forEach((col) => {
      if (col.label === '  Name') f.cell(c, col, ry, [...glyph, ...f.highlight(c, inst.name, f.query(c), st)])
      else if (col.label === 'Version') f.cell(c, col, ry, [[inst.mc, { fg: state ? t.text : t.subtle }]])
      else if (col.label === 'Loader') f.cell(c, col, ry, [[inst.loader, { fg: t.dim }]])
      else if (col.label === 'Played')
        f.cell(c, col, ry, inst.fresh ? [['new', { fg: t.secondary }]] : [[inst.lastPlayed ?? '—', { fg: t.dim }]])
    })
  }
  if (!shown.length && q) buf.text(inner.x + 2, y + 1, `Nothing matches “${q}”.`, { fg: t.faint })
}

function detailRows(inst: Instance | null, tab: number) {
  if (!inst) return 0
  return [inst.worlds.length, inst.mods.length, inst.packs.length][tab]
}

function details(c: Ctx, r: Rect, inst: Instance | null, tab: number, focused: boolean, sel: number) {
  const { buf, t } = c
  if (!inst) {
    const inner = f.panel(c, r, 'New instance', { region: 'focus:1' })
    buf.text(inner.x, inner.y + 1, 'A version, a loader and a name, all on one screen.', { fg: t.dim }, inner.w)
    buf.text(inner.x, inner.y + 2, 'Mods and worlds can come later.', { fg: t.dim }, inner.w)
    f.button(c, inner.x, inner.y + 4, '+ Create', 'n', 'primary', 'screen:wizard')
    return
  }
  const status: Line = inst.trouble
    ? [['▲ Crashed', { fg: t.warning }]]
    : inst.fresh
      ? [['○ Never played', { fg: t.dim }]]
      : [['● Ready', { fg: t.success }]]
  const inner = f.panel(c, r, [[inst.name, focused ? {} : { fg: t.strong }]], { focused, right: status, region: 'focus:1' })
  const iw = inner.w

  // Facts in a grid, filled column by column.
  const loader = inst.loader === 'Vanilla' ? 'Vanilla' : `${inst.loader} ${inst.loaderVersion}`
  const facts: [string, string][] = [
    ['Version', inst.mc],
    ['Loader', loader],
    ['Java', inst.java.replace(/^Java /, '')],
    ['Played', inst.playtime],
    ['Last', inst.lastPlayed ?? 'never'],
    ['Memory', inst.memory],
  ]
  const more = extra[inst.name]
  if (iw >= 78 && more) facts.push(['Created', more.created], ['Launches', more.launches], ['On disk', more.disk])
  const ncol = facts.length > 6 ? 3 : 2
  const perCol = Math.ceil(facts.length / ncol)
  const colWs = ncol === 3 ? [Math.floor(iw * 0.4), Math.floor(iw * 0.32)] : [Math.max(25, Math.floor(iw * 0.56))]
  let cx = inner.x
  for (let ci = 0; ci < ncol; ci++) {
    const chunk = facts.slice(ci * perCol, ci * perCol + perCol)
    const last = ci === ncol - 1
    const cw = last ? right(inner) - cx : colWs[ci]
    const lw = Math.max(...chunk.map(([l]) => l.length)) + 2
    chunk.forEach(([label, value], j) => {
      buf.text(cx, inner.y + j, label, { fg: t.dim }, lw)
      buf.text(cx + lw, inner.y + j, value, { fg: t.text }, cw - lw - (last ? 0 : 1))
    })
    cx += cw
  }
  let y = inner.y + perCol + 1

  if (inst.trouble) {
    const row = rect(inner.x, y, inner.w, 1)
    buf.fill(row, { bg: t.warningBg })
    buf.line(inner.x + 1, y, [['▲ ', { fg: t.warning }], ['Crashed on its last launch, exit code 1.', { fg: t.text }], ['  l', { fg: t.accent, bold: true }], [' log', { fg: t.dim }]], {}, inner.w - 2)
    y += 2
  }

  let bx = inner.x
  const btns: [string, string, 'primary' | 'secondary', string][] = [
    ['▶ Play', '↵', 'primary', 'key:↵'],
    ['Offline', 'o', 'secondary', 'offline'],
    ['Mods', 'm', 'secondary', 'screen:mods'],
    ['Edit', 'e', 'secondary', 'edit'],
    ['Folder', 'f', 'secondary', 'folder'],
  ]
  for (const [label, key, kind, region] of btns) {
    const need = label.length + key.length + 4
    if (bx + need > right(inner)) break
    bx = right(f.button(c, bx, y, label, key, kind, region)) + 2
  }
  y += 2
  if (y >= bottom(inner)) return

  // Worlds / Mods / Packs, tabbed in a rule.
  f.tabRule(c, rect(r.x, y, r.w, 1), [['Worlds', inst.worlds.length], ['Mods', inst.mods.length], ['Packs', inst.packs.length]], tab, focused)
  y++
  const list = rect(inner.x, y, inner.w, bottom(inner) - y)
  const empty = (s: string) => buf.text(list.x + 2, list.y + 1, s, { fg: t.faint }, list.w - 2)
  const rowAt = (i: number) => {
    const ry = list.y + 1 + i
    const row = rect(r.x, ry, r.w, 1)
    const state = f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${i}` : 'focus:1')
    return { ry, state }
  }
  if (tab === 0) {
    if (!inst.worlds.length) return empty('No worlds yet. They show up after you play.')
    const spec: f.Col[] =
      iw >= 60
        ? [{ label: 'World', w: 'fr' }, { label: 'Mode', w: 9 }, { label: 'Size', w: 7, align: 'right' }, { label: 'Last played', w: 11, align: 'right' }]
        : iw >= 48
          ? [{ label: 'World', w: 'fr' }, { label: 'Mode', w: 9 }, { label: 'Size', w: 7, align: 'right' }, { label: 'Played', w: 7, align: 'right' }]
          : [{ label: 'World', w: 'fr' }, { label: 'Size', w: 7, align: 'right' }, { label: 'Played', w: 7, align: 'right' }]
    const p = f.place(list, spec)
    f.header(c, list.y, p)
    inst.worlds.slice(0, list.h - 1).forEach((wd, i) => {
      const { ry, state } = rowAt(i)
      p.forEach((col) => {
        const v =
          col.label === 'World'
            ? [wd.name, f.rowText(c, state)]
            : col.label === 'Mode'
              ? [wd.mode, { fg: t.dim }]
              : col.label === 'Size'
                ? [wd.size, { fg: t.subtle }]
                : [wd.lastPlayed, { fg: t.dim }]
        f.cell(c, col, ry, [v as [string, Style]])
      })
    })
  } else if (tab === 1) {
    if (!inst.mods.length)
      return empty(inst.loader === 'Vanilla' ? 'Vanilla runs no mods. Make a Fabric instance for those.' : 'No mods yet. Press m to add some.')
    const spec: f.Col[] = [{ label: 'Mod', w: 'fr' }, { label: 'Version', w: 9 }, { label: 'Status', w: 9 }]
    const p = f.place(list, spec)
    f.header(c, list.y, p)
    inst.mods.slice(0, list.h - 1).forEach((m, i) => {
      const { ry, state } = rowAt(i)
      f.cell(c, p[0], ry, [[m.name, m.enabled ? f.rowText(c, state) : { fg: t.dim }]])
      f.cell(c, p[1], ry, [[m.version, { fg: t.subtle }]])
      f.cell(
        c,
        p[2],
        ry,
        m.update ? [['↑ ' + m.update, { fg: t.secondary }]] : m.enabled ? [['● on', { fg: t.success }]] : [['○ off', { fg: t.dim }]],
      )
    })
  } else {
    if (!inst.packs.length) return empty('No resource packs.')
    const spec: f.Col[] = [{ label: '#', w: 2, align: 'right' }, { label: 'Pack', w: 'fr' }, { label: 'Status', w: 9 }]
    const p = f.place(list, spec)
    f.header(c, list.y, p)
    inst.packs.slice(0, list.h - 1).forEach((pk, i) => {
      const { ry, state } = rowAt(i)
      f.cell(c, p[0], ry, [[String(i + 1), { fg: t.muted }]])
      f.cell(c, p[1], ry, [[pk, f.rowText(c, state)]])
      f.cell(c, p[2], ry, [['● on', { fg: t.success }]])
    })
  }
}

export function activityGlyph(c: Ctx, a: Activity): Line {
  const { t } = c
  switch (a.kind) {
    case 'download':
      return [[c.spinner(), { fg: t.accent }]]
    case 'queued':
      return [['○', { fg: t.muted }]]
    case 'ok':
      return [['✓', { fg: t.success }]]
    case 'warn':
      return [['▲', { fg: t.warning }]]
    case 'error':
      return [['✗', { fg: t.error }]]
    default:
      return [['•', { fg: t.dim }]]
  }
}

function activityPanel(c: Ctx, r: Rect, sel: number, focused: boolean) {
  const { buf, t } = c
  const downloading = activity.filter((a) => a.kind === 'download').length
  const queued = activity.filter((a) => a.kind === 'queued').length
  const inner = f.panel(c, r, 'Activity', {
    focused,
    region: 'focus:2',
    right: [[`${downloading} downloading`, { fg: t.subtle }], ['  ·  ', { fg: t.faint }], [`${queued} queued`, { fg: t.dim }]],
  })
  const spec: f.Col[] = inner.w >= 70 ? [{ label: '', w: 6 }, { label: '', w: 'fr' }, { label: '', w: 16 }] : [{ label: '', w: 6 }, { label: '', w: 'fr' }]
  const p = f.place(inner, spec)
  const fit = inner.h
  const start = sel < 0 ? 0 : f.scrollStart(activity.length, sel, fit)
  activity.slice(start, start + fit).forEach((a, j) => {
    const i = start + j
    const y = inner.y + j
    const row = rect(r.x, y, r.w, 1)
    const state = sel < 0 ? null : f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${i}` : 'focus:2')
    f.cell(c, p[0], y, [[a.time, { fg: t.muted }]])
    const what = p[1]
    const wEnd = what.x + what.cw
    const gx = buf.line(what.x, y, activityGlyph(c, a))
    const tx = gx + 1
    const avail = wEnd - tx
    const textSt = state ? f.rowText(c, state) : a.kind === 'queued' ? { fg: t.dim } : { fg: t.text }
    if (a.kind === 'download' && a.progress !== undefined) {
      const nameEnd = buf.text(tx, y, a.text, textSt, avail)
      const tail = `  ${Math.round(a.progress * 100)}%  ${a.detail ?? ''}`
      const barW = Math.min(24, wEnd - nameEnd - 2 - tail.length)
      if (barW >= 6) {
        const bx = f.bar(c, nameEnd + 2, y, barW, a.progress)
        buf.line(bx, y, [[`  ${Math.round(a.progress * 100)}%`, { fg: t.subtle }], [`  ${a.detail ?? ''}`, { fg: t.dim }]], {}, wEnd - bx)
      }
    } else if (a.kind === 'queued') {
      const end = buf.text(tx, y, a.text, textSt, avail)
      buf.line(end, y, [['  queued', { fg: t.muted }], [`  ${a.detail ?? ''}`, { fg: t.faint }]], {}, wEnd - end)
    } else buf.text(tx, y, a.text, textSt, avail)
    if (p[2]) f.cell(c, p[2], y, [[a.source || 'mctui', { fg: a.source ? t.dim : t.faint }]])
  })
  if (activity.length > start + fit) {
    const more = activity.length - start - fit
    buf.lineRight(rect(r.x, bottom(r) - 1, r.w - 2, 1), [[` ↓ ${more} more `, { fg: t.muted }]])
  }
}

function accounts(c: Ctx, r: Rect) {
  const { buf, t, data } = c
  const inner = f.panel(c, r, 'Accounts', {
    count: data.accounts.length,
    region: 'screen:accounts',
    right: [['a', { fg: t.accent, bold: true }], [' manage', { fg: t.dim }]],
  })
  const p = f.place(inner, [{ label: '', w: 'fr' }, { label: '', w: 9 }, { label: '', w: 7, align: 'right' }])
  data.accounts.forEach((a, i) => {
    const y = inner.y + i
    if (y >= bottom(inner)) return
    const [glyph, fg, state] =
      a.status === 'active' ? ['●', t.success, 'active'] : a.status === 'expired' ? ['▲', t.warning, 'expired'] : ['○', t.muted, 'offline']
    const row = rect(r.x, y, r.w, 1)
    if (c.hovered(`acct:${i}`)) f.band(c, row, 'hover')
    f.cell(c, p[0], y, [[glyph + ' ', { fg }], [a.name, { fg: a.status === 'active' ? t.strong : t.text }]])
    f.cell(c, p[1], y, [[a.kind, { fg: t.dim }]])
    f.cell(c, p[2], y, [[state, { fg: a.status === 'expired' ? t.warning : t.dim }]])
    buf.region(row, `acct:${i}`)
  })
  const ay = inner.y + data.accounts.length
  if (ay < bottom(inner)) buf.text(inner.x, ay, '+ Add account', { fg: t.faint })
}

/** Wide only: the Java runtimes and disk use beside the Activity feed. */
function machine(c: Ctx, r: Rect) {
  const { t } = c
  const inner = f.panel(c, r, 'Java', { count: javas.length, right: [['automatic', { fg: t.dim }]] })
  const p = f.place(inner, [{ label: '', w: 'fr' }, { label: '', w: 13 }, { label: '', w: 11, align: 'right' }])
  let y = inner.y
  javas.forEach((j) => {
    if (y >= bottom(inner)) return
    f.cell(c, p[0], y, [[j.name, { fg: t.text }]])
    f.cell(c, p[1], y, [[j.detail, { fg: t.dim }]])
    f.cell(c, p[2], y, [[`${j.used} inst.`, { fg: t.muted }]])
    y++
  })
  y++
  if (y + 2 >= bottom(inner)) return
  f.subRule(c, rect(r.x, y, r.w, 1), 'Disk', '1.9 GB')
  y++
  const disk: [string, string, number][] = [
    ['Worlds', '1.06 GB', 0.56],
    ['Game files', '512 MB', 0.27],
    ['Java runtimes', '298 MB', 0.16],
    ['Mods & packs', '41 MB', 0.02],
  ]
  const pd = f.place(inner, [{ label: '', w: 14 }, { label: '', w: 'fr' }, { label: '', w: 7, align: 'right' }])
  disk.forEach(([label, size, share]) => {
    if (y >= bottom(inner)) return
    f.cell(c, pd[0], y, [[label, { fg: t.dim }]])
    f.bar(c, pd[1].x, y, pd[1].cw, share, t.secondary)
    f.cell(c, pd[2], y, [[size, { fg: t.subtle }]])
    y++
  })
}
