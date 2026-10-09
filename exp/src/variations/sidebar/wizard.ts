// New instance: a new page in the main area. Three rows at the top hold the
// answers (every one pre-filled with a good default, the active one marked),
// and the active question's choices sit below them, so nothing moves as you
// step. The sidebar shows the instance being made, its second line filling in.

import type { Ctx } from '../../kit'
import { type Line, type Style, rect, right, bottom } from '../../term/buffer'
import * as p from './parts'
import { helpPage } from './help'

const LOADERS: [string, string][] = [
  ['Fabric', 'runs mods; Fabric API comes with it'],
  ['Vanilla', 'the game exactly as Mojang ships it'],
]

export function wizard(c: Ctx) {
  const { buf, t, data, ui } = c
  const L = p.layout(c)
  const P = L.page
  const step = c.tabs(3)
  const versions = data.versions.filter((v) => !v.snapshot)
  const latest = versions.find((v) => v.latest) ?? versions[0]
  const sel = c.sel(step === 0 ? versions.length : step === 1 ? LOADERS.length : 1)

  const version = step === 0 ? versions[sel].id : latest.id
  const loader = step === 1 ? LOADERS[sel][0] : 'Fabric'
  const fallback = `${version} ${loader}`
  const name = step === 2 && ui.query ? ui.query : fallback

  // The sidebar: the draft entry, selected, filling in as you answer.
  const answered = (i: number): Style => (i <= step ? { fg: t.dim } : { fg: t.faint })
  p.sidebar(c, L, {
    sel: -1,
    focused: false,
    draft: {
      name: step === 2 ? [[name, { fg: t.strong, bold: true }]] : [['new…', { fg: t.strong, bold: true }]],
      meta: [[version, answered(0)], [' · ', { fg: t.faint }], [loader, answered(1)]],
    },
    region: () => 'screen:home',
  })
  p.account(c, L)
  if (ui.overlay && ui.overlay !== 'confirm') return helpPage(c, L)

  p.title(c, P, ['New instance'], [['Every answer has a default: ↵ ↵ ↵ makes it.', { fg: t.dim }]])

  // The answers: stable rows, clickable to jump back to a step.
  const answers: [string, string, string?][] = [
    ['version', version, step === 0 && versions[sel].latest ? 'latest release' : undefined],
    ['loader', loader + (loader === 'Fabric' ? ' 0.17.2' : '')],
    ['name', name],
  ]
  answers.forEach(([label, value, note], i) => {
    const y = P.y + 3 + i
    const on = i === step
    const hov = c.hovered(`tab:${i}`)
    buf.text(P.x, y, on ? '›' : ' ', { fg: t.accent, bold: true })
    buf.text(P.x + 2, y, label, on ? { fg: t.accent, bold: true } : { fg: hov ? t.text : t.muted })
    const vst: Style = on ? { fg: t.strong, bold: true } : i < step ? { fg: t.text } : { fg: t.dim }
    const vx = buf.text(P.x + 12, y, value, vst, P.w - 12)
    if (note) buf.text(vx + 2, y, note, { fg: t.success })
    else if (i > step) buf.text(vx + 2, y, 'default', { fg: t.faint })
    buf.region(rect(P.x, y, P.w, 1), `tab:${i}`)
  })

  const top = P.y + 7
  // Choices read as a column, not a table across the whole page.
  const area = rect(P.x, top, Math.min(P.w, 64), bottom(P) - top)
  if (step === 0) {
    p.label(c, area.x, area.y, 'releases', versions.length)
    buf.lineRight(rect(area.x, area.y, area.w, 1), [['snapshots hidden', { fg: t.faint }]])
    const list = rect(area.x, area.y + 1, area.w, area.h - 1)
    const fit = versions.length > list.h ? list.h - 1 : list.h
    const start = Math.max(0, Math.min(versions.length - fit, sel - fit + 1))
    versions.slice(start, start + fit).forEach((v, j) => {
      const i = start + j
      const y = list.y + j
      const on = sel === i
      p.band(c, rect(list.x, y, list.w, 1), on, true, `sel:${i}`)
      buf.text(list.x, y, v.id, on ? { fg: t.strong, bold: true } : { fg: t.text })
      if (v.latest) buf.text(list.x + 12, y, 'latest', { fg: t.success })
      buf.lineRight(rect(list.x, y, list.w, 1), [[v.date, { fg: on ? t.dim : t.muted }]])
    })
    const more = versions.length - start - fit
    if (more > 0) buf.text(list.x, list.y + fit, `↓ ${more} more, back to ${versions[versions.length - 1].id}`, { fg: t.faint })
  } else if (step === 1) {
    p.label(c, area.x, area.y, 'loader')
    LOADERS.forEach(([l, why], i) => {
      const y = area.y + 1 + i
      const on = sel === i
      p.band(c, rect(area.x, y, area.w, 1), on, true, `sel:${i}`)
      buf.text(area.x, y, l, on ? { fg: t.strong, bold: true } : { fg: t.text })
      buf.text(area.x + 12, y, why, { fg: on ? t.dim : t.muted }, area.w - 12)
    })
  } else {
    p.label(c, area.x, area.y, 'name')
    const fy = area.y + 1
    const field = rect(area.x - 1, fy, area.w + 2, 1)
    buf.fill(field, { bg: ui.typing ? t.selection : t.surface })
    const end = ui.query
      ? buf.text(area.x, fy, ui.query, { fg: t.strong }, area.w - 2)
      : buf.text(area.x, fy, fallback, { fg: ui.typing ? t.muted : t.text }, area.w - 2)
    if (ui.typing) buf.cursor = { x: ui.query ? end : area.x, y: fy }
    buf.region(field, 'query')
    const slug = name.toLowerCase().replace(/[^a-z0-9.]+/g, '-')
    buf.line(area.x, fy + 1, [['saved as ', { fg: t.faint }], [`instances/${slug}`, { fg: t.muted }]], {}, area.w)
    const b = p.pill(c, area.x, fy + 3, 'Create', '↵', 'key:↵')
    const n = data.instances.length + 1
    buf.line(right(b) + 3, fy + 3, n <= 9 ? ([['it becomes ', { fg: t.muted }], [String(n), { fg: t.subtle, bold: true }], [' in the sidebar', { fg: t.muted }]] as Line) : [], {}, right(P) - right(b) - 3)
  }

  p.footer(
    c,
    L,
    step === 0
      ? [['↵', 'next'], ['←→', 'step'], ['space', 'snapshots'], ['esc', 'cancel'], ['?', 'help']]
      : step === 1
        ? [['↵', 'next'], ['←→', 'step'], ['esc', 'cancel'], ['?', 'help']]
        : [['↵', 'create'], ['/', 'rename'], ['←', 'back'], ['esc', 'cancel'], ['?', 'help']],
  )
  c.enter(step < 2 ? `tab:${step + 1}` : 'home')
}
