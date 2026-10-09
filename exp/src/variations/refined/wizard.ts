// Refined new instance: a three-field form (version, loader, name) that
// always shows what ↵ ↵ ↵ would make, and one picker beside it for the field
// in focus. Every field starts on a good default; enter accepts and moves on.

import type { Ctx } from '../../kit'
import type { Version } from '../../data/mock'
import { type Line, rect, right, bottom, width } from '../../term/buffer'
import { frame, header, footer, statusLine, helpSheet } from './frame'
import { filter, label, lit, type Hint } from './bits'

/** The Java a version runs on, as the app picks it. */
function javaFor(id: string): string {
  const [a, b = 0, c = 0] = id.split(/[.-]/).map((n) => Number(n) || 0)
  if (a >= 26) return 'Java 25'
  if (b > 20 || (b === 20 && c >= 5)) return 'Java 21'
  if (b >= 18) return 'Java 17'
  if (b === 17) return 'Java 16'
  return 'Java 8'
}

const LOADERS: [string, string][] = [
  ['Vanilla', 'The game as Mojang ships it'],
  ['Fabric', 'For mods: Sodium, Iris, Mod Menu…'],
]

export function wizard(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  header(c, f, ['New instance'])
  const step = c.panes(3)
  const X = f.x
  const formW = f.w >= 90 ? 34 : 30
  const px = X + formW + 6
  const pv = rect(px, f.body.y, right(f.body) - px, f.body.h)

  // The values: each field's own selection, whether or not it has focus.
  const releases = data.versions.filter((v) => !v.snapshot)
  const q = step === 0 ? ui.query : ''
  const shown = filter(releases, q, (v) => v.id)
  const vsel = c.sel(Math.max(1, shown.length), 0)
  const version: Version = shown[vsel]?.item ?? releases[0]
  const lsel = c.sel(LOADERS.length, 1)
  const loader = LOADERS[lsel][0]
  const base = loader === 'Vanilla' ? version.id : `${version.id} ${loader}`
  const taken = new Set(data.instances.map((i) => i.name))
  let name = base
  for (let k = 2; taken.has(name); k++) name = `${base} ${k}`

  // ── The form ──
  label(c, X, f.body.y, 'New instance')
  const fields: [string, string, string?][] = [
    ['Version', version.id, version.latest ? 'latest' : undefined],
    ['Loader', loader === 'Fabric' ? 'Fabric 0.17.2' : loader],
    ['Name', name, name !== base ? 'taken, so numbered' : undefined],
  ]
  fields.forEach(([k, v, note], i) => {
    const y = f.body.y + 1 + i
    const on = step === i
    const hov = c.hovered(`focus:${i}`)
    if (on) buf.text(X - 2, y, '›', { fg: t.accent, bold: true })
    buf.text(X, y, k, on ? { fg: t.accent, bold: true } : { fg: hov ? t.text : t.muted })
    const vx = buf.text(X + 10, y, v, on ? { fg: t.strong, bold: true } : i < step ? { fg: t.text } : { fg: t.subtle }, formW - 10)
    if (note && right(rect(X, y, formW, 1)) - vx > width(note) + 2) buf.text(vx + 2, y, note, { fg: t.faint })
    buf.region(rect(X - 2, y, formW + 2, 1), `focus:${i}`)
  })
  const memory = loader === 'Fabric' ? '4 GB' : '2 GB'
  const fy = f.body.y + 5
  buf.line(X, fy, [
    [javaFor(version.id), { fg: t.dim }],
    ['  ·  ', { fg: t.faint }],
    [`${memory} memory`, { fg: t.dim }],
  ], {}, formW)
  buf.text(X, fy + 1, formW >= 34 ? 'Picked for you; e changes them.' : 'e changes them later.', { fg: t.faint }, formW)
  if (step < 2) {
    const left = 2 - step
    buf.line(X, fy + 3, [
      [`${'↵ '.repeat(left + 1).trim()}`, { fg: t.accent, bold: true }],
      [left === 2 ? '  makes it as shown' : '  makes it', { fg: t.dim }],
    ], {}, formW)
  }

  // ── The picker for the field in focus ──
  if (step === 0) {
    const ly = pv.y
    if (ui.typing || q) {
      const qx = buf.text(pv.x, ly, '/', { fg: t.accent, bold: true }) + 1
      const end = buf.text(qx, ly, q, { fg: t.strong })
      if (ui.typing) buf.cursor = { x: end, y: ly }
      buf.lineRight(rect(pv.x, ly, pv.w, 1), [[`${shown.length} of ${releases.length}`, { fg: t.muted }]])
    } else {
      label(c, pv.x, ly, 'Releases', releases.length, true)
      buf.lineRight(rect(pv.x, ly, pv.w, 1), [['snapshots hidden', { fg: t.faint }]])
    }
    const top = ly + 1
    const fit = Math.max(1, bottom(pv) - top - 1)
    const n = shown.length
    const first = Math.max(0, Math.min(vsel - fit + 1, n - fit, vsel))
    const idW = Math.max(...releases.map((v) => width(v.id)))
    const dateW = Math.max(...releases.map((v) => width(v.date)))
    const usedBy = (id: string) => data.instances.filter((i) => i.mc === id).map((i) => i.name)
    shown.slice(first, first + fit).forEach(({ item: v, at }, j) => {
      const i = first + j
      const y = top + j
      const on = vsel === i
      const hov = c.hovered(`sel:${i}@0`)
      if (on) buf.fill(rect(pv.x - 2, y, pv.w + 4, 1), { bg: t.selection })
      else if (hov) buf.fill(rect(pv.x - 2, y, pv.w + 4, 1), { bg: t.surface })
      lit(c, pv.x, y, v.id, at, on ? { fg: t.strong, bold: true } : { fg: t.text }, idW)
      let x = pv.x + idW + 3
      if (v.latest) x = buf.text(x, y, 'latest', { fg: t.success }) + 3
      const used = usedBy(v.id)
      const dx = right(pv) - dateW
      if (used.length) {
        const long = `used by ${used.join(', ')}`
        const room = dx - x - 3
        if (width(long) <= room) buf.text(x, y, long, { fg: t.muted })
        else if (room >= 6) buf.text(x, y, 'in use', { fg: t.muted })
      }
      buf.lineRight(rect(pv.x, y, pv.w, 1), [[v.date, { fg: on ? t.subtle : t.dim }]])
      buf.region(rect(pv.x - 2, y, pv.w + 4, 1), `sel:${i}@0`)
    })
    const below = n - first - Math.min(fit, n - first)
    if (below > 0) buf.text(pv.x, top + fit, `↓ ${below} more`, { fg: t.faint })
    if (!n) buf.text(pv.x, top, `No release matches “${q}”`, { fg: t.dim })
  } else if (step === 1) {
    label(c, pv.x, pv.y, 'Loader', undefined, true)
    const nameW = Math.max(...LOADERS.map(([n]) => width(n)))
    LOADERS.forEach(([n, d], i) => {
      const y = pv.y + 1 + i
      const on = lsel === i
      const hov = c.hovered(`sel:${i}@1`)
      if (on) buf.fill(rect(pv.x - 2, y, pv.w + 4, 1), { bg: t.selection })
      else if (hov) buf.fill(rect(pv.x - 2, y, pv.w + 4, 1), { bg: t.surface })
      buf.text(pv.x, y, n, on ? { fg: t.strong, bold: true } : { fg: t.text })
      buf.text(pv.x + nameW + 3, y, d, { fg: on ? t.subtle : t.dim }, pv.w - nameW - 3)
      buf.region(rect(pv.x - 2, y, pv.w + 4, 1), `sel:${i}@1`)
    })
    const note: Line = [['Fabric 0.17.2', { fg: t.text }], [` supports ${version.id}. Add mods after, with `, { fg: t.dim }], ['m', { fg: t.strong, bold: true }], ['.', { fg: t.dim }]]
    buf.line(pv.x, pv.y + 4, note, {}, pv.w)
  } else {
    label(c, pv.x, pv.y, 'Name', undefined, true)
    const fr = rect(pv.x - 1, pv.y + 1, Math.min(pv.w + 1, 40), 1)
    buf.fill(fr, { bg: t.selection })
    const end = buf.text(pv.x, fr.y, name, { fg: t.strong, bold: true }, fr.w - 2)
    buf.cursor = { x: end, y: fr.y }
    buf.line(pv.x, pv.y + 3, [['Folder  ', { fg: t.muted }], [`instances/${name.replace(/\s+/g, '-').toLowerCase()}`, { fg: t.dim }]], {}, pv.w)
    buf.text(pv.x, pv.y + 5, `The game downloads the first time you play it.`, { fg: t.dim }, pv.w)
  }

  statusLine(c, f)
  const keys: Hint[] = ui.typing
    ? [['↑↓', 'pick'], ['↵', 'done'], ['esc', 'clear']]
    : step === 0
      ? [['↵', 'next'], ['/', 'filter'], ['space', 'show snapshots'], ['esc', 'cancel']]
      : step === 1
        ? [['↵', 'next'], ['↑↓', 'pick'], ['shift+tab', 'back'], ['esc', 'cancel']]
        : [['↵', 'create'], ['shift+tab', 'back'], ['esc', 'cancel']]
  footer(c, f, keys)
  c.enter(step < 2 ? `focus:${step + 1}` : 'home')
  if (ui.overlay === 'help')
    helpSheet(c, f, 'New instance · keys', [
      ['Form', [['↵', 'accept, next'], ['tab', 'next field'], ['shift+tab', 'previous'], ['esc', 'cancel']]],
      ['Version', [['↑↓', 'pick'], ['/', 'filter'], ['space', 'snapshots']]],
    ])
}
