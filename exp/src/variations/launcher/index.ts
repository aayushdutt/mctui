// Launcher: play-first, like a game launcher's front page. Home is a stage for
// one instance (its name set large, one big Play) over a shelf of the rest.

import { defineVariation, type Ctx } from '../../kit'
import { type Color, type Line, type Rect, type Style, rect, right, bottom, width, lineWidth, wrap } from '../../term/buffer'
import { mix, onColor } from '../../term/theme'
import * as w from '../../term/widgets'
import type { Instance, InstalledMod, ModHit } from '../../data/mock'
import { FONT_XL, FONT_L, FONT_S, type Font, drawBig, fit, measure, spaced } from './font'

// ── pieces ──────────────────────────────────────────────────────────────────

/** Faces for a stage title, largest first. */
const STAGE = [FONT_XL, FONT_L, FONT_S]

/** Rows a stage needs besides its title: gap, subtitle, gap, action, gap, facts ×2. */
const STAGE_BODY = 7

/** The faces whose height leaves room for the rest of a stage `h` rows tall. */
const facesFor = (h: number, faces: Font[] = STAGE) => faces.filter((f) => f.rows + STAGE_BODY <= h)

/**
 * Row shades for display type: strong at the top, leaning into the accent. Any
 * other tone (red for danger) just dims a little towards the bottom.
 */
function shades(c: Ctx, f: Font, tone: Color = c.t.strong): Color[] {
  const lean = tone === c.t.strong ? c.t.accent : c.t.bg
  const depth = tone === c.t.strong ? 0.6 : 0.3
  return Array.from({ length: f.rows }, (_, r) => mix(tone, lean, f.rows > 1 ? (depth * r) / (f.rows - 1) : 0))
}

/**
 * A name set as big as the room allows (XL, L, S, then letterspaced capitals).
 * Returns the rows it took.
 */
function title(c: Ctx, x: number, y: number, text: string, maxW: number, faces: Font[] = STAGE, tone?: Color): number {
  const f = fit(text, maxW, faces)
  if (f) {
    drawBig(c.buf, x, y, text, f, shades(c, f, tone))
    return f.rows
  }
  c.buf.text(x, y, spaced(text), { fg: tone ?? c.t.strong, bold: true }, maxW)
  return 1
}

const titleRows = (text: string, maxW: number, faces: Font[] = STAGE) => fit(text, maxW, faces)?.rows ?? 1

/** `Minecraft 1.21.4  ·  Fabric 0.16.10  ·  4h 12m played`. */
function dotted(c: Ctx, parts: (string | [string, Style])[]): Line {
  const out: Line = []
  parts.forEach((p, i) => {
    if (i) out.push(['  ·  ', { fg: c.t.faint }])
    out.push(typeof p === 'string' ? [p, { fg: c.t.dim }] : p)
  })
  return out
}

type Kind = 'primary' | 'danger' | 'quiet'

/** The one big action: `   ▶  PLAY        ↵  ` on the accent, one row. */
function bigButton(c: Ctx, x: number, y: number, glyph: string, label: string, region: string, kind: Kind = 'primary', key = '↵'): Rect {
  const { t, buf } = c
  const hovered = c.hovered(region)
  const bg =
    kind === 'primary' ? (hovered ? t.accentHi : t.accent) : kind === 'danger' ? t.error : hovered ? t.surfaceHi : t.surface
  const fg = kind === 'quiet' ? (hovered ? t.strong : t.text) : onColor(bg)
  const keyFg = kind === 'quiet' ? t.muted : mix(fg, bg, 0.45)
  const line: Line = ['   ', [glyph, { fg, bold: true }], '  ', [label, { fg, bold: true }], ' '.repeat(Math.max(3, 12 - width(label))), [key || ' ', { fg: keyFg, bold: true }], '  ']
  const r = rect(x, y, lineWidth(line), 1)
  buf.fill(r, { bg })
  buf.line(x, y, line, { fg })
  buf.region(r, region)
  return r
}

/** Steam-style facts: a small-caps label over a value, side by side. Returns the width used. */
function facts(c: Ctx, x: number, y: number, items: [string, Line][], colW: number, maxW: number): number {
  const { t, buf } = c
  let cx = x
  let used = 0
  for (const [label, value] of items) {
    if (cx - x + Math.min(colW, Math.max(width(label), lineWidth(value))) > maxW) break
    buf.text(cx, y, label.toUpperCase(), { fg: t.muted, bold: true }, colW - 2)
    buf.line(cx, y + 1, value, { fg: t.text }, Math.min(colW - 2, x + maxW - cx))
    used = cx - x + Math.max(width(label), lineWidth(value))
    cx += colW
  }
  return used
}

interface ShelfItem {
  name: string
  meta: Line
  /** A quieter name (the “+ New” tile). */
  ghost?: boolean
}

/**
 * The shelf: tiles standing on a thin plank, the current one lit from below.
 * Scrolls to keep the selection in view, with `‹ ›` when more are off-screen.
 * Three rows; `lit` is false while another pane has focus.
 */
function shelf(c: Ctx, r: Rect, items: ShelfItem[], sel: number, lit: boolean, minTile = 20, prefix = 'sel') {
  const { t, buf } = c
  const inner = rect(r.x + 2, r.y, r.w - 4, 3)
  const fitN = Math.max(1, Math.min(items.length, Math.floor((inner.w + 2) / minTile)))
  const tileW = Math.floor((inner.w + 2) / fitN)
  const { start, shown } = w.scroll(items, sel, fitN)
  const plankY = r.y + 2
  buf.hline(r.x, plankY, r.w, { fg: t.rule }, '▔')
  shown.forEach((it, j) => {
    const i = start + j
    const x = inner.x + j * tileW
    const tw = tileW - 2
    const on = i === sel
    const hov = c.hovered(`${prefix}:${i}`)
    const nameSt: Style = on
      ? { fg: lit ? t.strong : t.text, bold: true }
      : { fg: hov ? t.text : it.ghost ? t.muted : t.dim }
    buf.text(x, r.y, it.name, nameSt, tw)
    buf.line(x, r.y + 1, it.meta, { fg: on ? t.dim : t.faint }, tw)
    if (on) buf.hline(x, plankY, tw, { fg: lit ? t.accent : t.muted }, '▀')
    else if (hov) buf.hline(x, plankY, tw, { fg: t.muted }, '▔')
    buf.region(rect(x, r.y, tw, 3), `${prefix}:${i}`)
  })
  const arrow = (x: number, glyph: string, key: string) => {
    buf.text(x, r.y, glyph, { fg: c.hovered(key) ? t.strong : t.subtle, bold: true })
    buf.region(rect(x, r.y, 1, 2), key)
  }
  if (lit && start > 0) arrow(r.x, '‹', 'key:↑')
  if (lit && start + shown.length < items.length) arrow(right(r) - 1, '›', 'key:↓')
  if (!lit && start > 0) buf.text(r.x, r.y, '‹', { fg: t.faint })
  if (!lit && start + shown.length < items.length) buf.text(right(r) - 1, r.y, '›', { fg: t.faint })
}

/** Footer keycaps, dropping the least useful (listed in `drop`) until they fit. */
function footer(c: Ctx, r: Rect, hints: [string, string][], drop: string[] = []) {
  const need = (hs: [string, string][]) => hs.reduce((n, [k, l]) => n + width(k) + 2 + 1 + width(l) + 3, -3)
  let hs = hints
  for (const k of drop) {
    if (need(hs) <= r.w) break
    hs = hs.filter(([key]) => key !== k)
  }
  w.keycaps(c, r.x, r.y, hs, r.w)
}

/**
 * A card over the dimmed page, headed in letterspaced capitals. It clears whole
 * rows, a letterbox band across the screen, so no scraps of the page crowd it.
 */
function sheet(c: Ctx, wd: number, h: number, heading: string, opts: { danger?: boolean; hint?: string } = {}): Rect {
  const { t, buf } = c
  buf.fade(t.backdrop)
  buf.region(buf.area(), 'close')
  wd = Math.min(wd, buf.cols - 8)
  h = Math.min(h, buf.rows - 4)
  const r = rect(Math.floor((buf.cols - wd) / 2), Math.floor((buf.rows - h) / 2), wd, h)
  const band = rect(0, r.y - 2, buf.cols, r.h + 4)
  buf.clear(band)
  buf.hline(0, band.y, buf.cols, { fg: t.rule }, '▁')
  buf.hline(0, bottom(band) - 1, buf.cols, { fg: t.rule }, '▔')
  buf.text(r.x, r.y, spaced(heading), { fg: opts.danger ? t.error : t.accent, bold: true }, r.w)
  if (opts.hint) buf.lineRight(rect(r.x, r.y, r.w, 1), [[opts.hint, { fg: t.muted }]])
  buf.hline(r.x, r.y + 1, r.w, { fg: t.rule }, '▔')
  buf.region(r, 'card')
  return rect(r.x, r.y + 3, r.w, r.h - 3)
}

const loaderName = (i: Instance) => (i.loader === 'Vanilla' ? 'Vanilla' : `${i.loader} ${i.loaderVersion}`)

function instanceTiles(c: Ctx): ShelfItem[] {
  const { t, data } = c
  const tiles: ShelfItem[] = data.instances.map((i) => ({
    name: i.name,
    meta: i.trouble
      ? [['▲ crashed', { fg: t.warning }], ['  ' + i.mc, {}]]
      : i.fresh
        ? [['● new', { fg: t.secondary }], ['  ' + i.mc, {}]]
        : [[i.lastPlayed ?? '', {}], ['  ' + i.mc, {}]],
  }))
  tiles.push({ name: '+ New instance', meta: [['pick a version', {}]], ghost: true })
  return tiles
}

/** A chunkier spinner than braille, which not every font has. */
function spin(c: Ctx) {
  c.spinner()
  return ['◐', '◓', '◑', '◒'][c.ui.tick % 4]
}

/** A hint like `d  remove`: the key in the accent, the label dim. Returns the end x. */
function hint(c: Ctx, x: number, y: number, key: string, label: string, region?: string): number {
  const { t, buf } = c
  const hov = !!region && c.hovered(region)
  const end = buf.line(x, y, [[key, { fg: t.accent, bold: true }], [`  ${label}`, { fg: hov ? t.text : t.dim }]])
  if (region) buf.region(rect(x, y, end - x, 1), region)
  return end
}

// ── home ────────────────────────────────────────────────────────────────────

function home(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  w.header(c, p.header)
  c.panes(1)
  const insts = data.instances
  const tiles = instanceTiles(c)
  const sel = c.sel(tiles.length, { across: true })
  const creating = sel === insts.length
  const inst = insts[Math.min(sel, insts.length - 1)]
  c.enter(creating ? 'wizard' : 'play')

  // The shelf stands on the floor of the body; the stage fills what's above.
  const shelfR = rect(p.body.x, bottom(p.body) - 3, p.body.w, 3)
  const labelY = shelfR.y - 2
  w.section(c, p.body.x + 2, labelY, 'Library', insts.length)

  const stage = rect(p.body.x + 2, p.body.y, p.body.w - 4, labelY - 1 - p.body.y)
  const name = creating ? 'New instance' : inst.name
  const faces = facesFor(stage.h)
  const rows = titleRows(name, stage.w, faces)
  const top = stage.y + Math.max(0, Math.floor((stage.h - rows - STAGE_BODY) / 2))
  title(c, stage.x, top, name, stage.w, faces)

  const subY = top + rows + 1
  const pillY = subY + 2
  const factY = pillY + 2
  let leftW: number
  if (creating) {
    buf.line(stage.x, subY, [['Pick a Minecraft version, a loader and a name.', { fg: t.dim }]], {}, stage.w)
    const b = bigButton(c, stage.x, pillY, '+', 'CREATE', 'screen:wizard')
    const latest = data.versions.find((v) => v.latest)!
    const snap = data.versions.find((v) => v.snapshot)!
    leftW = Math.max(
      b.w,
      facts(
        c,
        stage.x,
        factY,
        [
          ['Latest release', [[latest.id, {}], [`  ${latest.date}`, { fg: t.dim }]]],
          ['Latest snapshot', [[snap.id, {}]]],
          ['Loaders', [['Vanilla, Fabric', {}]]],
        ],
        24,
        stage.w,
      ),
    )
  } else {
    const sub = dotted(c, [`Minecraft ${inst.mc}`, loaderName(inst), inst.fresh ? 'never played' : `${inst.playtime} played`])
    buf.line(stage.x, subY, sub, {}, stage.w)
    const b = bigButton(c, stage.x, pillY, '▶', 'PLAY', 'key:↵')
    let nx = right(b)
    if (inst.trouble) {
      nx = right(w.chip(c, nx + 3, pillY, [['▲ Crashed last time', { fg: t.warning }]], 'warning'))
      nx = hint(c, nx + 2, pillY, 'l', 'why', 'why')
    } else if (inst.fresh) {
      nx = buf.text(nx + 3, pillY, 'First launch downloads the game', { fg: t.muted })
    }
    const updates = inst.mods.filter((m) => m.update).length
    const content: [string, Line] =
      inst.loader === 'Vanilla'
        ? ['Resource packs', [[inst.packs.length ? String(inst.packs.length) : '—', {}]]]
        : ['Mods', [[String(inst.mods.length), {}], ...(updates ? ([[`  ↑ ${updates} update`, { fg: t.secondary }]] as Line) : [])]]
    const fw = facts(
      c,
      stage.x,
      factY,
      [
        ['Last played', [[inst.lastPlayed ?? 'never', {}]]],
        ['Worlds', [[inst.worlds.length ? String(inst.worlds.length) : '—', {}], [inst.worlds[0] ? `  ${inst.worlds[0].name}` : '', { fg: t.dim }]]],
        content,
      ],
      18,
      stage.w,
    )
    leftW = Math.max(nx - stage.x, fw)
  }

  // What's new: a quiet corner beside the action, only when there's room.
  const newsW = 32
  if (right(p.body) - newsW - (stage.x + leftW) >= 6) {
    const nx = right(p.body) - newsW
    buf.text(nx, pillY, "WHAT'S NEW", { fg: t.muted, bold: true })
    data.news.forEach((n, i) => {
      const r = rect(nx, factY + i, newsW, 1)
      const hov = c.hovered(`news:${i}`)
      buf.line(r.x, r.y, [['● ', { fg: i === 0 ? t.secondary : t.faint }], [n.title, { fg: hov ? t.strong : t.text }]], {}, newsW - 8)
      buf.lineRight(r, [[n.date, { fg: t.dim }]])
      buf.region(r, `news:${i}`)
    })
  }

  shelf(c, shelfR, tiles, sel, true)

  footer(
    c,
    p.footer,
    creating
      ? [['↵', 'create'], ['↑↓', 'browse'], [':', 'commands'], ['?', 'help']]
      : [['↵', 'play'], ['↑↓', 'browse'], ['o', 'offline'], ['m', 'mods'], ['n', 'new'], [':', 'commands'], ['?', 'help']],
    ['n', 'o', ':'],
  )
  overlays(c, creating ? null : inst)
}

// ── mods ────────────────────────────────────────────────────────────────────

function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  const inst = data.instances[0]
  w.header(c, p.header, [inst.name, 'Mods'])
  const focus = c.panes(2)
  c.enter(null)

  // Installed mods stand on the shelf at the bottom, like home's library.
  const shelfR = rect(p.body.x, bottom(p.body) - 3, p.body.w, 3)
  const labelY = shelfR.y - 2
  const updates = inst.mods.filter((m) => m.update).length
  const lx = w.section(c, p.body.x + 2, labelY, `On ${inst.name}`, inst.mods.length, focus === 1)
  if (updates) buf.text(lx + 3, labelY, `↑ ${updates} update`, { fg: t.secondary })

  const hits = data.modHits.filter((h) => h.name.toLowerCase().includes(ui.query.toLowerCase()))
  const sel = c.sel(focus === 0 ? Math.max(1, hits.length) : inst.mods.length)

  // Results: a quiet list on the left, led by the search field.
  const listW = Math.min(40, Math.max(28, Math.floor(p.body.w * 0.38)))
  const list = rect(p.body.x, p.body.y, listW, labelY - 1 - p.body.y)
  const count = list.w >= 36 ? `${hits.length} of ${data.modTotal}` : String(hits.length)
  w.search(c, rect(list.x, list.y, list.w, 1), ui.query, 'Search Modrinth', count, ui.typing)
  const area = rect(list.x, list.y + 2, list.w, list.h - 2)
  // Roomy terminals get a line of description under each name.
  const stride = hits.length * 3 - 1 <= area.h ? 3 : 2
  const all = Math.floor((area.h + 1) / stride)
  const fitN = Math.max(1, hits.length > all ? Math.floor((area.h - 1) / stride) : all)
  const { start, shown } = w.scroll(hits, focus === 0 ? sel : 0, fitN)
  shown.forEach((h, j) => {
    const i = start + j
    const r = rect(area.x, area.y + j * stride, area.w, 1)
    const hov = c.hovered(`sel:${i}`)
    const on = focus === 0 && sel === i
    const inner = w.band(c, r, on, hov)
    const acc: Line = h.installed ? [['✓', { fg: t.success }]] : [[h.downloads, { fg: on ? t.subtle : t.dim }]]
    const ax = buf.lineRight(inner, acc)
    buf.text(inner.x, r.y, h.name, w.rowLabel(c, on, hov), ax - inner.x - 2)
    if (stride === 3) buf.text(inner.x, r.y + 1, h.description, { fg: on ? t.dim : t.faint }, inner.w)
    buf.region(rect(r.x, r.y, r.w, stride - 1), focus === 0 ? `sel:${i}` : 'focus:0')
  })
  if (!hits.length) buf.text(area.x + 2, area.y, `Nothing called “${ui.query}”.`, { fg: t.faint }, area.w - 2)
  w.more(c, area.x + 2, area.y + shown.length * stride, hits.length - start - shown.length)

  // The stage: whatever's selected, set large.
  const stage = rect(right(list) + 4, p.body.y, right(p.body) - right(list) - 6, list.h)
  const installed = focus === 1 ? inst.mods[sel] : undefined
  const hit: ModHit | undefined = installed ? data.modHits.find((h) => h.name === installed.name) : hits[sel]
  if (installed || hit) {
    const name = installed?.name ?? hit!.name
    const faces = facesFor(stage.h - 3, [FONT_L, FONT_S])
    const rows = title(c, stage.x, stage.y, name, stage.w, faces)
    let y = stage.y + rows + 1
    const by: (string | [string, Style])[] = hit ? [`by ${hit.author}`, hit.category, hit.side] : ['Added by hand']
    buf.line(stage.x, y, dotted(c, by), {}, stage.w)
    y += 2
    const room = Math.max(0, Math.min(3, stage.h - rows - 8))
    const lines = hit ? wrap(hit.description, Math.min(stage.w, 64)) : []
    const desc = lines.slice(0, room)
    if (lines.length > room && room) desc[room - 1] = desc[room - 1].replace(/\s*\S*$/, '…')
    desc.forEach((l, i) => buf.text(stage.x, y + i, l, { fg: t.text }))
    y += desc.length + 1
    if (installed) stageInstalled(c, stage, y, installed)
    else stageHit(c, stage, y, hit!, inst)
  }

  shelf(
    c,
    shelfR,
    inst.mods.map((m) => ({
      name: m.name,
      meta: m.update ? [[m.version, {}], [`  ↑ ${m.update}`, { fg: t.secondary }]] : [[m.version, {}]],
    })),
    focus === 1 ? sel : -1,
    focus === 1,
    22,
  )
  if (focus !== 1) buf.region(shelfR, 'focus:1')

  footer(
    c,
    p.footer,
    focus === 0
      ? [hit?.installed ? ['d', 'remove'] : ['↵', 'install'], ['/', 'search'], ['o', 'page'], ['tab', 'installed'], ['esc', 'back'], ['?', 'help']]
      : [installed?.update ? ['↵', 'update'] : ['space', 'disable'], ['d', 'remove'], ['tab', 'discover'], ['esc', 'back'], ['?', 'help']],
    ['o', 'tab', '?'],
  )
  overlays(c, inst)
}

function stageHit(c: Ctx, stage: Rect, y: number, hit: ModHit, inst: Instance) {
  const b = hit.installed
    ? bigButton(c, stage.x, y, '✓', 'INSTALLED', 'installed', 'quiet', '')
    : bigButton(c, stage.x, y, '↓', 'INSTALL', 'install')
  let lx = right(b) + 3
  if (hit.installed && right(stage) - lx >= 10) lx = hint(c, lx, y, 'd', 'remove', 'remove') + 4
  if (right(stage) - lx >= 17) hint(c, lx, y, 'o', 'Modrinth page', 'page')
  facts(
    c,
    stage.x,
    y + 2,
    [
      ['Downloads', [[hit.downloads, {}]]],
      ['Updated', [[hit.updated, {}]]],
      ['For', [[`${inst.mc} ${inst.loader}`, {}]]],
    ],
    16,
    stage.w,
  )
}

function stageInstalled(c: Ctx, stage: Rect, y: number, m: InstalledMod) {
  const { t } = c
  const b = m.update ? bigButton(c, stage.x, y, '↑', 'UPDATE', 'update') : bigButton(c, stage.x, y, '●', 'ENABLED', 'toggle', 'quiet', 'space')
  const lx = right(b) + 3
  if (right(stage) - lx >= 10) hint(c, lx, y, 'd', 'remove', 'remove')
  facts(
    c,
    stage.x,
    y + 2,
    [
      ['Version', m.update ? [[m.version, {}], ['  →  ', { fg: t.faint }], [m.update, { fg: t.secondary }]] : [[m.version, {}]]],
      ['State', [[m.enabled ? 'Enabled' : 'Disabled', { fg: m.enabled ? t.success : t.dim }]]],
    ],
    22,
    stage.w,
  )
}

// ── play ────────────────────────────────────────────────────────────────────

function play(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  const inst = data.instances[0]
  w.header(c, p.header, [inst.name, 'Playing'])
  c.panes(1)
  c.enter(null)
  const a = rect(p.body.x + 2, p.body.y, p.body.w - 4, p.body.h)

  // The title steps down to a one-row header: the game has the big screen now.
  buf.text(a.x, a.y, spaced(inst.name), { fg: t.strong, bold: true }, a.w)
  const world = inst.worlds[0]
  buf.line(a.x, a.y + 1, dotted(c, [`Minecraft ${inst.mc}`, loaderName(inst), `as ${data.account.name}`, ...(world ? [world.name] : [])]), {}, a.w)

  // …and the session clock takes the stage, ticking.
  const secs = 18 * 60 + 42 + Math.floor(ui.tick * 0.08)
  const clock = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, '0')}`
  const cy = a.y + 3
  const f = fit(clock, a.w - 24, [FONT_L, FONT_S])
  let cx = a.x
  if (f) cx = drawBig(buf, a.x, cy, clock, f, [t.success, t.success, mix(t.success, t.bg, 0.3)])
  else cx = buf.text(a.x, cy, clock, { fg: t.success, bold: true })
  const sx = cx + 4
  buf.line(sx, cy, [[spin(c), { fg: t.success }], ['  RUNNING', { fg: t.success, bold: true }]])
  buf.text(sx + 3, cy + 1, 'since 14:02', { fg: t.dim })
  if ((f?.rows ?? 1) >= 3) buf.text(sx + 3, cy + 2, `${inst.java.replace(' (automatic)', '')}  ·  ${inst.memory}`, { fg: t.muted })

  const by = cy + (f?.rows ?? 1) + 1
  bigButton(c, a.x, by, '■', 'STOP', 'stop', 'quiet', 's')
  const lvl = c.hovered('level')
  const lr = buf.lineRight(rect(a.x, by, a.w, 1), [
    [`${data.log.length} lines`, { fg: t.muted }],
    ['   ·   ', { fg: t.faint }],
    ['everything', { fg: lvl ? t.strong : t.subtle }],
    ['  v', { fg: t.accent, bold: true }],
  ])
  buf.region(rect(lr, by, right(a) - lr, 1), 'level')

  // A plank with a light running along it: the game is alive.
  const py = by + 2
  const seg = 14
  buf.hline(p.body.x, py, p.body.w, { fg: t.rule }, '▔')
  const span = p.body.w + seg
  const head = ((ui.tick + 24) % span) - seg
  for (let i = 0; i < seg; i++) {
    const x = p.body.x + head + i
    if (x < p.body.x || x >= right(p.body)) continue
    buf.set(x, py, '▀', { fg: mix(t.rule, t.accent, (i + 1) / seg) })
  }

  // The log: newest at the bottom, brightest.
  const body = rect(a.x, py + 2, a.w, bottom(a) - py - 2)
  const lines = data.log.slice(-body.h)
  lines.forEach((l, i) => {
    const y = body.y + i
    const newest = i === lines.length - 1
    const fg = l.level === 'ERROR' ? t.error : l.level === 'WARN' ? t.warning : newest ? t.text : t.subtle
    const x = buf.text(body.x, y, l.time, { fg: newest ? t.dim : t.faint })
    buf.text(x + 2, y, l.text, { fg }, right(body) - x - 2)
  })

  footer(
    c,
    p.footer,
    [['s', 'stop'], ['v', 'log level'], ['/', 'filter'], ['c', 'copy log'], ['esc', 'back']],
    ['c', '/'],
  )
  overlays(c, inst)
}

// ── wizard ──────────────────────────────────────────────────────────────────

function wizard(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  w.header(c, p.header, ['New instance'])
  c.panes(1)
  c.enter(null)

  // Where you are: three steps on one line.
  let x = p.body.x + 2
  ;['Version', 'Loader', 'Name'].forEach((step, i) => {
    if (i) x = buf.text(x + 2, p.body.y, '›', { fg: t.faint }) + 2
    const on = i === 0
    x = buf.line(x, p.body.y, [[`${i + 1}  `, { fg: on ? t.accent : t.faint, bold: true }], [step, on ? { fg: t.strong, bold: true } : { fg: t.dim }]])
  })

  const versions = data.versions.filter((v) => !v.snapshot)
  const sel = c.sel(versions.length)
  const v = versions[sel]

  const shelfR = rect(p.body.x, bottom(p.body) - 3, p.body.w, 3)
  const labelY = shelfR.y - 2
  w.section(c, p.body.x + 2, labelY, 'Releases', versions.length)
  const tog = w.toggle(c, right(p.body) - 9, labelY, false, false, 'snapshots')
  buf.text(tog.x - 11, labelY, 'Snapshots', { fg: t.dim })

  const stage = rect(p.body.x + 2, p.body.y + 2, p.body.w - 4, labelY - 1 - p.body.y - 2)
  const faces = facesFor(stage.h)
  const rows = titleRows(v.id, stage.w, faces)
  const top = stage.y + Math.max(0, Math.floor((stage.h - rows - STAGE_BODY) / 2))
  title(c, stage.x, top, v.id, stage.w, faces)
  const subY = top + rows + 1
  buf.line(stage.x, subY, dotted(c, ['Release', v.date, ...(v.latest ? [['latest', { fg: t.success }] as [string, Style]] : [])]), {}, stage.w)
  const pillY = subY + 2
  bigButton(c, stage.x, pillY, '→', 'NEXT', 'key:↵')
  facts(
    c,
    stage.x,
    pillY + 2,
    [
      ['Minecraft', [[v.id, { fg: t.strong }]]],
      ['Loader', [['—', { fg: t.faint }]]],
      ['Name', [['—', { fg: t.faint }]]],
    ],
    18,
    stage.w,
  )

  shelf(
    c,
    shelfR,
    versions.map((ver) => {
      const [md, yr] = ver.date.split(', ')
      return { name: ver.id, meta: ver.latest ? [['latest', { fg: t.success }]] : [[yr === '2026' ? md : `${md.split(' ')[0]} ${yr}`, {}]] }
    }),
    sel,
    true,
    12,
  )

  footer(c, p.footer, [['↵', 'next'], ['↑↓', 'browse'], ['space', 'snapshots'], ['esc', 'cancel'], ['?', 'help']], ['space', '?'])
  overlays(c, null)
}

// ── overlays ────────────────────────────────────────────────────────────────

function overlays(c: Ctx, inst: Instance | null) {
  const { buf, t, ui, data } = c
  if (!ui.overlay) return
  const target = inst ?? data.instances[0]
  if (ui.overlay === 'help') {
    const r = sheet(c, 64, 15, 'Keys', { hint: 'esc  close' })
    const half = Math.floor((r.w - 4) / 2)
    const block = (x: number, y: number, head: string, keys: [string, string][]) => {
      buf.text(x, y, head.toUpperCase(), { fg: t.muted, bold: true })
      keys.forEach(([k, d], i) => {
        buf.text(x, y + 2 + i, k, { fg: t.accent, bold: true })
        buf.text(x + 9, y + 2 + i, d, { fg: t.dim }, half - 9)
      })
      return y + keys.length + 3
    }
    const y = block(r.x, r.y, 'Stage', [['↵', 'play'], ['o', 'play offline'], ['l', 'last log'], ['x', 'more actions']])
    block(r.x, y, 'Shelf', [['←→ ↑↓', 'browse'], ['n', 'new instance'], ['d ⌫', 'delete']])
    const y2 = block(r.x + half + 4, r.y, 'Go to', [['m', 'mods'], ['a', 'accounts'], ['s', 'settings']])
    block(r.x + half + 4, y2, 'Anywhere', [[':', 'commands'], ['?', 'this card'], ['q', 'quit']])
  } else if (ui.overlay === 'palette') {
    const r = sheet(c, 60, 13, 'Commands')
    w.field(c, rect(r.x, r.y, r.w, 1), ui.oquery, 'Play, open or change something…', true)
    const cmds: [string, string][] = [
      [`Play ${target.name}`, '↵'],
      ['Play offline', 'o'],
      ['New instance', 'n'],
      ['Browse mods', 'm'],
      ['Accounts', 'a'],
      ['Settings', 's'],
      ['Open instance folder', ''],
    ]
    const sel = c.osel(cmds.length)
    cmds.slice(0, r.h - 2).forEach(([cmd, key], i) => {
      const row = rect(r.x - 2, r.y + 2 + i, r.w + 4, 1)
      const on = sel === i
      const inner = w.band(c, row, on, c.hovered(`osel:${i}`))
      buf.text(inner.x, row.y, cmd, w.rowLabel(c, on, false))
      if (key) buf.lineRight(inner, [[key, { fg: on ? t.accent : t.muted, bold: true }]])
      buf.region(row, `osel:${i}`)
    })
  } else if (ui.overlay === 'menu') {
    const wd = Math.max(44, Math.min(measure(target.name, FONT_L), buf.cols - 8))
    const r = sheet(c, wd, 10 + titleRows(target.name, wd, [FONT_L, FONT_S]), 'Instance')
    const rows = title(c, r.x, r.y, target.name, r.w, [FONT_L, FONT_S])
    const items: [string, string][] = [
      ['Play', '↵'],
      ['Play offline', 'o'],
      ['Mods', 'm'],
      ['Rename', 'e'],
      ['Open folder', ''],
      ['Delete', 'd'],
    ]
    const sel = c.osel(items.length)
    items.forEach(([it, key], i) => {
      const row = rect(r.x - 2, r.y + rows + 1 + i, r.w + 4, 1)
      const on = sel === i
      const inner = w.band(c, row, on, c.hovered(`osel:${i}`))
      buf.text(inner.x, row.y, it, it === 'Delete' ? { fg: t.error, bold: on } : w.rowLabel(c, on, false))
      if (key) buf.lineRight(inner, [[key, { fg: t.muted, bold: true }]])
      buf.region(row, `osel:${i}`)
    })
  } else if (ui.overlay === 'confirm') {
    const wd = Math.max(56, Math.min(measure(target.name, FONT_L), buf.cols - 8))
    const n = target.worlds.length
    const msg = wrap(`${n} world${n === 1 ? '' : 's'} and ${target.mods.length} mods go with it. This can’t be undone.`, wd)
    const r = sheet(c, wd, 6 + msg.length + titleRows(target.name, wd, [FONT_L, FONT_S]), 'Delete instance', { danger: true })
    const rows = title(c, r.x, r.y, target.name, r.w, [FONT_L, FONT_S], t.error)
    msg.forEach((l, i) => buf.text(r.x, r.y + rows + 1 + i, l, { fg: t.text }))
    const by = r.y + rows + 2 + msg.length
    const k = w.button(c, r.x, by, 'Keep', '↵', 'secondary', 'close')
    w.button(c, right(k) + 2, by, 'Delete', 'y', 'danger', 'delete')
  }
}

export default defineVariation({
  id: 'launcher',
  name: 'Launcher',
  order: 20,
  blurb:
    'Play-first, like a game launcher’s front page: one instance on stage, its name set large, one big Play, the rest on a shelf. Trades density and side-by-side comparison for calm and one obvious decision.',
  notes: [
    'Display type is a hand-built half-block face in three sizes (6, 3 and 2 rows, then letterspaced caps), foreground only: line spacing just opens thin gaps inside letters. The lint counts those stacked blocks as “vertical strokes”; nothing else on screen stacks.',
    'The shelf is the instance list turned sideways: tiles on a thin plank, the current one lit from below, ‹ › when more are off-screen. The lab drives it with ↑↓ and clicks (←→ would too in the app). Mods and the new-instance version picker reuse it.',
    'The stage is the same everywhere: big name, one-line subtitle, one accent action, Steam-style facts. Home adds a What’s new corner when there’s room.',
    'Playing: the title steps down to a one-row header and the session clock takes the stage, ticking in green; a light runs along the plank and the log’s newest line is the brightest.',
  ],
  screens: { home, mods, wizard, play },
})
