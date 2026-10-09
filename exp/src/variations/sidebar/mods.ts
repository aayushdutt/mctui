// Mods: the instance page's mods view. What's installed comes first, with
// only the states worth reading (update, disabled, broken). `/` searches
// Modrinth in place: installed matches stay on top, new finds below, and the
// selected one's details sit at the foot of the page. The sidebar stays.

import type { Ctx } from '../../kit'
import type { ModHit } from '../../data/mock'
import { type Line, rect, right, bottom, wrap, width, truncate } from '../../term/buffer'
import * as p from './parts'
import { modStates, type ModState } from './data'
import { helpPage } from './help'

type Row = { kind: 'installed'; s: ModState; hit?: ModHit } | { kind: 'hit'; hit: ModHit }

const matches = (q: string, ...fields: (string | undefined)[]) => !q || fields.some((f) => f?.toLowerCase().includes(q.toLowerCase()))

export function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const L = p.layout(c)
  const idx = 0
  const inst = data.instances[idx] ?? null
  const focus = c.panes(2)
  p.sidebar(c, L, { sel: inst ? idx : data.instances.length, focused: focus === 1, region: () => 'screen:home' })
  p.account(c, L)
  if (ui.overlay && ui.overlay !== 'confirm') return helpPage(c, L)
  const P = L.page
  if (!inst) {
    p.title(c, P, ['Mods'], [['Mods belong to an instance. Make one first: n.', { fg: t.dim }]])
    return p.footer(c, L, [['n', 'new instance'], ['?', 'help']])
  }

  const q = ui.overlay ? '' : ui.query
  const states = modStates(c, inst.name, inst.mods)
  const hitFor = (name: string) => data.modHits.find((h) => h.name === name)
  const installed: Row[] = states
    .map((s): Row => ({ kind: 'installed', s, hit: hitFor(s.mod.name) }))
    .filter((r) => r.kind === 'installed' && matches(q, r.s.mod.name, r.hit?.category, r.hit?.description))
  const names = new Set(inst.mods.map((m) => m.name))
  const found: Row[] = q
    ? data.modHits.filter((h) => !names.has(h.name) && matches(q, h.name, h.category, h.description)).map((hit) => ({ kind: 'hit', hit }))
    : []
  const rows = [...installed, ...found]
  const sel = c.sel(Math.max(1, rows.length), 0)
  const cur = rows[sel]

  const updates = states.filter((s) => s.note?.tone === 'update').length
  p.title(c, P, [inst.name, [' › ', { fg: t.faint, bold: false }], ['mods', { fg: t.text, bold: false }]], [
    [`${inst.mc} · ${p.loaderName(inst)}`, { fg: t.dim }],
    [` · ${inst.mods.length} installed`, { fg: t.muted }],
    ...(updates ? ([[` · ${updates} update${updates > 1 ? 's' : ''}`, { fg: t.muted }]] as Line) : []),
  ])

  // The search row: quiet until `/`, then it's where you type.
  const sy = P.y + 3
  const field = p.pageRow(P, sy)
  buf.fill(field, { bg: ui.typing ? t.selection : c.hovered('query') ? t.surfaceHi : t.surface })
  buf.text(P.x, sy, '/', ui.typing ? { fg: t.accent, bold: true } : { fg: t.muted, bold: true })
  const qx = P.x + 2
  const end = q ? buf.text(qx, sy, q, { fg: t.strong }, P.w - 16) : (buf.text(qx, sy, 'search Modrinth', { fg: t.muted }), qx)
  if (ui.typing) buf.cursor = { x: end, y: sy }
  if (q) buf.lineRight(rect(P.x, sy, P.w - 1, 1), [[`${data.modTotal} on Modrinth`, { fg: t.faint }]])
  buf.region(field, 'query')

  // Details of the selected row: beside the list when there's room, else at the foot.
  const side = L.wide
  const detailH = P.h >= 22 ? 6 : 5
  const listW = side ? Math.min(64, Math.floor(P.w * 0.55)) : P.w
  const list = side ? rect(P.x, sy + 2, listW, bottom(P) - sy - 2) : rect(P.x, sy + 2, P.w, bottom(P) - detailH - 1 - (sy + 2))
  const detailR = side ? rect(P.x + listW + 6, sy + 2, P.w - listW - 6, 10) : rect(P.x, bottom(P) - detailH, P.w, detailH)
  const nameW = Math.min(26, Math.max(16, ...rows.map((r) => width(r.kind === 'hit' ? r.hit.name : r.s.mod.name) + 3)))
  const focused = focus === 0
  let y = list.y

  const drawGroup = (label: string, count: string, group: Row[], offset: number, trail?: Line) => {
    if (y >= bottom(list)) return
    p.label(c, P.x, y, label, count)
    if (trail) buf.lineRight(rect(P.x, y, list.w, 1), trail)
    y++
    const room = bottom(list) - y
    const start = Math.max(0, Math.min(group.length - room, sel - offset - room + 1))
    group.slice(start, start + room).forEach((r, j) => {
      const i = offset + start + j
      const on = sel === i
      p.band(c, rect(P.x, y, list.w, 1), on, focused, `sel:${i}@0`)
      const nameSt = on && focused ? { fg: t.strong, bold: true } : on ? { fg: t.strong } : { fg: t.text }
      if (r.kind === 'installed') {
        const { mod, note } = r.s
        buf.line(P.x, y, p.highlight(c, mod.name, q, mod.enabled ? nameSt : { ...nameSt, fg: t.muted }), {}, nameW - 2)
        buf.text(P.x + nameW, y, mod.version, { fg: t.muted }, 10)
        if (note) {
          const fg = { update: t.success, warn: t.warning, error: t.error, quiet: t.muted }[note.tone]
          buf.text(P.x + nameW + 12, y, note.text, { fg }, right(list) - (P.x + nameW + 12))
        }
      } else {
        const h = r.hit
        buf.line(P.x, y, p.highlight(c, h.name, q, nameSt), {}, nameW - 2)
        const dx = buf.lineRight(rect(P.x, y, list.w, 1), [[h.downloads, { fg: t.muted }]])
        buf.text(P.x + nameW, y, h.category.toLowerCase(), { fg: t.muted }, dx - (P.x + nameW) - 2)
      }
      y++
    })
    y++
  }

  if (installed.length || !q)
    drawGroup('installed', q ? `${installed.length} of ${inst.mods.length}` : String(inst.mods.length), installed, 0)
  if (q) {
    if (found.length) drawGroup('modrinth', String(found.length), found, installed.length)
    else buf.text(P.x, y, `Nothing new on Modrinth for “${q}”.`, { fg: t.faint }, P.w)
  }
  if (!rows.length && !q) buf.text(P.x, list.y + 1, 'None yet. / searches Modrinth.', { fg: t.faint })

  if (cur) detail(c, detailR, cur, side)

  const hints: [string, string][] = ui.typing
    ? [['↵', 'done'], ['esc', 'clear'], ['↑↓', 'pick']]
    : cur?.kind === 'hit'
      ? [['↵', 'install'], ['/', 'search'], ['esc', q ? 'clear' : 'back'], ['?', 'help']]
      : cur?.s.note?.tone === 'update'
        ? [['u', 'update'], ['space', 'disable'], ['d', 'remove'], ['/', 'search'], ['esc', 'back'], ['?', 'help']]
        : [['space', cur?.s.mod.enabled === false ? 'enable' : 'disable'], ['d', 'remove'], ['/', 'search'], ['esc', 'back'], ['?', 'help']]
  p.footer(c, L, hints)
  c.enter(null)
}

/** The selected mod: who made it, what it does, and its one action. */
function detail(c: Ctx, r: ReturnType<typeof rect>, row: Row, side: boolean) {
  const { buf, t } = c
  const hit = row.kind === 'hit' ? row.hit : row.hit
  const name = row.kind === 'hit' ? row.hit.name : row.s.mod.name
  buf.text(r.x, r.y, name, { fg: t.strong, bold: true }, r.w)
  const by: string[] = hit ? [`by ${hit.author}`, hit.category, hit.side] : []
  if (row.kind === 'installed') by.unshift(row.s.mod.version)
  // The by-line sheds its least useful part until it fits.
  while (by.length > 1 && width(by.join(' · ')) > r.w) by.pop()
  buf.text(r.x, r.y + 1, by.join(' · '), { fg: t.muted }, r.w)
  const desc = hit?.description ?? 'Installed from a file, so there’s nothing more to say about it.'
  const all = wrap(desc, r.w)
  const room = Math.max(1, r.h - 4)
  const lines = all.slice(0, room)
  if (all.length > room) lines[room - 1] = truncate(`${lines[room - 1]} ${all[room]}`, r.w)
  lines.forEach((l, i) => buf.text(r.x, r.y + 2 + i, l, { fg: t.text }))
  // The one action, and a quiet fact beside it (or under it, when narrow).
  // Beside the list, the fact reads under the description and the pill ends the block.
  const factY = r.y + 2 + lines.length
  const ay = side ? factY + 2 : r.y + r.h - 1
  const aside = (b: ReturnType<typeof rect>, text: string) => {
    if (side) buf.text(r.x, factY, text, { fg: t.muted }, r.w)
    else if (right(b) + 3 + width(text) <= right(r)) buf.text(right(b) + 3, ay, text, { fg: t.muted })
  }
  if (row.kind === 'hit') {
    aside(p.pill(c, r.x, ay, 'Install', '↵', 'key:↵'), `${row.hit.downloads} downloads · updated ${row.hit.updated}`)
  } else {
    const note = row.s.note
    if (note?.tone === 'update') {
      const b = p.pill(c, r.x, ay, `Update to ${row.s.mod.update}`, 'u', 'key:u')
      if (hit) aside(b, `updated ${hit.updated}`)
    } else if (note) {
      const fg = { warn: t.warning, error: t.error, quiet: t.muted, update: t.success }[note.tone]
      buf.text(r.x, ay, note.text, { fg }, r.w)
    } else buf.line(r.x, ay, [['✓ ', { fg: t.success }], ['up to date', { fg: t.muted }]], {}, r.w)
  }
}
