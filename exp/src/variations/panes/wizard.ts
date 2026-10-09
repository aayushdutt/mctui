// New instance: the three steps as three panels side by side, so every
// choice stays in view. Tab (or ↵) moves to the next step; the status bar
// carries the running summary.

import type { Ctx } from '../../kit'
import { type Line, type Rect, rect, cols, right, bottom, wrap } from '../../term/buffer'
import * as f from './frame'
import { loaders } from './data'
import { overlays } from './overlays'

export function wizard(c: Ctx) {
  const { buf, t, data } = c
  const W = buf.cols
  const body = rect(1, 0, W - 2, buf.rows - 1)
  const step = c.panes(3)

  const versions = data.versions.filter((v) => !v.snapshot).filter((v) => v.id.includes(f.query(c)))
  // Each step keeps what was picked while the next one has focus.
  const vsel = c.sel(Math.max(1, versions.length), 0)
  const lsel = step === 0 ? 1 : c.sel(loaders.length, 1)
  const fsel = step === 2 ? c.sel(5) : -1
  const version = versions[vsel]?.id ?? '—'
  const loader = loaders[lsel]
  const loaderLabel = loader.version ? `${loader.name} ${loader.version}` : loader.name
  const name = loader.name === 'Vanilla' ? version : `${version} ${loader.name}`

  const [vR, , lR, , nR] = cols(body, W >= 124 ? ['1fr', 2, '1fr', 2, '1fr'] : W >= 96 ? ['34%', 2, '30%', 2, '1fr'] : ['37%', 2, '27%', 2, '1fr'])
  versionPanel(c, vR, versions, vsel, step)
  loaderPanel(c, lR, lsel, step, version)
  namePanel(c, nR, fsel, step, name, version, loaderLabel)

  const hints: [string, string][] =
    step === 0
      ? [['↵', 'next'], ['/', 'filter'], ['space', 'snapshots'], ['tab', 'step'], ['esc', 'cancel'], ['?', 'help']]
      : step === 1
        ? [['↵', 'next'], ['tab', 'step'], ['esc', 'cancel'], ['?', 'help']]
        : [['↵', 'create'], ['←→', 'change'], ['tab', 'step'], ['esc', 'cancel'], ['?', 'help']]
  const summary: Line = [
    [version, { fg: t.text }],
    ['  ·  ', { fg: t.faint }],
    [step >= 1 ? loaderLabel : '…', { fg: step >= 1 ? t.text : t.faint }],
    ['  ·  ', { fg: t.faint }],
    [step >= 2 ? name : '…', { fg: step >= 2 ? t.text : t.faint }],
  ]
  f.statusBar(c, f.typing(c) ? { label: 'FILTER' } : { label: 'NEW' }, W >= 96 ? summary : [], hints)
  overlays(c, { screen: 'New instance', subject: name })
}

/** `1 Version` · `✓ Version` once done · dim while pending. */
function stepTitle(c: Ctx, n: number, label: string, step: number): Line {
  const { t } = c
  if (n < step) return [['✓ ', { fg: t.success }], [label, { fg: t.text }]]
  if (n === step) return [[`${n + 1} `, { fg: t.accent }], label]
  return [[`${n + 1} `, { fg: t.faint }], [label, { fg: t.dim, bold: false }]]
}

function versionPanel(c: Ctx, r: Rect, versions: typeof c.data.versions, sel: number, step: number) {
  const { buf, t } = c
  const focused = step === 0
  const snapHov = c.hovered('snapshots')
  const inner = f.panel(c, r, stepTitle(c, 0, 'Version', step), {
    focused,
    count: versions.length,
    region: 'focus:0',
    right: [['snapshots ', { fg: snapHov ? t.text : t.dim }], ['○', { fg: t.muted }]],
  })
  if (r.w >= 34) buf.region(rect(right(r) - 14, r.y, 12, 1), 'snapshots')
  let y = inner.y
  if (f.typing(c) || f.query(c)) {
    const row = rect(r.x, y, r.w, 1)
    buf.fill(row, { bg: f.typing(c) ? t.selection : t.surface })
    buf.text(inner.x, y, '/', { fg: t.accent, bold: true })
    const end = f.query(c) ? buf.text(inner.x + 2, y, f.query(c), { fg: t.strong }) : inner.x + 2
    if (!f.query(c)) buf.text(inner.x + 2, y, 'filter versions', { fg: t.muted })
    if (f.typing(c)) buf.cursor = { x: end, y }
    buf.region(row, 'query')
    y++
  }
  const spec: f.Col[] = inner.w >= 26 ? [{ label: 'Version', w: 'fr' }, { label: 'Released', w: 12, align: 'right' }] : [{ label: 'Version', w: 'fr' }]
  const list = rect(inner.x, y, inner.w, bottom(inner) - y)
  const p = f.place(list, spec)
  f.header(c, list.y, p)
  const fit = list.h - 1
  const start = f.scrollStart(versions.length, sel, fit)
  versions.slice(start, start + fit).forEach((v, j) => {
    const i = start + j
    const ry = list.y + 1 + j
    const row = rect(r.x, ry, r.w, 1)
    const state = f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${i}` : 'focus:0')
    const tag: Line = v.latest ? [['  latest', { fg: t.success }]] : []
    f.cell(c, p[0], ry, [...f.highlight(c, v.id, f.query(c), f.rowText(c, state)), ...tag])
    if (p[1]) f.cell(c, p[1], ry, [[v.date, { fg: state ? t.subtle : t.dim }]])
  })
  const below = versions.length - start - fit
  if (below > 0) buf.lineRight(rect(r.x, bottom(r) - 1, r.w - 2, 1), [[` ↓ ${below} more `, { fg: t.muted }]])
  if (!versions.length) buf.text(list.x + 2, list.y + 1, `No release matches “${f.query(c)}”.`, { fg: t.faint }, list.w - 2)
}

function loaderPanel(c: Ctx, r: Rect, sel: number, step: number, version: string) {
  const { buf, t } = c
  const focused = step === 1
  const inner = f.panel(c, r, stepTitle(c, 1, 'Loader', step), { focused, region: 'focus:1', right: [[`for ${version}`, { fg: t.dim }]] })
  const p = f.place(
    inner,
    inner.w >= 26
      ? [{ label: 'Loader', w: 'fr' }, { label: 'Build', w: 8 }, { label: '', w: 7, align: 'right' }]
      : [{ label: 'Loader', w: 'fr' }, { label: 'Build', w: 8 }],
  )
  f.header(c, inner.y, p)
  loaders.forEach((l, i) => {
    const y = inner.y + 1 + i
    if (y >= bottom(inner)) return
    const row = rect(r.x, y, r.w, 1)
    const chosen = i === sel && step > 1
    // A pending step shows its default without claiming it's picked.
    const state = step < 1 ? (c.hovered(`sel:${i}`) ? 'hover' : null) : f.rowState(c, i === sel, focused, `sel:${i}`)
    f.band(c, row, state)
    buf.region(row, focused ? `sel:${i}` : 'focus:1')
    const sameAsAbove = i > 0 && loaders[i - 1].name === l.name
    f.cell(c, p[0], y, [[sameAsAbove ? '' : l.name, state || chosen ? f.rowText(c, state) : { fg: t.text }]])
    const build = l.version ?? '—'
    const buildFg = l.latest && !p[2] ? t.success : l.version ? (state ? t.text : t.subtle) : t.faint
    f.cell(c, p[1], y, [[build, { fg: buildFg }]])
    if (p[2]) f.cell(c, p[2], y, [[l.note, { fg: l.latest ? t.success : t.muted }]])
  })
  const ny = inner.y + loaders.length + 2
  const note =
    loaders[sel].name === 'Vanilla'
      ? 'Plain Minecraft: no mods. Resource packs still work.'
      : 'Fabric runs mods from Modrinth. Fabric API gets added for you; switch builds any time.'
  wrap(note, inner.w).forEach((l, i) => {
    if (ny + i < bottom(inner)) buf.text(inner.x, ny + i, l, { fg: t.muted })
  })
}

function namePanel(c: Ctx, r: Rect, sel: number, step: number, name: string, version: string, loader: string) {
  const { buf, t } = c
  const focused = step === 2
  const inner = f.panel(c, r, stepTitle(c, 2, 'Name', step), { focused, region: 'focus:2' })
  const lw = 9
  const fields: [string, string, 'field' | 'choice' | 'info'][] = [
    ['Name', name, 'field'],
    ['Memory', '4 GB', 'choice'],
    ['Java', 'automatic', 'choice'],
    ['Folder', `instances/${name.toLowerCase().replace(/ /g, '-')}`, 'info'],
  ]
  fields.forEach(([label, value, kind], i) => {
    const y = inner.y + i
    if (y >= bottom(inner)) return
    const on = focused && sel === i
    const hov = c.hovered(`sel:${i}`)
    buf.text(inner.x, y, label, { fg: on ? t.strong : t.dim, bold: on })
    const vr = rect(inner.x + lw, y, inner.w - lw, 1)
    if (kind === 'field') {
      buf.fill(rect(vr.x - 1, y, vr.w + 1, 1), { bg: on ? t.selection : hov ? t.surfaceHi : t.surface })
      const end = buf.text(vr.x, y, value, { fg: step >= 2 ? t.strong : t.dim }, vr.w - 1)
      if (on) buf.cursor = { x: end, y }
    } else if (kind === 'choice') {
      const arrows = { fg: on ? t.accent : t.faint }
      buf.line(vr.x, y, [['‹ ', arrows], [value, { fg: on ? t.strong : t.text, bold: on }], [' ›', arrows]])
    } else buf.text(vr.x, y, value, { fg: t.muted }, vr.w)
    buf.region(rect(r.x, y, r.w, 1), focused ? `sel:${i}` : 'focus:2')
  })
  // Summary, then the one primary action.
  const sy = inner.y + fields.length + 1
  const by = Math.min(bottom(inner) - 1, sy + 5)
  if (by === sy + 5) {
    f.subRule(c, rect(r.x, sy, r.w, 1), 'Summary')
    const rowsS: [string, string][] = [
      ['Minecraft', version],
      ['Loader', step >= 1 ? loader : '—'],
      ['Downloads', loader.startsWith('Fabric') ? '~ 412 MB' : '~ 380 MB'],
    ]
    rowsS.forEach(([k, v], i) => {
      buf.text(inner.x, sy + 1 + i, k, { fg: t.dim })
      buf.text(inner.x + 11, sy + 1 + i, v, { fg: v === '—' ? t.faint : t.text }, inner.w - 11)
    })
  }
  const onCreate = focused && sel === 4
  const b = f.button(c, inner.x, by, '+ Create', '↵', step === 2 ? 'primary' : 'secondary', 'create')
  if (onCreate) buf.text(right(b) + 1, by, '‹', { fg: t.accent, bold: true })
}
