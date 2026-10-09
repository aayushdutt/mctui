// Blocky: Minecraft-native and playful, still a tool. One-row pixel sprites
// (a half block = two pixels per cell), the title screen's splash, a hotbar
// footer, an XP bar for progress, a chat-style log, the mods screen as an
// inventory, and overlays in the game's own dialects.

import { defineVariation, type Ctx } from '../../kit'
import { cols, rect, right, bottom, wrap, type Line, type Rect } from '../../term/buffer'
import * as w from '../../term/widgets'
import { sprite, type SpriteId } from './sprites'
import { header, splash, section, hotbar, xpbar, button, stats, slot, tooltip, iconTabs, panel, ghost, SLOT_W, type Hint } from './parts'
import { SPLASH, CATEGORIES, CHAT, ICON_CHOICES, instanceIcon, worldIcon, modIcon, modPage, accountIcon } from './extra'
import { mix } from '../../term/theme'
import type { Instance, InstalledMod, ModHit } from '../../data/mock'

const DOWNLOAD = { ratio: 0.37, files: '1,204 of 3,412 files', speed: '48 MB/s', left: 'about 40s left' }

function home(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  const logoEnd = header(c, p.header)
  // Room to spare: the splash gets its own row, hung off the logo's lower right.
  const roomy = buf.rows >= 26
  if (roomy) splash(c, logoEnd - 3, p.header.y + 1, SPLASH)
  else splash(c, logoEnd + 2, p.header.y, SPLASH)
  const body = roomy ? rect(p.body.x, p.body.y + 1, p.body.w, p.body.h - 1) : p.body

  const focus = c.panes(2)
  const insts = data.instances
  const sel = c.sel(insts.length + 1)
  const listW = Math.min(46, Math.max(34, Math.floor(body.w * 0.4)))
  const [list, , detail] = cols(body, [listW, 2, '1fr'])
  const stride = body.h >= 19 ? 3 : 2

  section(c, list.x + 2, list.y, 'Instances', insts.length, focus === 0)
  insts.forEach((inst, i) => {
    const y = list.y + 2 + i * stride
    if (y >= bottom(list)) return
    const row = rect(list.x, y, list.w, 1)
    const on = sel === i
    const hov = c.hovered(`sel:${i}`)
    if (on) buf.fill(row, { bg: focus === 0 ? t.selection : t.surface })
    else if (hov) buf.fill(row, { bg: t.surface })
    const ix = list.x + 2
    const nx = ix + 6
    const inner = rect(nx, y, right(row) - 2 - nx, 1)
    const tight = inner.w < inst.name.length + 11
    const acc: Line = inst.trouble
      ? [[tight ? '▲' : '▲ Crashed', { fg: t.warning }]]
      : inst.fresh
        ? [[`↓ ${Math.round(DOWNLOAD.ratio * 100)}%`, { fg: t.success }]]
        : [[inst.lastPlayed ?? '', { fg: t.dim }]]
    const ax = buf.lineRight(inner, acc)
    buf.text(nx, y, inst.name, on ? { fg: t.strong, bold: true } : hov ? { fg: t.strong } : { fg: t.text }, ax - nx - 2)
    if (stride === 3) {
      const bits = [inst.mc, inst.loader, inst.mods.length ? `${inst.mods.length} mods` : ''].filter(Boolean)
      buf.text(nx, y + 1, bits.join('  ·  '), { fg: t.muted }, inner.w)
    }
    buf.region(rect(list.x, y, list.w, stride - 1), `sel:${i}`)
    // The world list's hover: a play arrow over the dimmed icon (it shares
    // `key:↵` with the hotbar's play slot, so each lights the other).
    if (on && c.hovered('key:↵')) {
      sprite(c, ix, y, instanceIcon(inst), 0.6)
      buf.set(ix + 1, y, '▶', { fg: t.strong, bold: true })
    } else sprite(c, ix, y, instanceIcon(inst))
    if (on) buf.region(rect(ix, y, 4, 1), 'key:↵')
  })
  const ny = list.y + 2 + insts.length * stride
  const newRow = rect(list.x, ny, list.w, 1)
  const onNew = sel === insts.length
  const hovNew = c.hovered(`sel:${insts.length}`)
  if (onNew) buf.fill(newRow, { bg: focus === 0 ? t.selection : t.surface })
  else if (hovNew) buf.fill(newRow, { bg: t.surface })
  // An empty slot waiting for a block.
  buf.fill(rect(list.x + 2, ny, 4, 1), { bg: onNew ? t.surfaceHi : t.surface })
  buf.text(list.x + 3, ny, '+', { fg: onNew || hovNew ? t.accent : t.muted, bold: true })
  buf.text(list.x + 8, ny, 'New instance', { fg: onNew ? t.strong : hovNew ? t.text : t.dim, bold: onNew })
  buf.region(newRow, `sel:${insts.length}`)

  // The launcher's news, when the list leaves room under it.
  const newsY = bottom(list) - 1 - data.news.length * 2
  if (newsY > ny + 3) {
    section(c, list.x + 2, newsY, 'News')
    data.news.forEach((n, i) => {
      const y = newsY + 2 + i * 2
      sprite(c, list.x + 2, y, i ? 'tnt' : 'cherry')
      buf.text(list.x + 8, y, n.title, { fg: t.text }, list.w - 18)
      buf.lineRight(rect(list.x, y, list.w - 2, 1), [[n.date, { fg: t.dim }]])
    })
  }

  const d = rect(detail.x + 2, detail.y, detail.w - 2, detail.h)
  if (onNew) newTeaser(c, d)
  else instanceDetail(c, d, insts[sel], focus === 1)

  hotbar(c, p.footer, [
    onNew ? ['↵', 'create'] : ['↵', 'play'],
    ['o', 'offline'],
    ['n', 'new'],
    ['/', 'search'],
    [':', 'commands'],
    ['?', 'help'],
  ])
  overlays(c)
}

function instanceDetail(c: Ctx, d: Rect, inst: Instance, focused: boolean) {
  const { buf, t } = c
  sprite(c, d.x, d.y, instanceIcon(inst))
  const nx = d.x + 6
  const [label, fg, bg] = inst.trouble
    ? ['▲ Crashed', t.warning, t.warningBg]
    : inst.fresh
      ? ['↓ Downloading', t.success, t.successBg]
      : ['● Ready', t.success, t.successBg]
  const chipX = right(d) - label.length - 2
  buf.fill(rect(chipX, d.y, label.length + 2, 1), { bg })
  buf.text(chipX + 1, d.y, label, { fg })
  buf.text(nx, d.y, inst.name, { fg: t.strong, bold: true }, chipX - nx - 2)
  const loader = inst.loader === 'Vanilla' ? 'Vanilla' : `${inst.loader} ${inst.loaderVersion}`
  const sub = `Minecraft ${inst.mc}  ·  ${loader}`
  buf.text(nx, d.y + 1, sub.length <= d.w - 6 ? sub : `${inst.mc}  ·  ${loader}`, { fg: t.dim }, d.w - 6)

  const by = d.y + 3
  const half = Math.floor((d.w - 2) / 2)
  button(c, rect(d.x, by, half, 1), '▶ Play', '↵', 'primary', 'play')
  button(c, rect(d.x + half + 2, by, d.w - half - 2, 1), 'Play offline', 'o', 'secondary', 'offline')

  const tab = c.tabs(3)
  w.tabs(c, rect(d.x, by + 2, d.w, 1), [['Overview'], ['Mods', inst.mods.length], ['Resource packs', inst.packs.length]], tab, focused)

  const body = rect(d.x, by + 4, d.w, bottom(d) - by - 4)
  if (tab === 0) {
    let y = body.y
    if (inst.fresh) {
      section(c, d.x, y, 'Getting ready')
      buf.lineRight(rect(d.x, y, d.w, 1), [[DOWNLOAD.left, { fg: t.muted }]])
      xpbar(c, rect(d.x, y + 2, d.w, 1), DOWNLOAD.ratio, `${Math.round(DOWNLOAD.ratio * 100)}%`)
      buf.text(d.x, y + 3, `${DOWNLOAD.files}  ·  ${DOWNLOAD.speed}`, { fg: t.dim })
      y += 5
    } else {
      y += stats(c, body, [
        [inst.playtime, 'played'],
        [inst.lastPlayed ?? 'never', 'last played'],
        [String(inst.worlds.length), inst.worlds.length === 1 ? 'world' : 'worlds'],
        [inst.memory, 'memory'],
      ]) + 1
    }
    if (y >= bottom(body)) return
    section(c, d.x, y, 'Worlds', inst.worlds.length)
    if (!inst.worlds.length) buf.text(d.x, y + 2, 'No worlds yet. They show up after you play.', { fg: t.faint })
    inst.worlds.forEach((world, i) => {
      const r = rect(d.x, y + 2 + i * 2, d.w, 1)
      if (r.y >= bottom(body)) return
      sprite(c, r.x, r.y, worldIcon(world.mode, world.name))
      const ax = buf.lineRight(r, [[world.lastPlayed, { fg: t.dim }]])
      const mx = buf.text(r.x + 5, r.y, world.name, { fg: t.text }, ax - r.x - 7)
      const meta = [`${world.mode}  ·  ${world.size}`, world.size].find((m) => m.length <= ax - mx - 4)
      if (meta) buf.text(mx + 2, r.y, meta, { fg: t.muted })
    })
    y += 2 + Math.max(1, inst.worlds.length) * 2

    // Room to spare: the instance's own hotbar of mods, then this month's play.
    if (inst.mods.length && y + 3 <= bottom(body)) {
      section(c, d.x, y, 'Mods')
      let sx = d.x
      inst.mods.forEach((m, i) => {
        slot(c, sx, y + 2, modIcon(m.name), {
          badge: m.update ? ['↑', t.success] : undefined,
          fade: m.enabled ? 0 : 0.6,
          hovered: c.hovered(`mod:${i}`),
          region: 'screen:mods',
        })
        sx += SLOT_W + 1
      })
      const names = inst.mods.map((m) => m.name).join(', ')
      if (right(d) - sx > 12) buf.text(sx + 2, y + 2, names, { fg: t.muted }, right(d) - sx - 2)
      y += 4
    }
    if (!inst.fresh && y + 3 <= bottom(body)) {
      section(c, d.x, y, 'Last 30 days')
      buf.lineRight(rect(d.x, y, d.w, 1), [[`${inst.playtime} in all`, { fg: t.muted }]])
      activity(c, rect(d.x, y + 2, d.w, 1), inst.name)
    }
  } else if (tab === 1) {
    if (!inst.mods.length) buf.text(body.x, body.y, 'No mods yet. Press m to open the inventory.', { fg: t.faint })
    inst.mods.forEach((m, i) => {
      const r = rect(body.x, body.y + i * 2, body.w, 1)
      if (r.y >= bottom(body)) return
      sprite(c, r.x, r.y, modIcon(m.name), m.enabled ? 0 : 0.6)
      buf.text(r.x + 5, r.y, m.name, { fg: m.enabled ? t.text : t.muted })
      buf.lineRight(r, m.update ? [[`↑ ${m.update}`, { fg: t.success }], [`  ${m.version}`, { fg: t.dim }]] : [[m.enabled ? m.version : 'off', { fg: t.dim }]])
    })
  } else {
    if (!inst.packs.length) buf.text(body.x, body.y, 'No resource packs.', { fg: t.faint })
    inst.packs.forEach((name, i) => {
      const r = rect(body.x, body.y + i * 2, body.w, 1)
      sprite(c, r.x, r.y, i ? 'glass' : 'peak')
      buf.text(r.x + 5, r.y, name, { fg: t.text })
    })
  }
}

/** Play per day as a one-row bar chart: tall blocks on the days you played. */
function activity(c: Ctx, r: Rect, seed: string) {
  const { buf, t } = c
  const BARS = '▁▂▃▄▅▆▇'
  let h = [...seed].reduce((a, ch) => (a * 31 + ch.charCodeAt(0)) >>> 0, 7)
  const days = Array.from({ length: 30 }, () => {
    h = (h * 1103515245 + 12345) >>> 0
    const v = (h >>> 16) % 10
    return v < 4 ? 0 : v - 3
  })
  days[29] = 5
  const step = r.w >= 60 ? 2 : 1
  days.slice(-Math.floor(r.w / step)).forEach((v, i) => {
    const x = r.x + i * step
    if (v) buf.set(x, r.y, BARS[Math.min(BARS.length - 1, v)], { fg: t.success })
    else buf.set(x, r.y, '▁', { fg: t.rule })
  })
}

function newTeaser(c: Ctx, d: Rect) {
  const { buf, t, data } = c
  buf.fill(rect(d.x, d.y, 4, 1), { bg: t.surface })
  buf.text(d.x + 1, d.y, '+', { fg: t.accent, bold: true })
  buf.text(d.x + 6, d.y, 'New instance', { fg: t.strong, bold: true })
  buf.text(d.x + 6, d.y + 1, 'Pick a version, a loader, a name and a block.', { fg: t.dim }, d.w - 6)
  button(c, rect(d.x, d.y + 3, Math.min(34, d.w), 1), 'Create New Instance', '↵', 'primary', 'screen:wizard')
  const release = data.versions.find((v) => v.latest)
  const snap = data.versions.find((v) => v.snapshot)
  section(c, d.x, d.y + 5, 'Newest')
  ;([
    [release, 'grass', 'release'],
    [snap, 'tnt', 'snapshot'],
  ] as const).forEach(([v, icon, kind], i) => {
    if (!v) return
    const y = d.y + 7 + i * 2
    sprite(c, d.x, y, icon)
    const x = buf.text(d.x + 6, y, v.id, { fg: t.text })
    buf.text(x + 2, y, kind, { fg: kind === 'release' ? t.success : t.muted })
    buf.lineRight(rect(d.x, y, d.w, 1), [[v.date, { fg: t.dim }]])
  })
}

const INSTALLING = { name: 'Mod Menu', ratio: 0.62 }

function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  const inst = data.instances[0]
  header(c, p.header, [inst.name, 'Mods'])
  const focus = c.panes(2)

  const cats = CATEGORIES
  const all = modPage(data)
  const tab = c.tabs(cats.length)
  const inCat = (h: ModHit) => cats[tab][2] === null || h.category === cats[tab][2]
  const q = ui.query.toLowerCase()
  const hits = all.filter((h) => inCat(h) && h.name.toLowerCase().includes(q))
  const installed = inst.mods
  const isInstalled = (name: string) => installed.some((m) => m.name === name) || all.find((h) => h.name === name)?.installed

  // Columns: as many as fit beside a readable tooltip, nine at most (the game's width).
  const tipMin = 32
  const perRow = Math.max(4, Math.min(9, Math.floor((p.body.w - tipMin - 3 + 1) / (SLOT_W + 1))))
  const gridW = perRow * (SLOT_W + 1) - 1
  const grid = rect(p.body.x, p.body.y, gridW, p.body.h)
  const tipX = right(grid) + 4
  const tipW = Math.min(60, right(p.body) - tipX)

  iconTabs(
    c,
    rect(p.body.x, p.body.y, p.body.w, 1),
    cats.map(([label, icon]) => [label, icon] as [string, SpriteId]),
    tab,
    focus === 0,
  )
  w.search(c, rect(grid.x, grid.y + 2, gridW, 1), ui.query, 'Search Modrinth', `${hits.length} of ${data.modTotal}`, ui.typing)

  // The inventory: rows of slots, a blank row between (one-row tints only).
  // Below it, after a gap: the instance's own hotbar. A notice row at the end.
  const noticeY = bottom(p.body) - 1
  const gy = grid.y + 4
  const fit = Math.max(1, Math.floor((noticeY - 6 - gy + 1) / 2))
  const needed = Math.ceil(hits.length / perRow)
  // At least a few rows of slots so it reads as an inventory (the game's is 9×5).
  const rowsFit = Math.max(1, Math.min(fit, Math.max(needed, buf.rows >= 36 ? 5 : 4)))
  const sel = c.sel(Math.max(1, focus === 0 ? hits.length : installed.length), focus === 0 ? { grid: perRow } : { across: true })
  const selRow = Math.floor(sel / perRow)
  const firstRow = Math.max(0, Math.min(selRow - rowsFit + 1, needed - rowsFit))
  for (let r = 0; r < rowsFit; r++) {
    for (let col = 0; col < perRow; col++) {
      const i = (firstRow + r) * perRow + col
      const x = grid.x + col * (SLOT_W + 1)
      const y = gy + r * 2
      const h = hits[i]
      if (!h) {
        if (hits.length || r > 0) slot(c, x, y, null)
        continue
      }
      const busy = h.name === INSTALLING.name
      slot(c, x, y, modIcon(h.name), {
        selected: focus === 0 && sel === i,
        hovered: c.hovered(`sel:${i}`),
        badge: busy ? ['↓', t.success] : isInstalled(h.name) ? ['✓', t.success] : undefined,
        region: focus === 0 ? `sel:${i}` : 'focus:0',
      })
    }
  }
  const below = Math.max(0, hits.length - (firstRow + rowsFit) * perRow)
  if (!hits.length) buf.text(grid.x + 1, gy, `Nothing matches “${ui.query}”.`, { fg: t.faint })
  const moreY = gy + rowsFit * 2 - 1
  if (firstRow > 0) buf.lineRight(rect(grid.x, grid.y + 3, gridW, 1), [[`↑ ${firstRow * perRow} more`, { fg: t.faint }]])
  if (below > 0) buf.lineRight(rect(grid.x, moreY, gridW, 1), [[`↓ ${below} more`, { fg: t.faint }]])

  // Your hotbar: what this instance already has.
  const hbLabel = gy + rowsFit * 2 + 1
  const hotbarY = hbLabel + 2
  section(c, grid.x, hbLabel, 'Installed', installed.length, focus === 1)
  buf.lineRight(rect(grid.x, hbLabel, gridW, 1), [[`${inst.mc}  ·  ${inst.loader}`, { fg: t.muted }]])
  for (let col = 0; col < perRow; col++) {
    const m = installed[col]
    const x = grid.x + col * (SLOT_W + 1)
    if (!m) {
      slot(c, x, hotbarY, null)
      continue
    }
    slot(c, x, hotbarY, modIcon(m.name), {
      selected: focus === 1 && sel === col,
      hovered: c.hovered(`sel:${col}`),
      badge: m.update ? ['↑', t.success] : undefined,
      fade: m.enabled ? 0 : 0.6,
      region: focus === 1 ? `sel:${col}` : 'focus:1',
    })
  }

  // A download in flight: the XP bar, full width, on the notice row.
  if (noticeY > hotbarY + 1) {
    const lx = buf.text(grid.x, noticeY, `↓ Installing ${INSTALLING.name}`, { fg: t.text })
    const tail = '2 of 3 files'
    const tx = right(p.body) - tail.length
    buf.text(tx, noticeY, tail, { fg: t.muted })
    xpbar(c, rect(lx + 3, noticeY, tx - lx - 6, 1), INSTALLING.ratio, `${Math.round(INSTALLING.ratio * 100)}%`)
  }

  // The tooltip follows the selected slot, like hovering an item.
  const floor = noticeY > hotbarY + 1 ? noticeY - 1 : bottom(p.body)
  if (tipW >= 24) {
    const anchorY = focus === 0 ? gy + (selRow - firstRow) * 2 : hotbarY
    if (focus === 0 && hits[sel]) hitTooltip(c, tipX, anchorY, tipW, hits[sel], !!isInstalled(hits[sel].name), hits[sel].name === INSTALLING.name, floor, inst)
    else if (focus === 1 && installed[sel]) modTooltip(c, tipX, anchorY, tipW, installed[sel], inst.name, floor)
  }

  const have = focus === 0 && !!hits[sel] && !!isInstalled(hits[sel].name)
  const hints: Hint[] = !hits.length && focus === 0
    ? [['/', 'search'], ['esc', 'clear'], ['tab', 'installed'], ['?', 'help']]
    : focus === 1
      ? [installed[sel]?.update ? ['↵', 'update'] : ['space', 'turn off'], ['d', 'remove'], ['tab', 'discover'], ['esc', 'back'], ['?', 'help']]
      : have
        ? [['o', 'page'], ['d', 'remove'], ['/', 'search'], ['tab', 'installed'], ['esc', 'back'], ['?', 'help']]
        : [['↵', 'install'], ['o', 'page'], ['/', 'search'], ['tab', 'installed'], ['esc', 'back'], ['?', 'help']]
  hotbar(c, p.footer, hints)
  overlays(c)
}

/** Lays a tooltip out beside its anchor row, nudged up to stay on screen. */
function placeTip(anchorY: number, lines: number, extra: number, floor: number, top: number) {
  const h = lines + 2 + extra
  return Math.max(top, Math.min(anchorY - 1, floor - h))
}

/** What a mod pulls in with it (mock). */
const NEEDS: Record<string, string[]> = {
  'Iris Shaders': ['Sodium'],
  Zoomify: ['YetAnotherConfigLib'],
  Continuity: [],
  Create: ['Architectury'],
  Waystones: ['Cloth Config'],
}

function hitTooltip(c: Ctx, x: number, anchorY: number, tw: number, h: ModHit, have: boolean, busy: boolean, floor: number, inst: Instance) {
  const { buf, t } = c
  const lore = wrap(h.description, tw - 2).slice(0, 3)
  const lines: Line[] = [
    [[h.name, { fg: t.accent, bold: true }]],
    [[`by ${h.author}`, { fg: t.dim }]],
    ...lore.map((l): Line => [[l, { fg: t.subtle }]]),
    [[`${h.category}  ·  ${h.side}`, { fg: t.muted }]],
    [[[`${h.downloads} downloads  ·  updated ${h.updated}`, `${h.downloads} downloads  ·  ${h.updated}`, `${h.downloads} downloads`].find((l) => l.length <= tw - 2)!, { fg: t.muted }]],
    [['Modrinth', { fg: t.secondary, italic: true }]],
  ]
  const y = placeTip(anchorY, lines.length, 2, floor, c.buf.area().y + 3)
  const used = tooltip(c, x, y, tw, lines)
  if (have) buf.lineRight(rect(x, y + 1, tw - 1, 1), [['✓ Installed', { fg: t.success }]])
  const by = y + used + 1
  if (busy) xpbar(c, rect(x, by, tw, 1), INSTALLING.ratio, `${Math.round(INSTALLING.ratio * 100)}%`)
  else {
    const bw = Math.floor((tw - 2) / 2)
    button(c, rect(x, by, bw, 1), have ? 'Remove' : 'Install', have ? 'd' : '↵', have ? 'secondary' : 'primary', have ? 'remove' : 'install')
    button(c, rect(x + bw + 2, by, tw - bw - 2, 1), tw - bw - 2 >= 19 ? 'Modrinth page' : 'Page', 'o', 'secondary', 'page')
  }

  // Room below: whether it fits this instance, and what comes along with it.
  const fits: Line[] = [
    [['✓ ', { fg: t.success }], [`Minecraft ${inst.mc}`, { fg: t.text }]],
    [['✓ ', { fg: t.success }], [`${inst.loader} ${inst.loaderVersion ?? ''}`.trim(), { fg: t.text }]],
  ]
  const deps = h.name === 'Fabric API' ? [] : ['Fabric API', ...(NEEDS[h.name] ?? [])]
  for (const dep of deps) {
    const got = inst.mods.some((m) => m.name === dep)
    fits.push(got ? [['✓ ', { fg: t.success }], [dep, { fg: t.text }], ['  installed', { fg: t.muted }]] : [['↓ ', { fg: t.accent }], [dep, { fg: t.text }], ['  comes with it', { fg: t.muted }]])
  }
  const cy = by + 2
  if (cy + 1 + fits.length < floor) {
    section(c, x, cy, `Fits ${inst.name}`)
    fits.forEach((l, i) => buf.line(x, cy + 2 + i, l, {}, tw))
  }
}

function modTooltip(c: Ctx, x: number, anchorY: number, tw: number, m: InstalledMod, instName: string, floor: number) {
  const { t } = c
  const lines: Line[] = [
    [[m.name, { fg: t.accent, bold: true }]],
    [[`Version ${m.version}`, { fg: t.dim }]],
    m.update ? [[`↑ ${m.update} is out`, { fg: t.success }]] : [['Up to date', { fg: t.muted }]],
    [[m.enabled ? 'Enabled' : 'Disabled', { fg: m.enabled ? t.subtle : t.muted }]],
    [[instName, { fg: t.secondary, italic: true }]],
  ]
  const y = placeTip(anchorY, lines.length, 2, floor, c.buf.area().y + 3)
  const used = tooltip(c, x, y, tw, lines)
  const bw = Math.floor((tw - 2) / 2)
  if (m.update) button(c, rect(x, y + used + 1, bw, 1), 'Update', '↵', 'primary', 'update')
  else button(c, rect(x, y + used + 1, bw, 1), m.enabled ? 'Turn off' : 'Turn on', 'space', 'secondary', 'toggle')
  button(c, rect(x + bw + 2, y + used + 1, tw - bw - 2, 1), 'Remove', 'd', 'secondary', 'remove')
}

const THREADS: Record<string, string> = { main: 'Main', 'Render thread': 'Render', 'Server thread': 'Server', 'Worker-Main-4': 'Worker' }

type ChatKind = 'log' | 'chat' | 'join' | 'advancement' | 'warn' | 'error'
interface ChatLine {
  time: string
  kind: ChatKind
  who: string
  text: string
}

/** The game log read as chat: players talk, the game announces, threads mutter. */
function chatLines(c: Ctx): ChatLine[] {
  const out: ChatLine[] = c.data.log.map((l) => {
    const who = THREADS[l.thread] ?? l.thread
    if (l.level === 'ERROR') return { time: l.time, kind: 'error', who, text: l.text }
    if (l.level === 'WARN') return { time: l.time, kind: 'warn', who, text: l.text }
    if (l.text.includes('joined the game')) return { time: l.time, kind: 'join', who: '', text: l.text }
    if (l.text.startsWith('[CHAT] ')) {
      const text = l.text.slice(7)
      return { time: l.time, kind: text.includes('advancement') ? 'advancement' : 'chat', who: '', text }
    }
    return { time: l.time, kind: 'log', who, text: l.text }
  })
  for (const m of CHAT) {
    const at = out.findIndex((l) => l.time > m.time)
    const [, who, text] = /^<([^>]+)> (.*)$/.exec(m.text) ?? ['', '', m.text]
    out.splice(at < 0 ? out.length : at, 0, { time: m.time, kind: 'chat', who, text })
  }
  return out
}

function chatLine(c: Ctx, x: number, y: number, maxW: number, l: ChatLine, showTime: boolean) {
  const { buf, t } = c
  let cx = x
  if (showTime) cx = buf.text(cx, y, l.time, { fg: t.faint }) + 2
  const room = () => maxW - (cx - x)
  switch (l.kind) {
    case 'chat':
      cx = buf.text(cx, y, `<${l.who}> `, { fg: t.strong, bold: true }, room())
      buf.text(cx, y, l.text, { fg: t.text }, room())
      break
    case 'join':
      buf.text(cx, y, l.text, { fg: t.warning }, room())
      break
    case 'advancement': {
      const m = /^(.*?)(\[.*\])(.*)$/.exec(l.text)
      if (!m) {
        buf.text(cx, y, l.text, { fg: t.text }, room())
        break
      }
      buf.line(cx, y, [[m[1], { fg: t.text }], [m[2], { fg: t.success }], [m[3], { fg: t.text }]], {}, room())
      break
    }
    case 'warn':
    case 'error': {
      const fg = l.kind === 'warn' ? t.warning : t.error
      cx = buf.text(cx, y, l.kind === 'warn' ? '▲ ' : '✗ ', { fg }, room())
      cx = buf.text(cx, y, `[${l.who}] `, { fg: mix(t.bg, fg, 0.7) }, room())
      buf.text(cx, y, l.text, { fg }, room())
      break
    }
    default:
      cx = buf.text(cx, y, `[${l.who}] `, { fg: t.faint }, room())
      buf.text(cx, y, l.text, { fg: t.dim }, room())
  }
}

function play(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  const inst = data.instances[0]
  header(c, p.header, [inst.name, 'Playing'])
  const a = p.body

  // Who's playing what, and how it's going.
  sprite(c, a.x, a.y, instanceIcon(inst))
  const chipLabel = '● Running  18m'
  const chipX = right(a) - chipLabel.length - 2
  buf.fill(rect(chipX, a.y, chipLabel.length + 2, 1), { bg: t.successBg })
  buf.text(chipX + 1, a.y, chipLabel, { fg: t.success })
  buf.text(a.x + 6, a.y, inst.name, { fg: t.strong, bold: true })
  const world = inst.worlds[0]
  buf.text(a.x + 6, a.y + 1, `Minecraft ${inst.mc} with ${inst.loader}  ·  as ${data.account.name}  ·  in ${world.name}`, { fg: t.dim }, a.w - 6)

  // The XP bar, repurposed: memory in use, the "level" in the middle.
  const hy = a.y + 3
  const stop = button(c, rect(a.x, hy, 12, 1), 'Stop', 's', 'secondary', 'stop')
  const mx = buf.text(right(stop) + 3, hy, 'memory', { fg: t.muted }) + 2
  const cap = 'of 4 GB'
  const capX = right(a) - cap.length
  buf.text(capX, hy, cap, { fg: t.muted })
  xpbar(c, rect(mx, hy, capX - mx - 2, 1), 0.65, '2.6 GB')

  // Chat, like the game's: newest at the bottom, the input bar under it.
  const lines = chatLines(c)
  const views: [string, (l: ChatLine) => boolean][] = [
    ['Everything', () => true],
    ['Chat', (l) => l.kind === 'chat' || l.kind === 'join' || l.kind === 'advancement'],
    ['Problems', (l) => l.kind === 'warn' || l.kind === 'error'],
  ]
  const tab = c.tabs(views.length)
  const q = ui.query.toLowerCase()
  const shown = lines.filter((l) => views[tab][1](l) && (!q || `${l.who} ${l.text}`.toLowerCase().includes(q)))
  w.tabs(c, rect(a.x, hy + 2, a.w, 1), views.map(([label, f]) => [label, lines.filter(f).length] as [string, number]), tab, true)

  const inputY = bottom(a) - 1
  const logTop = hy + 4
  const fit = Math.max(1, inputY - 1 - logTop)
  const showTime = a.w >= 90
  // Overflow: the top row says how much is above instead of showing a line.
  const hidden = shown.length > fit ? shown.length - (fit - 1) : 0
  if (hidden) buf.text(a.x, logTop, `↑ ${hidden} earlier`, { fg: t.faint })
  const visible = shown.slice(hidden)
  const y0 = inputY - 1 - visible.length
  visible.forEach((l, i) => chatLine(c, a.x, y0 + i, a.w, l, showTime))
  if (!shown.length) buf.text(a.x, inputY - 2, 'Nothing here yet.', { fg: t.faint })

  // The chat input bar doubles as the filter.
  const bar = rect(a.x, inputY, a.w, 1)
  const hov = c.hovered('query')
  buf.fill(bar, { bg: ui.typing ? t.selection : hov ? t.surfaceHi : t.surface })
  buf.text(a.x + 1, inputY, '/', { fg: ui.typing ? t.accent : t.muted, bold: true })
  const qx = ui.query
    ? buf.text(a.x + 3, inputY, ui.query, { fg: t.strong })
    : (buf.text(a.x + 3, inputY, 'Filter the log', { fg: t.muted }), a.x + 3)
  if (ui.typing) buf.set(qx, inputY, '_', { fg: t.strong, bold: true })
  buf.lineRight(rect(a.x, inputY, a.w - 1, 1), [[`${shown.length} lines`, { fg: t.muted }]])
  buf.region(bar, 'query')

  hotbar(c, p.footer, [
    ['s', 'stop'],
    ['/', 'filter'],
    ['c', 'copy log'],
    ['f', 'folder'],
    ['esc', 'back'],
    ['?', 'help'],
  ])
  overlays(c)
}

/** Update names, the way players remember versions. */
const DROPS: Record<string, string> = {
  '1.21.11': 'Mounts of Mayhem',
  '1.21.9': 'The Copper Age',
  '1.21.6': 'Chase the Skies',
  '1.21.5': 'Spring to Life',
  '1.21.4': 'The Garden Awakens',
  '1.20.1': 'Trails & Tales',
  '1.18.2': 'Caves & Cliffs',
  '1.12.2': 'World of Color',
  '1.8.9': 'Bountiful Update',
}

const LOADERS: [name: string, version: string, about: string][] = [
  ['Fabric', '0.17.2', 'Lightweight mod loader. Fill its inventory from Modrinth.'],
  ['Vanilla', '', 'The game exactly as Mojang ships it. No mods.'],
]

/** "Create New World", for instances: tabs on a rule, one centred column. */
function wizard(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  header(c, p.header, ['New instance'])
  const col = rect(p.body.x + Math.max(0, Math.floor((p.body.w - 64) / 2)), p.body.y, Math.min(64, p.body.w), p.body.h)
  const tab = c.tabs(3)
  const versions = data.versions.filter((v) => !v.snapshot)

  // Tabs sit on a rule; the active one wears an accent underline.
  const names = ['Version', 'Loader', 'More']
  const tw = names.reduce((a, n) => a + n.length + 4, -2)
  let x = col.x + Math.floor((col.w - tw) / 2)
  buf.hline(col.x, col.y + 1, col.w, { fg: t.rule })
  names.forEach((n, i) => {
    const on = i === tab
    const hov = c.hovered(`tab:${i}`)
    const r = rect(x, col.y, n.length + 2, 1)
    if (on) buf.fill(r, { bg: t.selection })
    else if (hov) buf.fill(r, { bg: t.surface })
    buf.text(x + 1, col.y, n, { fg: on ? t.strong : hov ? t.text : t.dim, bold: on })
    if (on) buf.hline(x, col.y + 1, r.w, { fg: t.accent }, '━')
    buf.region(r, `tab:${i}`)
    x = right(r) + 2
  })

  // What you've picked so far (kept in the lab's other state for the mock).
  const vPick = versions[tab === 0 ? c.sel(versions.length) : 0]
  const lPick = LOADERS[tab === 1 ? c.sel(LOADERS.length) : 0]
  const iPick = ICON_CHOICES[tab === 2 ? c.sel(ICON_CHOICES.length) : 0]
  const name = ui.query || `${vPick.id} ${lPick[0]}`

  const footY = bottom(col) - 1
  const previewY = footY - 3
  const area = rect(col.x, col.y + 3, col.w, previewY - 2 - (col.y + 3))

  if (tab === 0) {
    const sel = c.sel(versions.length)
    const stride = area.h >= 12 ? 2 : 1
    const fit = Math.max(1, Math.floor((area.h + stride - 1) / stride))
    const { start, shown } = w.scroll(versions, sel, fit)
    shown.forEach((v, j) => {
      const i = start + j
      const r = rect(col.x, area.y + j * stride, col.w, 1)
      const on = sel === i
      const hov = c.hovered(`sel:${i}`)
      if (on) buf.fill(r, { bg: t.selection })
      else if (hov) buf.fill(r, { bg: t.surface })
      const ix = buf.text(r.x + 2, r.y, v.id, on ? { fg: t.strong, bold: true } : { fg: t.text })
      if (v.latest) buf.text(ix + 2, r.y, 'latest', { fg: t.success })
      else if (DROPS[v.id]) buf.text(ix + 2, r.y, DROPS[v.id], { fg: on ? t.subtle : t.muted })
      buf.lineRight(rect(r.x, r.y, r.w - 2, 1), [[v.date, { fg: t.dim }]])
      buf.region(r, `sel:${i}`)
    })
    w.more(c, col.x + 2, bottom(area), versions.length - start - shown.length)
  } else if (tab === 1) {
    const sel = c.sel(LOADERS.length)
    LOADERS.forEach(([n, v, about], i) => {
      const y = area.y + i * 4
      button(c, rect(col.x, y, col.w, 1), v ? `${n} ${v}` : n, undefined, 'secondary', `sel:${i}`, sel === i, { icon: n === 'Fabric' ? 'fabric' : 'grass' })
      buf.lineCenter(rect(col.x, y + 1, col.w, 1), [[about, { fg: sel === i ? t.subtle : t.muted }]])
    })
  } else {
    const sel = c.sel(ICON_CHOICES.length)
    buf.text(col.x, area.y, 'Instance name', { fg: t.dim })
    w.field(c, rect(col.x, area.y + 1, col.w, 1), ui.query, `${vPick.id} ${lPick[0]}`, ui.typing, 'query')
    buf.text(col.x, area.y + 3, 'Icon', { fg: t.dim })
    ICON_CHOICES.forEach((icon, i) => {
      const sx = col.x + i * 8
      if (sx + 6 > right(col)) return
      slot(c, sx, area.y + 4, icon, { selected: sel === i, hovered: c.hovered(`sel:${i}`), region: `sel:${i}` }, 4)
    })
    buf.text(col.x, area.y + 6, 'Memory', { fg: t.dim })
    w.choice(c, col.x, area.y + 7, '4 GB', false, 'memory')
    buf.text(col.x + 14, area.y + 7, 'Java 25 is picked for you', { fg: t.muted })
  }

  // "Will be saved in…": the row it'll be on the home screen.
  buf.hline(col.x, previewY - 1, col.w, { fg: t.rule })
  const py = previewY + 1
  buf.text(col.x + 2, previewY, 'On your home screen', { fg: t.muted })
  buf.fill(rect(col.x, py, col.w, 1), { bg: t.surface })
  sprite(c, col.x + 2, py, iPick)
  buf.text(col.x + 8, py, name, { fg: t.strong, bold: true }, col.w - 36)
  buf.lineRight(rect(col.x, py, col.w - 2, 1), [[`${vPick.id}  ·  ${lPick[0]}${lPick[1] ? ` ${lPick[1]}` : ''}`, { fg: t.dim }]])

  const half = Math.floor((col.w - 2) / 2)
  button(c, rect(col.x, footY, half, 1), 'Create New Instance', '↵', 'primary', 'create')
  button(c, rect(col.x + half + 2, footY, col.w - half - 2, 1), 'Cancel', 'esc', 'secondary', 'screen:home')

  hotbar(c, p.footer, [
    ['↵', tab < 2 ? 'next' : 'create'],
    ['←→', 'tabs'],
    ['/', tab === 2 ? 'rename' : 'filter'],
    ['esc', 'cancel'],
    ['?', 'help'],
  ])
  overlays(c)
}

function accounts(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  header(c, p.header, ['Accounts'])
  const list = rect(p.body.x, p.body.y, Math.min(72, p.body.w), p.body.h)
  const accts = data.accounts
  const sel = c.sel(accts.length + 1)
  section(c, list.x + 2, list.y, 'Players', accts.length, true)
  accts.forEach((a, i) => {
    const y = list.y + 2 + i * 3
    const r = rect(list.x, y, list.w, 1)
    const on = sel === i
    const hov = c.hovered(`sel:${i}`)
    if (on) buf.fill(r, { bg: t.selection })
    else if (hov) buf.fill(r, { bg: t.surface })
    sprite(c, list.x + 2, y, accountIcon(a.name))
    buf.text(list.x + 6, y, a.name, on ? { fg: t.strong, bold: true } : { fg: t.text })
    const [label, fg] =
      a.status === 'active' ? ['● In use', t.success] : a.status === 'expired' ? ['▲ Sign in again', t.warning] : ['Offline', t.dim]
    buf.lineRight(rect(r.x, y, r.w - 2, 1), [[label, { fg }]])
    buf.text(list.x + 6, y + 1, `${a.kind}  ·  ${a.uuid.slice(0, 8)}…`, { fg: t.muted })
    buf.region(rect(list.x, y, list.w, 2), `sel:${i}`)
  })
  const ay = list.y + 2 + accts.length * 3
  const add = rect(list.x, ay, list.w, 1)
  const onAdd = sel === accts.length
  if (onAdd) buf.fill(add, { bg: t.selection })
  else if (c.hovered(`sel:${accts.length}`)) buf.fill(add, { bg: t.surface })
  buf.fill(rect(list.x + 2, ay, 2, 1), { bg: onAdd ? t.surfaceHi : t.surface })
  buf.text(list.x + 2, ay, '+', { fg: onAdd ? t.accent : t.muted, bold: true })
  buf.text(list.x + 6, ay, 'Add Microsoft account', { fg: onAdd ? t.strong : t.dim, bold: onAdd })
  buf.region(add, `sel:${accts.length}`)
  hotbar(c, p.footer, [
    onAdd ? ['↵', 'add'] : ['↵', 'use'],
    ['o', 'offline player'],
    ['d', 'remove'],
    ['esc', 'back'],
    ['?', 'help'],
  ])
  overlays(c)
}

/** The Options screen: every setting a wide `Label: value` button, two by two. */
function settings(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  header(c, p.header, ['Options'])
  const items = data.settings
  const sel = c.sel(items.length)
  const col = rect(p.body.x + Math.max(0, Math.floor((p.body.w - 72) / 2)), p.body.y, Math.min(72, p.body.w), p.body.h)
  const half = Math.floor((col.w - 2) / 2)
  const shown = (s: (typeof items)[number]) => (s.placeholder ? s.value : s.value)
  let y = col.y
  let group = ''
  let k = 0
  items.forEach((s, i) => {
    if (s.group !== group) {
      if (group) y += k % 2 ? 2 : 0
      group = s.group
      k = 0
      buf.lineCenter(rect(col.x, y, col.w, 1), [[s.group, { fg: t.muted, bold: true }]])
      y += 2
    }
    const bx = k % 2 ? col.x + half + 2 : col.x
    const bw = k % 2 ? col.w - half - 2 : half
    const value = s.kind === 'toggle' ? (s.value === 'On' ? 'ON' : 'OFF') : shown(s)
    button(c, rect(bx, y, bw, 1), `${s.label}: ${value}`, undefined, 'secondary', `sel:${i}`, sel === i)
    if (k % 2) y += 2
    k++
  })
  if (k % 2) y += 2
  const hint: Record<string, string> = {
    Theme: '←→ previews each theme. auto follows your terminal’s light or dark background.',
    'Show snapshots': 'Adds weekly test builds to the version list. They can break worlds.',
    Java: 'Automatic picks the right Java for each version and downloads it when missing.',
    'JVM arguments': 'Passed to every instance unless it has its own.',
    'Microsoft client ID': 'Only needed if you registered your own Azure app.',
  }
  wrap(hint[items[sel].label] ?? '', col.w)
    .slice(0, 2)
    .forEach((l, i) => buf.lineCenter(rect(col.x, y + i, col.w, 1), [[l, { fg: t.dim }]]))
  button(c, rect(col.x + Math.floor((col.w - 30) / 2), bottom(col) - 1, 30, 1), 'Done', 'esc', 'secondary', 'screen:home')
  hotbar(c, p.footer, [
    ['↵', 'change'],
    ['←→', 'cycle'],
    ['esc', 'done'],
    ['?', 'help'],
  ])
  overlays(c)
}

/** Overlays in the game's own dialects: Controls, chat commands, the pause menu, "Are you sure?". */
function overlays(c: Ctx) {
  const { buf, t, ui, data } = c
  if (!ui.overlay) return
  const inst = data.instances[0]

  if (ui.overlay === 'help') {
    // The Controls screen: action, dotted leader, key.
    const W = Math.min(72, buf.cols - 8)
    const r = panel(c, W, Math.min(17, buf.rows - 2), [['Controls', {}]])
    const two = r.w >= 60
    const [a, , b] = two ? cols(r, ['1fr', 4, '1fr']) : [r, r, r]
    const block = (col: Rect, y: number, title: string, keys: [string, string][]) => {
      buf.text(col.x, y, title, { fg: t.accent, bold: true })
      keys.forEach(([k, d], i) => {
        const ry = y + 1 + i
        const lx = buf.text(col.x, ry, d, { fg: t.text })
        const kx = right(col) - k.length
        buf.hline(lx + 1, ry, Math.max(0, kx - lx - 2), { fg: t.faint }, '·')
        buf.text(kx, ry, k, { fg: t.strong, bold: true })
      })
      return y + keys.length + 2
    }
    const move: [string, string][] = [['↑↓ j k', 'Move'], ['tab', 'List ↔ details'], ['←→ 1-9', 'Tabs'], ['/', 'Filter']]
    const act: [string, string][] = [['↵', 'Play'], ['o', 'Play offline'], ['n', 'New instance'], ['e', 'Edit'], ['d ⌫', 'Delete']]
    const inv: [string, string][] = [['m', 'Mods'], ['p', 'Resource packs'], ['a', 'Accounts'], ['s', 'Settings']]
    const misc: [string, string][] = [[':', 'Commands'], ['?', 'Controls'], ['q', 'Quit']]
    if (two) {
      block(a, block(a, r.y, 'Movement', move), 'Gameplay', act)
      block(b, block(b, r.y, 'Inventory', inv), 'Miscellaneous', misc)
    } else block(a, block(a, r.y, 'Movement', move), 'Gameplay', act)
  } else if (ui.overlay === 'palette') {
    // Chat commands: suggestions stacked over the input bar, bottom left.
    ghost(c)
    buf.region(buf.area(), 'close')
    const cmds: [string, string][] = [
      [`/play ${inst.name}`, 'start the game'],
      ['/new', 'create an instance'],
      ['/mods', 'open the inventory'],
      ['/packs', 'resource packs'],
      ['/accounts', 'switch player'],
      ['/settings', 'theme, Java, memory'],
      ['/folder', 'open in Finder'],
    ]
    const q = ui.oquery.replace(/^\//, '').toLowerCase()
    const list = cmds.filter(([cmd]) => cmd.slice(1).toLowerCase().includes(q))
    const sel = c.osel(Math.max(1, list.length))
    const W = Math.min(60, buf.cols - 4)
    const x = 2
    const inputY = buf.rows - 3
    buf.clear(rect(0, inputY - list.length - 1, W + 4, list.length + 1))
    buf.clear(rect(0, inputY, buf.cols, 3))
    list.forEach(([cmd, what], i) => {
      const y = inputY - list.length + i
      const row = rect(x, y, W, 1)
      const on = sel === i
      if (on) buf.fill(row, { bg: t.selection })
      else if (c.hovered(`osel:${i}`)) buf.fill(row, { bg: t.surface })
      const cx = buf.text(x + 1, y, cmd, { fg: on ? t.strong : t.text, bold: on })
      if (W - (cx - x) > what.length + 4) buf.lineRight(rect(x, y, W - 1, 1), [[what, { fg: on ? t.subtle : t.muted }]])
      buf.region(row, `osel:${i}`)
    })
    const bar = rect(x, inputY + 1, buf.cols - 4, 1)
    buf.fill(bar, { bg: t.surface })
    const typed = ui.oquery ? (ui.oquery.startsWith('/') ? ui.oquery : `/${ui.oquery}`) : '/'
    const ex = buf.text(x + 1, bar.y, typed, { fg: t.strong })
    buf.set(ex, bar.y, '_', { fg: t.strong, bold: true })
    buf.lineRight(rect(x, bar.y, bar.w - 1, 1), [['↵ run  ·  tab complete  ·  esc close', { fg: t.muted }]])
    buf.region(bar, 'card')
  } else if (ui.overlay === 'menu') {
    // The pause menu: one wide button, then pairs, then the way out.
    const W = Math.min(52, buf.cols - 8)
    const r = panel(c, W, 14, [[inst.name, {}]])
    const sel = c.osel(8)
    const half = Math.floor((r.w - 2) / 2)
    const row = (y: number, items: [string, string | undefined, number][], wide = false) =>
      items.forEach(([label, key, i], k) => {
        const bx = wide ? r.x : r.x + k * (half + 2)
        const bw = wide ? r.w : k ? r.w - half - 2 : half
        const kind = i === 0 ? 'primary' : 'secondary'
        button(c, rect(bx, y, bw, 1), label, key, kind, `osel:${i}`, sel === i && i !== 0, i === 7 ? { fg: t.error } : {})
      })
    row(r.y, [['▶ Play', '↵', 0]], true)
    row(r.y + 2, [['Play offline', 'o', 1], ['Edit', 'e', 2]])
    row(r.y + 4, [['Mods', 'm', 3], ['Resource packs', 'p', 4]])
    row(r.y + 6, [['Open folder', 'f', 5], ['Duplicate', undefined, 6]])
    row(r.y + 8, [['Delete instance', 'd', 7]], true)
  } else if (ui.overlay === 'confirm') {
    // "Are you sure you want to delete this world?", word for word where it fits.
    const W = Math.min(60, buf.cols - 8)
    const r = panel(c, W, 11, [['Are you sure you want to delete this instance?', {}]], { danger: true })
    buf.lineCenter(rect(r.x, r.y, r.w, 1), [[`“${inst.name}” will be lost forever!`, { fg: t.text }], [' (A long time!)', { fg: t.muted }]])
    buf.lineCenter(rect(r.x, r.y + 1, r.w, 1), [[`Its ${inst.worlds.length} worlds and ${inst.mods.length} mods go with it.`, { fg: t.dim }]])
    const half = Math.floor((r.w - 2) / 2)
    button(c, rect(r.x, r.y + 3, half, 1), 'Delete', 'y', 'danger', 'delete')
    button(c, rect(r.x + half + 2, r.y + 3, r.w - half - 2, 1), 'Cancel', '↵', 'secondary', 'close', true)
  }
}

export default defineVariation({
  id: 'blocky',
  archived: 'Set aside: icons and sprites distract; a TUI has no sensible way to make them.',
  name: 'Blocky',
  order: 50,
  blurb:
    'Minecraft-native: one-row pixel sprites for every instance and mod, a hotbar footer, XP-bar progress, a chat-style log and mods as a creative inventory with item tooltips. Trades some calm and density for charm and instant recognition, and leans on game idioms a non-player won’t read.',
  notes: [
    'Sprites are one row: `▄` with fg = bottom pixel and bg = top pixel (`▀` alone when the bottom is see-through) makes each cell two square pixels, so nothing stripes. Lower halves, because font-drawn upper halves leave a sliver of bg above them.',
    'Game colours (grass, dirt, stone, diamond, gold…) live in sprites.ts, only ever paint pixels, and ease toward the page on light themes. Text and chrome stay on theme roles; yellow is the splash and join messages, green the XP bar.',
    'Footer = hotbar: equal slots, centred, the ↵ action wearing the selected-slot frame (▐ ▌). No slot digits: 1–9 already switch tabs, so numbers would lie.',
    'Mods = creative inventory: category tabs with icons, a 9-wide grid (↑↓ walk slots in reading order), a tooltip that follows the selected row (accent name, grey lore, italic source line), the instance’s mods as the hotbar below, ✓ ↑ ↓ where the stack count goes.',
    'Game dialects elsewhere: world-list rows (hover the icon for ▶), chat log (<Steve>, yellow joins, green advancements), Create New World tabs on a rule, Options as `Label: value` buttons, Controls with dotted leaders, a /command palette, the pause menu, “(A long time!)”.',
  ],
  screens: { home, mods, wizard, play, accounts, settings },
})
