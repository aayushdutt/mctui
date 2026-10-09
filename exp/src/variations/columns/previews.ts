// What goes in a column for each kind of thing: the lists one level down
// (worlds, mods, packs, settings), the details at the end of a path, and the
// content previews that aren't lists (the game log, a download).

import type { Ctx } from '../../kit'
import type { Instance, ModHit, World } from '../../data/mock'
import { type Line, type Rect, rect, right, bottom, width } from '../../term/buffer'
import { type Item, bar, hint, kv, para, title } from './ui'
import { type Kind, type ModRow, type Setting, fuzzy, logFg, logMark, logOf, modRows, modTrail, settingsOf, stateOf } from './model'

export type Ref =
  | { k: 'world'; w: World }
  | { k: 'mod'; m: ModRow }
  | { k: 'hit'; h: ModHit }
  | { k: 'add'; what: 'mods' | 'packs' }
  | { k: 'pack'; name: string; i: number; n: number }
  | { k: 'setting'; s: Setting }

export interface Level {
  items: Item[]
  refs: Ref[]
  empty?: Line
  extraW?: number
  /** The column title's count, or the filter's `n of m`. */
  count?: string | number
  filterCount?: string
}

export const LISTS: Kind[] = ['worlds', 'mods', 'packs', 'settings']

export const refName = (r: Ref | undefined): string | undefined =>
  !r ? undefined : r.k === 'world' ? r.w.name : r.k === 'mod' ? r.m.name : r.k === 'hit' ? r.h.name : r.k === 'pack' ? r.name : r.k === 'setting' ? r.s.label : undefined

/** The rows one level inside an instance entry. `query` filters (and, for mods, searches Modrinth). */
export function level(c: Ctx, inst: Instance, kind: Kind, query: string): Level {
  const { t, data } = c
  if (kind === 'worlds') {
    const ws = inst.worlds.map((w) => ({ w, hits: fuzzy(query, w.name) })).filter((x) => x.hits)
    return {
      items: ws.map(({ w, hits }) => ({
        label: w.name,
        hits,
        trail: [[w.lastPlayed, { fg: t.dim }]],
        extra: [[w.mode.padEnd(10), { fg: t.muted }], [w.size.padStart(7), { fg: t.muted }]],
      })),
      refs: ws.map(({ w }) => ({ k: 'world', w })),
      empty: [['No worlds yet. They appear after you play.', { fg: t.muted }]],
      extraW: 17,
      count: inst.worlds.length,
      filterCount: `${ws.length} of ${inst.worlds.length}`,
    }
  }
  if (kind === 'mods') {
    const mods = modRows(c, inst)
    if (inst.loader === 'Vanilla')
      return { items: [], refs: [], empty: [['Vanilla can’t load mods.', { fg: t.muted }]], count: '' }
    if (!query) {
      const hitsOf = (m: ModRow) => m.hit
      return {
        items: [
          ...mods.map((m) => ({
            label: m.name,
            quiet: !m.enabled,
            trail: modTrail(c, m),
            extra: [[hitsOf(m)?.category ?? '', { fg: t.muted }]] as Line,
          })),
          { label: '+ Add from Modrinth', quiet: true, gap: mods.length > 0 },
        ],
        refs: [...mods.map((m) => ({ k: 'mod' as const, m })), { k: 'add', what: 'mods' }],
        extraW: 14,
        count: mods.length,
      }
    }
    const mine = mods.map((m) => ({ m, hits: fuzzy(query, m.name) })).filter((x) => x.hits)
    const names = new Set(mods.map((m) => m.name))
    const found = data.modHits.filter((h) => !names.has(h.name)).map((h) => ({ h, hits: fuzzy(query, h.name) })).filter((x) => x.hits)
    const items: Item[] = []
    const refs: Ref[] = []
    mine.forEach(({ m, hits }, i) => {
      items.push({ label: m.name, hits, quiet: !m.enabled, trail: modTrail(c, m), group: i ? undefined : [['Installed', { fg: t.muted }]] })
      refs.push({ k: 'mod', m })
    })
    found.forEach(({ h, hits }, i) => {
      items.push({
        label: h.name,
        hits,
        trail: [[h.downloads, { fg: t.dim }]],
        extra: [[h.category, { fg: t.muted }]],
        group: i ? undefined : [['Modrinth', { fg: t.muted }], [`  ${found.length} of ${data.modTotal}`, { fg: t.faint }]],
      })
      refs.push({ k: 'hit', h })
    })
    return {
      items,
      refs,
      extraW: 14,
      empty: [[`Nothing called “${query}”, here or on Modrinth.`, { fg: t.muted }]],
      count: mods.length,
      filterCount: `${mine.length + found.length} found`,
    }
  }
  if (kind === 'packs') {
    const ps = inst.packs.map((p, i) => ({ p, i, hits: fuzzy(query, p) })).filter((x) => x.hits)
    return {
      items: [
        ...ps.map(({ p, hits }): Item => ({ label: p, hits })),
        ...(query ? [] : [{ label: '+ Add from Modrinth', quiet: true, gap: ps.length > 0 }]),
      ],
      refs: [...ps.map(({ p, i }) => ({ k: 'pack' as const, name: p, i, n: inst.packs.length })), ...(query ? [] : [{ k: 'add' as const, what: 'packs' as const }])],
      count: inst.packs.length,
      filterCount: `${ps.length} of ${inst.packs.length}`,
    }
  }
  // settings
  const ss = settingsOf(inst)
    .map((s) => ({ s, hits: fuzzy(query, s.label) }))
    .filter((x) => x.hits)
  return {
    items: ss.map(({ s, hits }) => ({ label: s.label, hits, trail: [[s.value, { fg: t.dim }]] })),
    refs: ss.map(({ s }) => ({ k: 'setting', s })),
    count: '',
    filterCount: `${ss.length} of 5`,
  }
}

// ── Details: the end of a path ────────────────────────────────────────────

/** The detail column for `ref`: its title row plus what it is and what you can do. */
export function detail(c: Ctx, r: Rect, inst: Instance, ref: Ref | undefined) {
  const { t, data } = c
  if (!ref) return
  const a = rect(r.x + 1, r.y + 2, r.w - 2, r.h - 2)
  let y = a.y
  const line = (l: Line) => c.buf.line(a.x, y++, l, { fg: t.text }, a.w)
  if (ref.k === 'world') {
    title(c, r, ref.w.name)
    kv(c, a.x, y++, a.w, 'Mode', [ref.w.mode])
    kv(c, a.x, y++, a.w, 'Size', [ref.w.size])
    kv(c, a.x, y++, a.w, 'Played', [ref.w.lastPlayed])
    y++
    para(c, rect(a.x, y, a.w, 2), `↵ starts ${inst.name} straight into this world, skipping the title screen.`, { fg: t.dim })
    return
  }
  if (ref.k === 'mod') {
    const m = ref.m
    title(c, r, m.name)
    if (m.hit) line([[`by ${m.hit.author} · ${m.hit.category} · ${m.hit.downloads}`, { fg: t.dim }]])
    y++
    kv(c, a.x, y++, a.w, 'Installed', [m.version])
    if (m.update) kv(c, a.x, y++, a.w, 'Update', [[m.update, { fg: t.accent }], [m.queued ? '  queued' : '  available', { fg: t.dim }]])
    if (m.broken) kv(c, a.x, y++, a.w, 'Problem', [[`✗ ${m.broken}`, { fg: t.error }]])
    else if (!m.enabled) kv(c, a.x, y++, a.w, 'Status', [['off', { fg: t.dim }], ['  not loaded', { fg: t.muted }]])
    if (m.hit) {
      y++
      y += para(c, rect(a.x, y, a.w, 4), m.hit.description, { fg: t.text }, 4)
    }
    if (m.broken) para(c, rect(a.x, y + 1, a.w, 2), `Updating ${m.broken.replace(/^needs /, '')} fixes it.`, { fg: t.dim })
    else if (m.update && !m.queued) hint(c, a.x, y + 1, [['↵', m.fixes ? `updates to ${m.update}, which fixes ${m.fixes}` : `updates to ${m.update}`]], a.w)
    return
  }
  if (ref.k === 'hit') {
    const h = ref.h
    title(c, r, h.name)
    line([[`by ${h.author} · ${h.category} · ${h.downloads}`, { fg: t.dim }]])
    y++
    y += para(c, rect(a.x, y, a.w, 4), h.description, { fg: t.text }, 4)
    y++
    line([[`${h.side} · updated ${h.updated}`, { fg: t.muted }]])
    y++
    hint(c, a.x, y, [['↵', `installs it for ${inst.mc} · ${inst.loader}`]], a.w)
    return
  }
  if (ref.k === 'add') {
    const what = ref.what === 'mods' ? 'Popular mods' : 'Popular packs'
    title(c, r, 'Modrinth', { right: [[`${inst.mc} · ${inst.loader}`, { fg: t.faint }]] })
    line([[what, { fg: t.dim }]])
    y++
    const names = new Set(inst.mods.map((m) => m.name))
    const hits = data.modHits.filter((h) => !names.has(h.name)).slice(0, Math.max(0, a.h - 5))
    if (ref.what === 'mods')
      hits.forEach((h) => {
        c.buf.text(a.x, y, h.name, { fg: t.subtle }, a.w - 8)
        c.buf.lineRight(rect(a.x, y, a.w, 1), [[h.downloads, { fg: t.muted }]])
        y++
      })
    else ['Fresh Animations', 'Faithful 32x', 'Stay True', 'Vanilla Tweaks'].forEach((p) => c.buf.text(a.x, y++, p, { fg: t.subtle }, a.w))
    y++
    hint(c, a.x, y, [['/', `search ${ref.what === 'mods' ? data.modTotal + ' mods' : 'packs'}`]], a.w)
    return
  }
  if (ref.k === 'pack') {
    title(c, r, ref.name)
    kv(c, a.x, y++, a.w, 'Status', [['on', { fg: t.text }]])
    kv(c, a.x, y++, a.w, 'Order', [`${ref.i + 1} of ${ref.n}`, [ref.i === 0 ? '  wins conflicts' : '', { fg: t.muted }]])
    return
  }
  const s = ref.s
  title(c, r, s.label)
  line([[s.value, { fg: t.strong, bold: true }]])
  y++
  y += para(c, rect(a.x, y, a.w, 4), s.about, { fg: t.dim }, 4) + 1
  if (s.choices) {
    let x = a.x
    s.choices.forEach((ch) => {
      const on = ch === s.value
      if (x + width(ch) > right(a)) return
      x = c.buf.text(x, y, ch, on ? { fg: t.accent, bold: true } : { fg: t.muted }) + 3
    })
  }
}

// ── Content previews ──────────────────────────────────────────────────────

/** The log in a preview column: why it crashed (if it did), then the tail. */
export function logPreview(c: Ctx, r: Rect, inst: Instance) {
  const { t, data, buf } = c
  title(c, r, 'Game log', { right: inst.lastPlayed || st0(c, inst) ? [[`${logOf(c, inst).length} lines`, { fg: t.faint }]] : undefined })
  const a = rect(r.x + 1, r.y + 2, r.w - 2, r.h - 2)
  const st = stateOf(c, inst)
  let y = a.y
  if (!inst.lastPlayed && st.phase !== 'running') {
    buf.text(a.x, y, 'Empty until the first time you play.', { fg: t.muted }, a.w)
    return
  }
  if (st.phase === 'crashed') {
    const why = st.fresh ? data.crash!.reason.split(' — ').pop()! : 'The game stopped with an error the last time it ran.'
    y += para(c, rect(a.x, y, a.w, 3), why, { fg: t.strong, bold: true }, 3)
    buf.text(a.x, y++, st.fresh ? `Exited with ${data.crash!.exit}, ${data.crash!.when}.` : 'Nothing has changed since.', { fg: t.dim }, a.w)
    y++
    hint(c, a.x, y, st.fresh ? [['m', 'fix mods'], ['c', 'copy report']] : [['↵', 'try again'], ['c', 'copy report']], a.w)
    y += 2
  }
  // A preview is a glance: the last few lines, not a wall. → opens the rest.
  logLines(c, rect(a.x, y, a.w, Math.min(bottom(a) - y, Math.max(10, Math.floor(a.h / 2)))), logOf(c, inst), '', true)
}

const st0 = (c: Ctx, inst: Instance) => stateOf(c, inst).phase === 'running'

/** Log lines, newest at the bottom; `short` drops the seconds for narrow columns. */
export function logLines(c: Ctx, r: Rect, lines: ReturnType<typeof logOf>, query: string, short: boolean) {
  const { t, buf } = c
  if (r.h <= 0) return
  const shown = query ? lines.filter((l) => l.text.toLowerCase().includes(query.toLowerCase())) : lines
  const fit = shown.length > r.h ? r.h - 1 : r.h
  const tail = shown.slice(-fit)
  let y = r.y
  if (shown.length > fit) buf.text(r.x, y++, `↑ ${shown.length - fit} earlier`, { fg: t.faint })
  tail.forEach((l) => {
    const time = short ? l.time.slice(0, 5) : l.time
    let x = buf.text(r.x, y, time, { fg: t.faint })
    x = buf.text(x + 1, y, logMark(l), { fg: logFg(c, l) }) + 1
    const st = { fg: logFg(c, l) }
    if (query) {
      const at = l.text.toLowerCase().indexOf(query.toLowerCase())
      const hits = [...Array(query.length).keys()].map((k) => at + k)
      const set = new Set(hits)
      let cx = x
      ;[...l.text].forEach((ch, i) => {
        if (cx >= right(r)) return
        buf.set(cx++, y, ch, set.has(i) ? { fg: t.accent, bold: true } : st)
      })
      if (width(l.text) > right(r) - x) buf.set(right(r) - 1, y, '…', st)
    } else buf.text(x, y, l.text, st, right(r) - x)
    y++
  })
  if (!shown.length) buf.text(r.x, r.y, `No lines with “${query}”.`, { fg: t.muted }, r.w)
}

/** A first launch fetching the game. */
export function downloadPreview(c: Ctx, r: Rect, inst: Instance, wideBar = false) {
  const { t, buf } = c
  const task = stateOf(c, inst).task!
  title(c, r, 'Download', { right: [[`${Math.round(task.ratio * 100)}%`, { fg: t.text }]] })
  const a = rect(r.x + 1, r.y + 2, r.w - 2, r.h - 2)
  let y = a.y
  buf.text(a.x, y++, task.label.replace(/^Downloading /, ''), { fg: t.strong, bold: true }, a.w)
  y++
  const w = Math.min(a.w, wideBar ? 48 : 30)
  bar(c, a.x, y++, w, task.ratio)
  task.detail.split(' · ').forEach((d) => buf.text(a.x, ++y, d, { fg: t.dim }, a.w))
  y += 2
  para(c, rect(a.x, y, a.w, 2), '↵ plays as soon as it’s ready. You can leave mctui open or not; it keeps going.', { fg: t.dim })
}

/** The `+ New instance` row's preview: what ↵ ↵ ↵ would make. On first run, what an instance is. */
export function newPreview(c: Ctx, r: Rect, firstRun: boolean) {
  const { t, data } = c
  const latest = data.versions.find((v) => v.latest)!
  title(c, r, 'New instance')
  const a = rect(r.x + 1, r.y + 2, Math.min(r.w - 2, 60), r.h - 2)
  let y = a.y
  if (firstRun) {
    c.buf.text(a.x, y++, 'No instances yet.', { fg: t.strong, bold: true }, a.w)
    y += para(c, rect(a.x, y, a.w, 3), 'An instance is one Minecraft version with its own worlds, mods and settings.', { fg: t.dim })
    y++
  }
  kv(c, a.x, y++, a.w, 'Version', [[latest.id, { fg: t.text }], ['  latest release', { fg: t.muted }]], 10)
  kv(c, a.x, y++, a.w, 'Loader', [['Fabric 0.17.2', { fg: t.text }], ['  so mods work', { fg: t.muted }]], 10)
  kv(c, a.x, y++, a.w, 'Name', [[`${latest.id} Fabric`, { fg: t.text }]], 10)
  y++
  para(c, rect(a.x, y, a.w, 2), '↵ to choose; ↵ ↵ ↵ ↵ takes these.', { fg: t.dim })
}
