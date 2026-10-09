// Refined mods: the instance's installed mods first, grouped by what needs
// you (to fix, to update, switched off) over the ones that are fine. `/`
// turns the same list into a Modrinth search; esc comes back. The sheet on
// the right follows the selection, as on home.

import type { Ctx } from '../../kit'
import type { InstalledMod, Instance, ModHit } from '../../data/mock'
import { type Line, type Rect, rect, right, bottom, width, wrap, lineWidth } from '../../term/buffer'
import { frame, header, footer, statusLine, helpSheet, ask, type Frame } from './frame'
import { affordance, crashNeeds, filter, fitParts, label, lit, type Hint } from './bits'

type Mark = 'fix' | 'update' | 'queued' | 'off' | 'ok'

interface Row {
  mod: InstalledMod
  mark: Mark
  note: Line
}

function rowsFor(c: Ctx, inst: Instance): Row[][] {
  const { t, data } = c
  const blame = data.crash?.instance === inst.name ? crashNeeds(data.crash.reason, inst.mods) : null
  const task = data.tasks.find((k) => k.instance === inst.name)
  const rows = inst.mods.map((mod): Row => {
    if (blame?.mod === mod.name) return { mod, mark: 'fix', note: [[`needs ${blame.needs}+`, { fg: t.error, bold: true }]] }
    if (mod.update && task?.label.includes(mod.name))
      return { mod, mark: 'queued', note: [[`↑ ${mod.update}`, { fg: t.dim }], ['  queued', { fg: t.muted }]] }
    if (mod.update) return { mod, mark: 'update', note: [[`↑ ${mod.update}`, { fg: t.accent }]] }
    if (!mod.enabled) return { mod, mark: 'off', note: [['off', { fg: t.muted }]] }
    return { mod, mark: 'ok', note: [] }
  })
  const rank: Record<Mark, number> = { fix: 0, update: 1, queued: 1, ok: 2, off: 3 }
  const groups: Row[][] = [[], [], [], []]
  rows.forEach((r) => groups[rank[r.mark]].push(r))
  return groups.filter((g) => g.length)
}

export function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  const inst = data.instances[0]
  header(c, f, inst ? [inst.name, 'Mods'] : ['Mods'])
  c.panes(1)
  if (!inst) {
    buf.text(f.x, f.body.y, 'No instance yet.', { fg: t.text })
    buf.line(f.x, f.body.y + 1, [['n', { fg: t.strong, bold: true }], [' makes one; its mods live here.', { fg: t.dim }]])
    footer(c, f, [['n', 'new instance'], ['esc', 'back']])
    return
  }
  const searching = ui.typing || !!ui.query
  if (searching) search(c, f, inst)
  else installed(c, f, inst)
  statusLine(c, f)
  if (ui.overlay === 'help')
    helpSheet(c, f, 'Mods · keys', [
      ['Move', [['↑↓  j k', 'move'], ['/', 'search Modrinth'], ['esc', 'back']]],
      ['Installed', [['u', 'update'], ['U', 'update all'], ['space', 'switch on, off'], ['d', 'remove']]],
      ['Modrinth', [['↵', 'install'], ['o', 'open page'], ['v', 'pick a version']]],
    ])
}

function installed(c: Ctx, f: Frame, inst: Instance) {
  const { buf, t, ui } = c
  const groups = rowsFor(c, inst)
  const flat = groups.flat()
  const n = flat.length
  const sel = c.sel(Math.max(1, n))
  const nameW = Math.max(12, ...flat.map((r) => width(r.mod.name)))
  const verW = Math.max(7, ...flat.map((r) => width(r.mod.version)))
  const noteW = Math.max(6, ...flat.map((r) => lineWidth(r.note)))
  const counts: Line = []
  const add = (k: number, s: string, fg: string) => {
    if (!k) return
    if (counts.length) counts.push(['  ·  ', { fg: t.faint }])
    counts.push([s, { fg }])
  }
  add(flat.filter((r) => r.mark === 'fix').length, '1 to fix', t.error)
  const ups = flat.filter((r) => r.mark === 'update' || r.mark === 'queued').length
  add(ups, `${ups} update${ups === 1 ? '' : 's'}`, t.accent)
  const offs = flat.filter((r) => r.mark === 'off').length
  add(offs, `${offs} off`, t.muted)
  const labelW = 'INSTALLED'.length + 2 + String(n).length + 4 + lineWidth(counts)
  const listW = Math.min(Math.max(nameW + 2 + verW + 2 + noteW, labelW, 24), Math.floor(f.w * 0.55))
  const X = f.x
  const px = X + listW + 6
  const pv = rect(px, f.body.y, Math.min(64, right(f.body) - px), f.body.h)

  // Label: count, and what needs you.
  label(c, X, f.body.y, 'Installed', n)
  buf.lineRight(rect(X, f.body.y, listW, 1), counts)

  if (!n) {
    buf.text(X, f.body.y + 1, 'None yet.', { fg: t.faint })
    buf.line(X, f.body.y + 3, [['/', { fg: t.strong, bold: true }], [' searches Modrinth', { fg: t.muted }]])
  }

  // Groups, a blank row apart; the selection runs straight through.
  let y = f.body.y + 1
  let i = 0
  groups.forEach((g, gi) => {
    if (gi) y++
    g.forEach((r) => {
      const on = sel === i
      const hov = c.hovered(`sel:${i}`)
      if (on) buf.fill(rect(X - 2, y, listW + 4, 1), { bg: t.selection })
      else if (hov) buf.fill(rect(X - 2, y, listW + 4, 1), { bg: t.surface })
      const off = r.mark === 'off'
      buf.text(X, y, r.mod.name, on ? { fg: t.strong, bold: true } : { fg: off ? t.dim : t.text }, nameW)
      buf.text(X + nameW + 2, y, r.mod.version, { fg: off ? t.muted : on ? t.subtle : t.dim }, verW)
      buf.lineRight(rect(X, y, listW, 1), r.note)
      buf.region(rect(X - 2, y, listW + 4, 1), `sel:${i}`)
      y++
      i++
    })
  })

  // The list's last row: where more come from.
  if (n) {
    const ay = y + 1
    const hov = c.hovered('query')
    if (hov) buf.fill(rect(X - 2, ay, listW + 4, 1), { bg: t.surface })
    buf.text(X, ay, '+ Add from Modrinth', { fg: hov ? t.text : t.dim })
    buf.lineRight(rect(X, ay, listW, 1), [['/', { fg: t.faint }]])
    buf.region(rect(X - 2, ay, listW + 4, 1), 'query')
  }

  const cur = flat[sel]
  if (cur) modSheet(c, pv, inst, cur)

  const keys: Hint[] = []
  if (cur?.mark === 'update' || cur?.mark === 'fix') keys.push(['u', 'update'])
  if (cur) keys.push(['space', cur.mark === 'off' ? 'switch on' : 'switch off'], ['d', 'remove'])
  keys.push(['/', 'add from Modrinth'], ['esc', 'back'])
  if (ui.overlay === 'confirm' && cur) {
    ask(c, f, [['Remove ', { fg: t.text }], [cur.mod.name, { fg: t.strong, bold: true }], [` from ${inst.name}?`, { fg: t.text }]], [
      ['y', 'remove', 'close'],
      ['esc', 'keep', 'close'],
    ])
  }
  footer(c, f, keys)
}

function modSheet(c: Ctx, r: Rect, inst: Instance, row: Row) {
  const { buf, t, data } = c
  const { mod, mark } = row
  const hit = data.modHits.find((h) => h.name === mod.name)
  const aff: Line | null =
    mark === 'update' || mark === 'fix' ? affordance(c, 'u', 'update') : mark === 'off' ? affordance(c, 'space', 'switch on') : null
  const aw = aff ? lineWidth(aff) : 0
  if (aff) buf.lineRight(rect(r.x, r.y, r.w, 1), aff)
  buf.text(r.x, r.y, mod.name, { fg: t.strong, bold: true }, r.w - aw - 2)
  // `0.6.0  →  0.6.5 available`: the version gives way first when narrow.
  const needs = crashNeeds(data.crash?.reason, inst.mods)?.needs ?? 'newer'
  const [joint, msg]: [string, Line] =
    mark === 'fix'
      ? ['  ·  ', [[`${needer(data.crash?.reason) ?? inst.name} needs ${needs}+`, { fg: t.error }]]]
      : mark === 'update'
        ? ['  →  ', [[`${mod.update} available`, { fg: t.accent }]]]
        : mark === 'queued'
          ? ['  →  ', [[`${mod.update}, queued`, { fg: t.dim }]]]
          : mark === 'off'
            ? ['  ·  ', [['switched off, not loaded', { fg: t.muted }]]]
            : ['  ·  ', [['up to date', { fg: t.dim }]]]
  const full: Line = [[mod.version, { fg: t.dim }], [joint, { fg: t.faint }], ...msg]
  buf.line(r.x, r.y + 1, lineWidth(full) <= r.w ? full : msg, {}, r.w)
  let y = r.y + 3
  if (hit) y = hitBody(c, r, hit, y)
  else buf.text(r.x, y, 'Not from Modrinth: added as a file.', { fg: t.faint }, r.w)
}

/** The part of a sheet a Modrinth project fills: who, what, how popular. */
function hitBody(c: Ctx, r: Rect, hit: ModHit, y: number): number {
  const { buf, t } = c
  buf.line(
    r.x,
    y++,
    fitParts(
      c,
      [
        { line: [['by ', { fg: t.muted }], [hit.author, { fg: t.text }]], prio: 0 },
        { line: [[hit.category, { fg: t.dim }]], prio: 1 },
        { line: [[hit.side, { fg: t.dim }]], prio: 2 },
      ],
      r.w,
    ),
  )
  y++
  const lines = wrap(hit.description, r.w)
  const room = Math.max(1, Math.min(4, bottom(r) - y - 2))
  lines.slice(0, room).forEach((l, i) => {
    const last = i === room - 1 && lines.length > room
    buf.text(r.x, y++, last ? l.replace(/\s*\S*$/, ' …') : l, { fg: t.text }, r.w)
  })
  y++
  if (y < bottom(r))
    buf.line(
      r.x,
      y++,
      fitParts(
        c,
        [
          { line: [[hit.downloads, { fg: t.text }], [' downloads', { fg: t.muted }]], prio: 0 },
          { line: [['updated ', { fg: t.muted }], [hit.updated, { fg: t.text }]], prio: 1 },
        ],
        r.w,
      ),
    )
  return y
}

function search(c: Ctx, f: Frame, inst: Instance) {
  const { buf, t, data, ui } = c
  const hits = filter(data.modHits, ui.query, (h) => h.name)
  const n = hits.length
  const sel = c.sel(Math.max(1, n))
  const X = f.x
  const nameW = Math.max(12, ...data.modHits.map((h) => width(h.name)))
  const authW = f.w >= 88 ? Math.max(8, ...data.modHits.map((h) => width(h.author))) : 0
  const dlW = Math.max(6, ...data.modHits.map((h) => width(h.downloads)))
  const listW = Math.min(nameW + (authW ? 2 + authW : 0) + 2 + dlW + 3, Math.floor(f.w * 0.58))
  const px = X + listW + 6
  const pv = rect(px, f.body.y, Math.min(64, right(f.body) - px), f.body.h)

  // The label row is the search field.
  const ly = f.body.y
  const qx = buf.text(X, ly, '/', { fg: t.accent, bold: true }) + 1
  const end = buf.text(qx, ly, ui.query, { fg: t.strong }, listW - 16)
  if (!ui.query) buf.text(qx, ly, 'Search Modrinth', { fg: t.faint })
  if (ui.typing) buf.cursor = { x: end, y: ly }
  const count: Line = ui.query
    ? [[`${n}`, { fg: t.text }], [` of ${data.modTotal}`, { fg: t.muted }]]
    : listW >= 44
      ? [['popular for ', { fg: t.muted }], [`${inst.mc} ${inst.loader}`, { fg: t.dim }]]
      : []
  buf.lineRight(rect(X, ly, listW, 1), count)
  buf.region(rect(X, ly, listW, 1), 'query')

  const top = ly + 1
  const fit = Math.max(1, bottom(f.body) - top)
  const first = Math.max(0, Math.min(sel - fit + 1, n - fit, sel))
  hits.slice(first, first + fit).forEach(({ item: h, at }, j) => {
    const i = first + j
    const y = top + j
    const on = sel === i
    const hov = c.hovered(`sel:${i}`)
    if (on) buf.fill(rect(X - 2, y, listW + 4, 1), { bg: t.selection })
    else if (hov) buf.fill(rect(X - 2, y, listW + 4, 1), { bg: t.surface })
    lit(c, X, y, h.name, at, on ? { fg: t.strong, bold: true } : { fg: t.text }, nameW)
    if (authW) buf.text(X + nameW + 2, y, h.author, { fg: on ? t.subtle : t.dim }, authW)
    buf.lineRight(rect(X, y, listW - 3, 1), [[h.downloads.padStart(dlW), { fg: on ? t.subtle : t.dim }]])
    if (h.installed) buf.text(X + listW - 1, y, '✓', { fg: t.success })
    buf.region(rect(X - 2, y, listW + 4, 1), `sel:${i}`)
  })
  if (!n) buf.text(X, top, `Nothing on Modrinth matches “${ui.query}”`, { fg: t.dim }, listW)

  const hit = hits[sel]?.item
  if (hit) {
    const aff: Line = hit.installed ? [['✓ installed', { fg: t.success }]] : affordance(c, '↵', 'install')
    buf.lineRight(rect(pv.x, pv.y, pv.w, 1), aff)
    buf.text(pv.x, pv.y, hit.name, { fg: t.strong, bold: true }, pv.w - lineWidth(aff) - 2)
    buf.line(pv.x, pv.y + 1, [['for ', { fg: t.muted }], [`Minecraft ${inst.mc} · ${inst.loader}`, { fg: t.dim }]], {}, pv.w)
    hitBody(c, pv, hit, pv.y + 3)
  }

  const keys: Hint[] = ui.typing
    ? [['↑↓', 'pick'], ['↵', 'done'], ['esc', 'installed']]
    : [['↵', hit?.installed ? 'reinstall' : 'install'], ['o', 'page'], ['/', 'edit search'], ['esc', 'installed']]
  footer(c, f, keys)
}

/** Who a crash says needs the newer mod: `Sodium 0.6.0`. */
function needer(reason: string | undefined): string | null {
  return reason?.match(/—\s*(.+?) needs /)?.[1] ?? null
}
