// New instance as columns: Version → Loader → Name. Each column narrows the
// next (old versions offer no Fabric), each has a good default selected, so
// ↵ ↵ ↵ makes something sensible and ← always goes back a step.

import type { Ctx } from '../../kit'
import type { Version } from '../../data/mock'
import { rect } from '../../term/buffer'
import { type Item, filterRow, footer, frame, header, hint, kv, list, para, slots, title } from './ui'
import { defaultName, fuzzy, loadersFor } from './model'
import { overlays } from './sheet'

const javaFor = (v: Version) => {
  const [a, b] = v.id.split(/[.-]/).map(Number)
  if (a >= 2) return '25'
  if (b >= 21) return '21'
  if (b >= 18) return '17'
  return '8'
}

export function wizard(c: Ctx) {
  const { data, ui, t, buf } = c
  const f = frame(c)
  const sl = slots(f)
  const focus = c.panes(3)
  const q = ui.query

  const all = data.versions.filter((v) => !v.snapshot)
  const filtering = focus === 0 && !!q
  const vs = all.map((v) => ({ v, hits: filtering ? fuzzy(q, v.id) : [] })).filter((x) => x.hits)
  const s0 = c.sel(Math.max(1, vs.length), { pane: 0 })
  const v = vs[s0]?.v ?? all[0]
  const loaders = loadersFor(v)
  const s1 = c.sel(loaders.length, { pane: 1 })
  const loader = loaders[s1]
  c.sel(1, { pane: 2 })
  const typed = focus === 2 && q ? q : ''
  const name = typed || defaultName(v, loader)

  const versionItems: Item[] = vs.map(({ v, hits }) => ({
    label: v.id,
    hits,
    trail: v.latest ? [['latest', { fg: t.subtle }]] : [[v.date, { fg: t.muted }]],
  }))
  const loaderItems: Item[] = loaders.map((l) => ({ label: l.name, trail: l.version ? [[l.version, { fg: t.dim }]] : [] }))

  const versionCol = (r: typeof sl[0], at: number) => {
    const on = at === focus
    if (on && q) filterRow(c, r, q, `${vs.length} of ${all.length}`)
    else title(c, r, 'Version', { count: all.length, focused: on, region: 'focus:0' })
    list(c, rect(r.x, r.y + 2, r.w, r.h - 2), {
      items: versionItems,
      sel: s0,
      mark: on ? 'focus' : 'path',
      tone: on ? 'focus' : 'parent',
      pane: 0,
      empty: [['No version like that.', { fg: t.muted }]],
    })
  }
  const loaderCol = (r: typeof sl[0]) => {
    const on = focus === 1
    title(c, r, 'Loader', { focused: on, region: 'focus:1' })
    const a = rect(r.x, r.y + 2, r.w, r.h - 2)
    list(c, a, {
      items: loaderItems,
      sel: s1,
      mark: on ? 'focus' : focus > 1 ? 'path' : 'ahead',
      tone: on ? 'focus' : focus > 1 ? 'parent' : 'preview',
      pane: 1,
    })
    let y = a.y + loaderItems.length + 1
    y += para(c, rect(a.x + 1, y, a.w - 2, 3), loader.about, { fg: t.dim })
    if (loaders.length === 1) para(c, rect(a.x + 1, y + 1, a.w - 2, 2), `Fabric needs 1.14 or newer.`, { fg: t.faint })
  }
  const nameCol = (r: typeof sl[0]) => {
    const on = focus === 2
    title(c, r, 'Name', { focused: on, region: 'focus:2' })
    const a = rect(r.x + 1, r.y + 2, r.w - 2, r.h - 2)
    const field = rect(r.x, a.y, r.w, 1)
    if (on) buf.fill(field, { bg: t.selection })
    else if (c.hovered('sel:0@2')) buf.fill(field, { bg: t.surface })
    const end = buf.text(a.x, a.y, name, on ? { fg: t.strong, bold: true } : { fg: t.text, bold: focus < 2 }, a.w)
    if (on) buf.cursor = { x: end, y: a.y }
    buf.region(field, 'sel:0@2')
    let y = a.y + 2
    kv(c, a.x, y++, a.w, 'Minecraft', [v.id], 11)
    kv(c, a.x, y++, a.w, 'Loader', [loader.version ? `Fabric ${loader.version}` : 'Vanilla'], 11)
    kv(c, a.x, y++, a.w, 'Memory', [loader.name === 'Fabric' ? '4 GB' : '3 GB'], 11)
    kv(c, a.x, y++, a.w, 'Java', [`${javaFor(v)}, automatic`], 11)
    y++
    y += para(c, rect(a.x, y, a.w, 2), 'About 280 MB downloads the first time you play it.', { fg: t.muted }) + 1
    if (on) hint(c, a.x, y, [['↵', 'create'], ['/', 'rename']], a.w)
    else buf.text(a.x, y, focus === 0 ? '↵ ↵ ↵ makes it with these.' : '↵ ↵ makes it with these.', { fg: t.faint }, a.w)
  }

  if (f.narrow) {
    const start = Math.min(focus, 1)
    if (start === 0) {
      versionCol(sl[0], 0)
      loaderCol(sl[1])
    } else {
      loaderCol(sl[0])
      nameCol(sl[1])
    }
  } else {
    versionCol(sl[0], 0)
    loaderCol(sl[1])
    nameCol(sl[2])
  }

  const crumbs: [string, string?][] = [['New instance', 'focus:0'], [v.id, 'focus:0']]
  if (focus >= 1) crumbs.push([loader.name, 'focus:1'])
  header(c, f.header, crumbs)
  c.enter(focus === 0 ? 'focus:1' : focus === 1 ? 'focus:2' : 'home')
  footer(
    c,
    f.footer,
    ui.typing
      ? [['↵', 'done'], ['esc', 'clear']]
      : focus === 0
        ? [['↵', 'next'], ['/', 'filter'], ['.', 'snapshots'], ['esc', 'cancel']]
        : focus === 1
          ? [['↵', 'next'], ['←', 'back'], ['esc', 'cancel']]
          : [['↵', 'create'], ['/', 'rename'], ['←', 'back'], ['esc', 'cancel']],
  )
  overlays(c, f, null, undefined)
}
