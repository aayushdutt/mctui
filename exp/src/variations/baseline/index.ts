// Today's design (DESIGN.md), redrawn with the lab kit. The reference point
// every direction diverges from, and a template to fork.

import { defineVariation, type Ctx } from '../../kit'
import { cols, rect, right, bottom, wrap, type Line } from '../../term/buffer'
import * as w from '../../term/widgets'

function home(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  w.header(c, p.header)
  const focus = c.panes(2)
  const insts = data.instances
  const sel = c.sel(insts.length + 1)
  const listW = Math.min(46, Math.max(30, Math.floor((p.body.w * 38) / 100)))
  const [list, , detail] = cols(p.body, [listW, 2, '1fr'])

  w.section(c, list.x + 2, list.y, 'Instances', insts.length, focus === 0)
  insts.forEach((inst, i) => {
    const y = list.y + 2 + i * 2
    if (y >= bottom(list)) return
    const row = rect(list.x, y, list.w, 1)
    const hovered = c.hovered(`sel:${i}`)
    const inner = w.band(c, row, focus === 0 && sel === i, hovered)
    const accessory: Line = inst.trouble
      ? [['▲ Crashed', { fg: t.warning }]]
      : inst.fresh
        ? [['new', { fg: t.secondary }]]
        : [[inst.lastPlayed ?? '', { fg: t.dim }]]
    const ax = buf.lineRight(inner, accessory)
    buf.text(inner.x, y, inst.name, w.rowLabel(c, focus === 0 && sel === i, hovered), ax - inner.x - 2)
    buf.region(row, `sel:${i}`)
  })
  const newY = list.y + 2 + insts.length * 2
  const newRow = rect(list.x, newY, list.w, 1)
  const newInner = w.band(c, newRow, focus === 0 && sel === insts.length, c.hovered(`sel:${insts.length}`))
  buf.text(newInner.x, newY, '+ New instance', { fg: sel === insts.length && focus === 0 ? t.strong : t.dim })
  buf.region(newRow, `sel:${insts.length}`)

  c.enter(sel === insts.length ? 'wizard' : 'play')
  const inst = insts[Math.min(sel, insts.length - 1)]
  const d = rect(detail.x + 2, detail.y, detail.w - 2, detail.h)
  buf.text(d.x, d.y, inst.name, { fg: t.strong, bold: true })
  const loader = inst.loader === 'Vanilla' ? 'Vanilla' : `${inst.loader} ${inst.loaderVersion}`
  buf.text(d.x, d.y + 1, `Minecraft ${inst.mc}  ·  ${loader}`, { fg: t.dim })
  const status: [string, 'success' | 'warning'] = inst.trouble ? ['▲ Crashed', 'warning'] : ['● Ready', 'success']
  const chipW = status[0].length + 2
  w.chip(c, right(d) - chipW, d.y, [[status[0], { fg: inst.trouble ? t.warning : t.success }]], status[1])

  const by = d.y + 3
  const play = w.button(c, d.x, by, '▶ Play', '↵', 'primary', 'play')
  w.button(c, right(play) + 2, by, 'Play offline', 'o', 'secondary', 'offline')

  const tab = c.tabs(3)
  w.tabs(c, rect(d.x, by + 2, d.w, 1), [['Overview'], ['Mods', inst.mods.length], ['Resource packs', inst.packs.length]], tab, focus === 1)

  const body = rect(d.x, by + 4, d.w, bottom(d) - by - 4)
  if (tab === 0) {
    const used = w.stats(c, body, [
      [inst.playtime, 'played'],
      [inst.lastPlayed ?? 'never', 'last played'],
      [String(inst.worlds.length), 'worlds'],
      [String(inst.mods.length), 'mods'],
    ])
    const wy = body.y + used + 2
    w.section(c, d.x, wy, 'Worlds', inst.worlds.length)
    if (!inst.worlds.length) buf.text(d.x, wy + 2, 'No worlds yet — they show up after you play.', { fg: t.faint })
    inst.worlds.forEach((world, i) => {
      const r = rect(d.x, wy + 2 + i, d.w, 1)
      buf.text(r.x, r.y, world.name, { fg: t.text })
      buf.lineRight(r, [[world.lastPlayed, { fg: t.dim }]])
    })
  } else {
    const items = tab === 1 ? inst.mods.map((m) => [m.name, m.version] as const) : inst.packs.map((p) => [p, ''] as const)
    if (!items.length) buf.text(body.x, body.y, tab === 1 ? 'No mods yet — press m to add some.' : 'No resource packs.', { fg: t.faint })
    items.forEach(([name, ver], i) => {
      const r = rect(body.x, body.y + i * 2, body.w, 1)
      buf.text(r.x, r.y, name, { fg: t.text })
      buf.lineRight(r, [[ver, { fg: t.dim }]])
    })
  }

  w.keycaps(c, p.footer.x, p.footer.y, [
    ['↵', 'play'],
    ['o', 'offline'],
    ['n', 'new'],
    ['/', 'search'],
    [':', 'commands'],
    ['?', 'help'],
  ])
  overlays(c)
}

function mods(c: Ctx) {
  const { buf, t, data, ui } = c
  const p = w.page(c)
  const inst = data.instances[0]
  w.header(c, p.header, [inst.name, 'Mods'])
  const focus = c.panes(2)
  const [left, , rightPane] = cols(p.body, [30, 4, '1fr'])

  w.section(c, left.x + 2, left.y, 'Installed', inst.mods.length, focus === 0)
  buf.text(left.x + 2, left.y + 2, `Minecraft ${inst.mc}  ·  ${inst.loader}`, { fg: t.dim })
  inst.mods.forEach((m, i) => {
    const r = rect(left.x + 2, left.y + 4 + i * 2, left.w - 4, 1)
    buf.text(r.x, r.y, m.name, { fg: t.text })
    buf.lineRight(r, [[m.version, { fg: t.dim }]])
  })

  const hits = data.modHits.filter((h) => h.name.toLowerCase().includes(ui.query.toLowerCase()))
  const sel = c.sel(Math.max(1, hits.length))
  w.section(c, rightPane.x + 2, rightPane.y, 'Discover', `${hits.length} of ${data.modTotal}`, focus === 1)
  w.search(c, rect(rightPane.x, rightPane.y + 2, rightPane.w, 1), ui.query, 'Search Modrinth', undefined, ui.typing)

  const detailH = 8
  const listArea = rect(rightPane.x, rightPane.y + 4, rightPane.w, rightPane.h - 4 - detailH - 1)
  const fit = Math.max(1, Math.floor((listArea.h + 1) / 2))
  const { start, shown } = w.scroll(hits, sel, fit)
  shown.forEach((h, j) => {
    const i = start + j
    const r = rect(listArea.x, listArea.y + j * 2, listArea.w, 1)
    const hovered = c.hovered(`sel:${i}`)
    const on = focus === 1 && sel === i
    const inner = w.band(c, r, on, hovered)
    buf.lineRight(inner, h.installed ? [['✓ Installed', { fg: t.success }]] : [[h.downloads, { fg: t.dim }]])
    buf.text(inner.x, r.y, h.name, w.rowLabel(c, on, hovered))
    buf.region(r, `sel:${i}`)
  })
  w.more(c, listArea.x + 2, listArea.y + shown.length * 2, hits.length - start - shown.length)

  const hit = hits[sel]
  if (hit) {
    const d = rect(rightPane.x + 2, bottom(rightPane) - detailH, rightPane.w - 4, detailH)
    buf.text(d.x, d.y, hit.name, { fg: t.strong, bold: true })
    buf.text(d.x, d.y + 1, `by ${hit.author}  ·  ${hit.category}  ·  ${hit.side}`, { fg: t.dim })
    wrap(hit.description, d.w)
      .slice(0, 2)
      .forEach((l, i) => buf.text(d.x, d.y + 2 + i, l, { fg: t.text }))
    buf.text(d.x, d.y + 4, `${hit.downloads} downloads  ·  Updated ${hit.updated}`, { fg: t.muted })
    const b = w.button(c, d.x, d.y + 6, hit.installed ? 'Remove' : 'Install', '↵', hit.installed ? 'secondary' : 'primary', 'install')
    w.button(c, right(b) + 2, d.y + 6, 'Modrinth page', 'o', 'secondary', 'page')
  }

  w.keycaps(c, p.footer.x, p.footer.y, [
    ['↵', 'install'],
    ['o', 'page'],
    ['/', 'search'],
    ['esc', 'back'],
    ['?', 'help'],
  ])
  overlays(c)
}

function wizard(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  w.header(c, p.header, ['New instance'])
  let x = p.body.x + 1
  ;['Version', 'Loader', 'Name'].forEach((step, i) => {
    if (i) x = buf.text(x + 1, p.body.y, '›', { fg: t.faint }) + 1
    x = right(w.chip(c, x, p.body.y, [[step, i === 0 ? { fg: t.strong, bold: true } : { fg: t.dim }]], i === 0 ? 'selected' : 'plain', `tab:${i}`))
  })
  const [list, , summary] = cols(rect(p.body.x, p.body.y + 2, p.body.w, p.body.h - 2), ['62%', 4, '1fr'])
  const versions = data.versions.filter((v) => !v.snapshot)
  const sel = c.sel(versions.length)
  w.section(c, list.x + 2, list.y, 'Releases', versions.length, true)
  const tog = w.toggle(c, right(list) - 9, list.y, false, false, 'snapshots')
  buf.text(tog.x - 11, list.y, 'Snapshots', { fg: t.dim })
  const area = rect(list.x, list.y + 2, list.w, list.h - 2)
  const fit = Math.max(1, Math.floor(area.h / 2))
  const { start, shown } = w.scroll(versions, sel, fit)
  shown.forEach((v, j) => {
    const i = start + j
    const r = rect(area.x, area.y + j * 2, area.w, 1)
    const hovered = c.hovered(`sel:${i}`)
    const inner = w.band(c, r, sel === i, hovered)
    if (v.latest) w.chip(c, right(inner) - 7, r.y, [['latest', { fg: t.success }]], 'success')
    else buf.lineRight(inner, [[v.date, { fg: t.dim }]])
    buf.text(inner.x, r.y, v.id, w.rowLabel(c, sel === i, hovered))
    buf.region(r, `sel:${i}`)
  })
  w.more(c, area.x + 2, bottom(area) - 1, versions.length - start - shown.length)

  w.section(c, summary.x, summary.y, 'Summary')
  w.kv(c, rect(summary.x, summary.y + 2, summary.w, 1), 'Minecraft', [[versions[sel].id, { fg: t.strong }]], 14)
  w.kv(c, rect(summary.x, summary.y + 3, summary.w, 1), 'Loader', [['—', { fg: t.faint }]], 14)
  w.kv(c, rect(summary.x, summary.y + 4, summary.w, 1), 'Name', [['—', { fg: t.faint }]], 14)

  w.keycaps(c, p.footer.x, p.footer.y, [
    ['↵', 'next'],
    ['/', 'filter'],
    ['space', 'show snapshots'],
    ['esc', 'cancel'],
    ['?', 'help'],
  ])
  overlays(c)
}

function play(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  const inst = data.instances[0]
  w.header(c, p.header, [inst.name, 'Playing'])
  const a = rect(p.body.x + 2, p.body.y, p.body.w - 4, p.body.h)
  const chip = `${c.spinner()} Running · 18m`
  w.chip(c, right(a) - chip.length - 2, a.y, [[chip, { fg: t.success }]], 'success')
  buf.text(a.x, a.y, inst.name, { fg: t.strong, bold: true })
  buf.text(a.x, a.y + 1, `Minecraft ${inst.mc} with Fabric  ·  as Steve`, { fg: t.dim })
  w.button(c, a.x, a.y + 3, 'Stop', 's', 'secondary', 'stop')

  const logArea = rect(a.x, a.y + 5, a.w, bottom(a) - a.y - 5)
  w.section(c, logArea.x, logArea.y, 'Game log', data.log.length)
  buf.lineRight(rect(logArea.x, logArea.y, logArea.w, 1), [['everything', { fg: t.subtle }], ['  v', { fg: t.muted }]])
  const body = rect(logArea.x, logArea.y + 2, logArea.w, logArea.h - 2)
  const lines = data.log.slice(-body.h)
  lines.forEach((l, i) => {
    const fg = l.level === 'ERROR' ? t.error : l.level === 'WARN' ? t.warning : t.subtle
    const x = buf.text(body.x, body.y + i, l.time, { fg: t.faint })
    buf.text(x + 2, body.y + i, l.text, { fg }, body.w - (x + 2 - body.x))
  })

  w.keycaps(c, p.footer.x, p.footer.y, [
    ['s', 'stop'],
    ['v', 'log level'],
    ['c', 'copy log'],
    ['esc', 'back'],
  ])
  overlays(c)
}

function accounts(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  w.header(c, p.header, ['Accounts'])
  const list = rect(p.body.x, p.body.y, Math.min(72, p.body.w), p.body.h)
  const accts = data.accounts
  const sel = c.sel(accts.length + 1)
  w.section(c, list.x + 2, list.y, 'Profiles', accts.length, true)
  accts.forEach((a, i) => {
    const r = rect(list.x, list.y + 2 + i * 2, list.w, 1)
    const hovered = c.hovered(`sel:${i}`)
    const inner = w.band(c, r, sel === i, hovered)
    const [label, tone, fg] =
      a.status === 'active'
        ? (['● Active', 'success', t.success] as const)
        : a.status === 'expired'
          ? (['▲ Sign-in expired', 'warning', t.warning] as const)
          : (['Offline', 'neutral', t.dim] as const)
    w.chip(c, right(r) - label.length - 2, r.y, [[label, { fg }]], tone)
    buf.text(inner.x, r.y, a.name, w.rowLabel(c, sel === i, hovered))
    buf.region(r, `sel:${i}`)
  })
  const ay = list.y + 2 + accts.length * 2
  const add = rect(list.x, ay, list.w, 1)
  const inner = w.band(c, add, sel === accts.length, c.hovered(`sel:${accts.length}`))
  buf.text(inner.x, ay, '+ Add Microsoft account', { fg: sel === accts.length ? t.strong : t.dim })
  buf.region(add, `sel:${accts.length}`)
  w.keycaps(c, p.footer.x, p.footer.y, [
    ['n', 'add account'],
    ['o', 'offline profile'],
    ['d', 'remove'],
    ['esc', 'back'],
    ['?', 'help'],
  ])
  overlays(c)
}

function settings(c: Ctx) {
  const { buf, t, data } = c
  const p = w.page(c)
  w.header(c, p.header, ['Settings'])
  const items = data.settings
  const sel = c.sel(items.length)
  let y = p.body.y
  let group = ''
  items.forEach((s, i) => {
    if (s.group !== group) {
      if (group) y += 0
      group = s.group
      w.section(c, p.body.x + 2, y, s.group)
      y += 2
    }
    const on = sel === i
    buf.text(p.body.x + 2, y, s.label, on ? { fg: t.strong, bold: true } : { fg: t.dim })
    const vx = p.body.x + 24
    if (s.kind === 'choice') w.choice(c, vx, y, s.value, on, `sel:${i}`)
    else if (s.kind === 'toggle') w.toggle(c, vx, y, s.value === 'On', on, `sel:${i}`)
    else w.field(c, rect(vx, y, 52, 1), s.placeholder ? '' : s.value, s.value, on, `sel:${i}`)
    y += 2
  })
  wrap("←→ previews each theme. auto follows your terminal's light or dark background.", 52).forEach((l, i) =>
    buf.text(p.body.x + 25, y + i, l, { fg: t.muted }),
  )
  w.keycaps(c, p.footer.x, p.footer.y, [
    ['↵', 'save'],
    ['←→', 'change'],
    ['tab', 'next'],
    ['esc', 'back'],
  ])
  overlays(c)
}

/** The shared overlays: help, palette, menu, confirm. */
function overlays(c: Ctx) {
  const { buf, t, ui, data } = c
  if (!ui.overlay) return
  if (ui.overlay === 'help') {
    const r = w.card(c, 70, 22, 'Home · keys')
    const [a, , b] = cols(r, ['1fr', 4, '1fr'])
    const block = (x: number, y: number, title: string, keys: [string, string][]) => {
      buf.text(x, y, title, { fg: t.strong, bold: true })
      keys.forEach(([k, d], i) => {
        buf.text(x, y + 1 + i, k, { fg: t.accent, bold: true })
        buf.text(x + 14, y + 1 + i, d, { fg: t.dim })
      })
      return y + keys.length + 2
    }
    const y = block(a.x, a.y, 'Navigate', [['↑↓ j k', 'move'], ['pgup pgdn', 'page'], ['tab', 'list ↔ details'], ['→ l ]', 'next tab']])
    block(a.x, y, 'Act', [['↵', 'play'], ['o', 'play offline'], ['n', 'new instance'], ['e', 'edit instance'], ['d ⌫', 'delete']])
    const y2 = block(b.x, b.y, 'Screens', [['m', 'mods'], ['p', 'resource packs'], ['s', 'settings'], ['a', 'accounts']])
    block(b.x, y2, 'General', [[':', 'command palette'], ['/', 'filter'], ['?', 'help'], ['q', 'quit']])
  } else if (ui.overlay === 'palette') {
    const r = w.card(c, 60, 16, 'Commands')
    w.field(c, rect(r.x, r.y, r.w, 1), ui.oquery, 'Type a command…', true)
    const cmds = ['Play Survival 1.21', 'New instance', 'Browse mods', 'Resource packs', 'Accounts', 'Settings', 'Open folder']
    const sel = c.osel(cmds.length)
    cmds.slice(0, r.h - 2).forEach((cmd, i) => {
      const row = rect(r.x - 2, r.y + 2 + i, r.w + 4, 1)
      const inner = w.band(c, row, sel === i, c.hovered(`osel:${i}`))
      buf.text(inner.x, row.y, cmd, w.rowLabel(c, sel === i, false))
      buf.region(row, `osel:${i}`)
    })
  } else if (ui.overlay === 'menu') {
    const r = w.card(c, 36, 14, data.instances[Math.min(ui.sel, data.instances.length - 1)].name)
    const items = ['Play', 'Play offline', 'Edit', 'Mods', 'Open folder', 'Delete']
    const sel = c.osel(items.length)
    items.forEach((it, i) => {
      const row = rect(r.x - 2, r.y + i, r.w + 4, 1)
      const inner = w.band(c, row, sel === i, c.hovered(`osel:${i}`))
      buf.text(inner.x, row.y, it, it === 'Delete' ? { fg: t.error } : w.rowLabel(c, sel === i, false))
      buf.region(row, `osel:${i}`)
    })
  } else if (ui.overlay === 'confirm') {
    const r = w.card(c, 52, 9, 'Delete “Survival 1.21”?', { danger: true })
    buf.text(r.x, r.y, 'Its worlds and mods go too. This can’t be undone.', { fg: t.text })
    const k = w.button(c, r.x, r.y + 2, 'Keep', '↵', 'secondary', 'close')
    w.button(c, right(k) + 2, r.y + 2, 'Delete', 'y', 'danger', 'delete')
  }
}

export default defineVariation({
  id: 'baseline',
  round: 0,
  name: 'Baseline',
  order: 0,
  blurb: 'Today’s design, redrawn with the lab kit: quiet chrome, one accent, single-row tints, blank line between rows.',
  notes: [
    'Reference for every other direction. Fork this folder to start a new one.',
    'Rows = one line + a blank; selection = a one-row accent-tinted band; buttons and chips are one-row pills.',
    'Compare against “Live app” to see the real renderer’s output.',
  ],
  screens: { home, mods, wizard, play, accounts, settings },
})

