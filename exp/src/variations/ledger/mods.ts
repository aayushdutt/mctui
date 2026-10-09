// Mods: the instance's installed mods as a table, outdated first. `/` turns
// the same table into Modrinth results; esc turns it back. The drawer shows
// the selected mod.

import type { Ctx } from '../../kit'
import type { InstalledMod, ModHit } from '../../data/mock'
import { type Line, type Style, rect, wrap } from '../../term/buffer'
import { fuzzy } from './fuzzy'
import { factsOf, hitsOf, modsOf, sideText, UPDATE_NOTES } from './data'
import { type Col, type Entry, type Hint, body, fit, footer, frame, head, ledger, lit, moreTail, prompt, rule, topLine } from './ui'
import { overlay } from './overlays'

interface Hit {
  hit: ModHit
  hits?: number[]
  score: number
}

export function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  const H = buf.rows
  const inst = data.instances.find((i) => i.loader === 'Fabric') ?? data.instances[0]
  // Mods belong to an instance; with none (first run) there's nothing to show.
  if (!inst) {
    topLine(c, f, ['mods'])
    buf.text(f.x, f.head, 'No instances yet — mods live inside one. esc goes back.', { fg: t.muted })
    return
  }
  const searching = ui.typing || !!ui.query
  const installed = modsOf(inst)
  // The mod the last crash asked for, if any.
  const needed = data.crash?.instance === inst.name ? 'Fabric API' : null

  const mine = [...installed].sort(
    (a, b) =>
      Number(b.name === needed) - Number(a.name === needed) ||
      Number(!!b.update) - Number(!!a.update) ||
      Number(b.enabled) - Number(a.enabled) ||
      a.name.localeCompare(b.name),
  )
  const found: Hit[] = []
  hitsOf(data).forEach((hit, i) => {
    const m = fuzzy(ui.query, hit.name)
    if (m) found.push({ hit, hits: m.hits, score: ui.query ? m.score : -i })
  })
  found.sort((a, b) => b.score - a.score)

  const n = searching ? found.length : mine.length
  const sel = c.sel(Math.max(1, n))

  // ── Top line ──
  const ups = installed.filter((m) => m.update).length
  const off = installed.filter((m) => !m.enabled).length
  topLine(
    c,
    f,
    searching ? [inst.name, 'mods', 'Modrinth'] : [inst.name, 'mods'],
    searching
      ? [[`for Minecraft ${inst.mc} with Fabric`, { fg: t.muted }]]
      : [
          [`${installed.length} installed`, { fg: t.muted }],
          [ups ? `  ·  ${ups} updates` : '', { fg: t.muted }],
          [off ? `  ·  ${off} off` : '', { fg: t.muted }],
        ],
  )

  // ── Geometry: the drawer hangs under the table ──
  const lastRow = f.foot - 2
  const dMin = H >= 36 ? 8 : H >= 28 ? 6 : 5
  const dMax = H >= 36 ? 9 : 7
  const tableRows = Math.max(1, Math.min(n, lastRow - dMin - 1 - f.body))
  const ruleY = f.body + tableRows + 1
  const drawer = rect(f.x, ruleY + 1, f.w, Math.min(lastRow, ruleY + dMax) - ruleY)
  const gap = 2

  let tail: Line = []
  let d: { title: Line; entries: Entry[] } | null = null
  if (!searching) {
    const placed = fit(installedCols(c, needed), f.x, f.w, gap, 5)
    head(c, f.head, placed)
    const { above, below } = body(c, f, f.body, tableRows, placed, mine, sel)
    tail = moreTail(c, above, below)
    if (mine[sel]) d = installedDrawer(c, mine[sel], inst.name, needed, drawer.w)
  } else {
    const placed = fit(hitCols(c), f.x, f.w, gap, 5)
    head(c, f.head, placed)
    if (!found.length) buf.text(f.x, f.body, `Nothing on Modrinth matches “${ui.query}”.`, { fg: t.dim })
    else {
      const { above, below } = body(c, f, f.body, tableRows, placed, found, sel)
      tail = moreTail(c, above, below)
    }
    if (found[sel]) d = hitDrawer(c, found[sel].hit, inst.name, drawer.w)
  }
  if (d && !ui.overlay) {
    rule(c, f, ruleY, d.title, tail)
    ledger(c, drawer, d.entries)
  }

  // ── Footer ──
  const cur = mine[sel]
  if (ui.typing) prompt(c, f, '/', ui.query, 'search Modrinth', `${found.length} of ${data.modTotal}`, [['↵', 'pick'], ['esc', 'installed']])
  else if (searching) {
    const h = found[sel]?.hit
    footer(c, f, [[h?.installed ? 'd' : '↵', h?.installed ? 'remove' : 'install'], ['o', 'page'], ['/', 'search'], ['esc', 'installed'], ['?', 'help']])
  } else {
    const hints: Hint[] = cur?.update ? [['↵', 'update'], ...(ups > 1 ? ([['U', 'update all']] as Hint[]) : [])] : []
    hints.push(['space', cur?.enabled === false ? 'turn on' : 'turn off'], ['d', 'remove'], ['/', 'add from Modrinth'], ['esc', 'back'], ['?', 'help'])
    footer(c, f, hints)
  }
  c.enter(null)
  if (ui.overlay) overlay(c, f, ruleY, drawer, 'mods', inst, null)
}

function installedCols(c: Ctx, needed: string | null): Col<InstalledMod>[] {
  const { t, data } = c
  const text = (m: InstalledMod, on: boolean): Style => ({ fg: !m.enabled ? t.muted : on ? t.subtle : t.dim })
  return [
    {
      key: 'name',
      label: 'NAME',
      w: 22,
      min: 14,
      flex: true,
      max: 26,
      cell: (m, w, on) => lit(c, m.name, undefined, !m.enabled ? { fg: t.muted } : on ? { fg: t.strong, bold: true } : { fg: t.text }, w),
    },
    { key: 'version', label: 'VERSION', w: 9, cell: (m, _w, on) => [[m.version, text(m, on)]] },
    {
      key: 'update',
      label: 'UPDATE',
      w: 10,
      cell: (m) => (m.update ? [[`↑ ${m.update}`, { fg: m.name === needed ? t.error : t.warning }]] : []),
    },
    { key: 'category', label: 'CATEGORY', w: 12, drop: 3, cell: (m, _w, on) => [[factsOf(data, m.name)?.category.toLowerCase() ?? '', text(m, on)]] },
    { key: 'side', label: 'SIDE', w: 15, min: 6, drop: 2, cell: (m, w, on) => [[w >= 15 ? sideText(factsOf(data, m.name)?.side) : sideText(factsOf(data, m.name)?.side).replace('client & server', 'both'), text(m, on)]] },
    { key: 'enabled', label: 'ENABLED', w: 7, spacer: true, cell: (m, _w, on) => [[m.enabled ? 'yes' : 'no', m.enabled ? text(m, on) : { fg: t.subtle }]] },
  ]
}

function hitCols(c: Ctx): Col<Hit>[] {
  const { t } = c
  const sec = (on: boolean): Style => ({ fg: on ? t.subtle : t.dim })
  return [
    {
      key: 'name',
      label: 'NAME',
      w: 22,
      min: 14,
      flex: true,
      max: 26,
      cell: (h, w, on) => lit(c, h.hit.name, h.hits, on ? { fg: t.strong, bold: true } : { fg: t.text }, w),
    },
    { key: 'author', label: 'AUTHOR', w: 13, min: 10, drop: 3, cell: (h, w, on) => lit(c, h.hit.author, undefined, sec(on), w) },
    { key: 'category', label: 'CATEGORY', w: 13, drop: 2, cell: (h, _w, on) => [[h.hit.category.toLowerCase(), sec(on)]] },
    { key: 'downloads', label: 'DOWNLOADS', w: 9, right: true, cell: (h, _w, on) => [[h.hit.downloads, sec(on)]] },
    { key: 'installed', label: '', w: 11, spacer: true, cell: (h) => (h.hit.installed ? [['✓ installed', { fg: t.success }]] : []) },
  ]
}

const keyCap = (c: Ctx, k: string): [string, Style] => [k, { fg: c.t.text, bold: true }]
const say = (c: Ctx, s: string): [string, Style] => [s, { fg: c.t.muted }]
const jar = (m: InstalledMod) => `mods/${m.name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${m.version}.jar`

/** The description at a readable measure, two lines at most (an ellipsis if cut). */
function about(c: Ctx, text: string, w: number, rows = 2): Entry[] {
  const lines = wrap(text, Math.min(w - 10, 84))
  const shown = lines.slice(0, rows)
  if (lines.length > rows) shown[rows - 1] = shown[rows - 1].replace(/\s*\S*$/, '') + '…'
  return shown.map((l, i) => ({ label: i ? undefined : 'about', line: [[l, { fg: c.t.text }]] }))
}

function installedDrawer(c: Ctx, m: InstalledMod, instName: string, needed: string | null, w: number) {
  const { t, data } = c
  const facts = factsOf(data, m.name)
  const title: Line = [[m.name, { fg: t.strong, bold: true }]]
  if (m.update) title.push([`  ${m.version} → ${m.update}`, { fg: m.name === needed ? t.error : t.warning }])
  else title.push([`  ${m.version}`, { fg: t.dim }])
  const entries: Entry[] = []
  if (m.name === needed)
    entries.push({
      label: 'needed',
      labelFg: t.error,
      line: [['Sodium 0.6.0 needs 0.112 or newer; the last launch crashed without it', { fg: t.text }], ['   ', {}], keyCap(c, '↵'), say(c, ' updates it')],
    })
  else if (m.update)
    entries.push({
      label: 'update',
      line: [[`${m.update} ${UPDATE_NOTES[m.name] ?? 'is out'}`, { fg: t.text }], ['   ', {}], keyCap(c, '↵'), say(c, ' updates it')],
    })
  if (!m.enabled)
    entries.push({ label: 'off', labelFg: t.warning, line: [[`turned off in ${instName}; the game skips it`, { fg: t.text }], ['   ', {}], keyCap(c, 'space'), say(c, ' turns it on')] })
  if (facts) {
    entries.push(...about(c, facts.description, w))
    entries.push({
      label: 'by',
      line: [[facts.author, { fg: t.text }], [`  ·  ${facts.downloads} downloads  ·  updated ${facts.updated} on Modrinth`, { fg: t.dim }]],
    })
  }
  entries.push({ label: 'file', line: [[jar(m), { fg: t.dim }]] })
  return { title, entries }
}

function hitDrawer(c: Ctx, h: ModHit, instName: string, w: number) {
  const { t } = c
  const title: Line = [[h.name, { fg: t.strong, bold: true }], [`  by ${h.author}`, { fg: t.muted }]]
  const entries: Entry[] = [...about(c, h.description, w)]
  entries.push({
    label: 'facts',
    line: [[`${h.category.toLowerCase()}  ·  ${sideText(h.side)}  ·  ${h.downloads} downloads  ·  updated ${h.updated}`, { fg: t.dim }]],
  })
  if (h.installed) entries.push({ label: 'added', line: [['✓ ', { fg: t.success }], [`in ${instName}`, { fg: t.text }], ['   ', {}], keyCap(c, 'd'), say(c, ' removes it')] })
  else
    entries.push({
      label: 'install',
      line: [keyCap(c, '↵'), say(c, ` adds it to ${instName}`), [h.name === 'Mod Menu' || h.name === 'Iris Shaders' ? '  ·  needs Fabric API, installed' : '', { fg: t.dim }]],
    })
  return { title, entries }
}
