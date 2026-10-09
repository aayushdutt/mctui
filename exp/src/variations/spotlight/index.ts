// Spotlight: command-first, no panes. One prompt, one ranked list whose
// selected result opens in place, and a helix-style status line.

import { defineVariation, type Ctx } from '../../kit'
import { type Line, rect, right, wrap, width } from '../../term/buffer'
import type { Instance } from '../../data/mock'
import * as w from '../../term/widgets'
import { fuzzy, substring } from './fuzzy'
import { page, prompt, results, status, acts, hl, drawer, popover, backdrop, head, facts, clamp, type Item, type Act } from './ui'
import { extraHits, ago, downloads, javaFor, loaderLabel, homeActions } from './data'

// ─── Home ───────────────────────────────────────────────────────────────────

function instanceItem(c: Ctx, inst: Instance): Item {
  const { t } = c
  const right: Line = inst.trouble
    ? [['▲ crashed', { fg: t.warning }]]
    : inst.fresh
      ? [['new', { fg: t.secondary }]]
      : [[inst.lastPlayed ?? '', { fg: t.dim }]]
  const v = (s: string): [string, { fg: string }] => [s, { fg: t.text }]
  const d = (s: string): [string, { fg: string }] => [s, { fg: t.dim }]
  const updates = inst.mods.filter((m) => m.update).length
  const detail: Line[] = []
  if (inst.trouble) detail.push([['▲ ', { fg: t.warning }], ['Crashed last time', { fg: t.warningSoft }], d(`, ${inst.lastPlayed}`)])
  if (inst.fresh) detail.push([d('Never played  ·  '), v(inst.worlds.length ? '' : 'no worlds yet')])
  else
    detail.push(
      facts(c, [
        [v(inst.playtime), d(' played')],
        [v(String(inst.worlds.length)), d(inst.worlds.length === 1 ? ' world' : ' worlds')],
        inst.mods.length || !inst.packs.length
          ? [v(String(inst.mods.length)), d(inst.mods.length === 1 ? ' mod' : ' mods')]
          : [v(String(inst.packs.length)), d(inst.packs.length === 1 ? ' pack' : ' packs')],
        ...(updates ? [[[`${updates} update`, { fg: t.warning }]] as Line] : []),
      ]),
    )
  detail.push(facts(c, [[d(loaderLabel(inst))], [d(inst.java.replace(' (automatic)', ''))], [d(`${inst.memory} memory`)]]))
  if (c.buf.rows >= 36 && inst.worlds.length)
    detail.push([d('Worlds  '), ...facts(c, inst.worlds.map((wd): Line => [v(wd.name), d(` ${wd.lastPlayed}`)]))])
  const list: Act[] = [
    { key: '↵', label: 'play', primary: true },
    { key: 'o', label: 'offline', region: 'offline' },
    { key: 'e', label: 'edit', region: 'edit' },
  ]
  if (inst.loader !== 'Vanilla') list.push({ key: 'm', label: 'mods', region: 'screen:mods' })
  else list.push({ key: 'r', label: 'packs', region: 'packs' })
  if (inst.trouble) list.splice(1, 0, { key: 'l', label: 'last log', region: 'screen:play' })
  list.push({ key: 'x', label: 'more', region: 'overlay:menu' })
  return {
    section: 'Instances',
    glyph: '■',
    title: inst.name,
    sub: [inst.mc, inst.loader === 'Vanilla' ? '' : inst.loader].filter((p) => p && !inst.name.includes(p)).join(' ') || undefined,
    right,
    detail,
    acts: list,
  }
}

function homeItems(c: Ctx): Item[] {
  const { t, data, ui } = c
  const q = ui.query.trim()
  const insts = [...data.instances].sort((a, b) => ago(a.lastPlayed) - ago(b.lastPlayed))
  const sel0 = insts[0]

  /** Keeps `it` when the query finds its title (or, weaker, its subtitle when `bySub`). */
  const keep = (it: Item, bySub = false): Item | null => {
    if (!q) return it
    const m = fuzzy(q, it.title)
    if (m) return { ...it, hits: m.hits, score: m.score }
    const s = bySub && it.sub ? fuzzy(q, it.sub) : null
    if (s) return { ...it, subHits: s.hits, score: s.score - 6 }
    return null
  }

  const instances = insts.map((i) => keep(instanceItem(c, i))).filter((x): x is Item => !!x)

  const actions = homeActions
    .filter((a) => q || !a.hidden)
    .map((a) =>
      keep({
        section: 'Actions',
        glyph: a.glyph,
        title: a.title,
        sub: a.title === 'Browse mods' ? `Modrinth, for ${sel0.name}` : a.sub,
        right: a.key ? [[a.key, { fg: t.muted, bold: true }]] : undefined,
        detail: actionDetail(c, a.title),
        acts: [
          { key: '↵', label: 'open', primary: true, region: a.region },
          ...(a.key ? [{ key: a.key, label: 'from anywhere', region: a.region }] : []),
        ],
      }),
    )
    .filter((x): x is Item => !!x)

  const worldsAll = data.instances
    .flatMap((inst) => inst.worlds.map((world) => ({ world, inst })))
    .sort((a, b) => ago(a.world.lastPlayed) - ago(b.world.lastPlayed))
  const worlds = (q ? worldsAll : worldsAll.slice(0, c.buf.rows >= 36 ? 5 : 3))
    .map(({ world, inst }) =>
      keep({
        section: q ? 'Worlds' : 'Recent worlds',
        glyph: '◇',
        title: world.name,
        sub: inst.name,
        right: [[world.lastPlayed, { fg: t.dim }]],
        detail: [
          facts(c, [
            [[world.mode, { fg: world.mode === 'Hardcore' ? t.warningSoft : t.text }]],
            [[world.size, { fg: t.text }]],
            [[`Minecraft ${inst.mc}`, { fg: t.dim }]],
          ]),
        ],
        acts: [
          { key: '↵', label: 'play this world', primary: true },
          { key: 'o', label: 'offline', region: 'offline' },
          { key: 'b', label: 'back up', region: 'backup' },
        ],
      }, true),
    )
    .filter((x): x is Item => !!x)

  const news = (q || c.buf.rows >= 36 ? data.news : [])
    .map((n) => {
      const snap = n.title.startsWith('Snapshot')
      const id = snap ? '26.3-snapshot-4' : '26.3'
      return keep({
        section: 'What’s new',
        glyph: '✦',
        title: n.title,
        right: [[n.date, { fg: t.dim }]],
        detail: [[[snap ? 'Try it without touching your worlds: ' : 'Start fresh on it: ', { fg: t.dim }], [`New instance with ${id}`, { fg: t.text }]]],
        acts: [
          { key: '↵', label: 'new instance', primary: true, region: 'screen:wizard' },
          { key: 'o', label: snap ? 'changelog' : 'release notes', region: 'notes' },
        ],
      })
    })
    .filter((x): x is Item => !!x)

  const out: Item[][] = [instances, actions, worlds, news]
  if (q) {
    const mods = data.instances
      .flatMap((inst) => inst.mods.map((m) => ({ m, inst })))
      .map(({ m, inst }) =>
        keep({
          section: 'Installed mods',
          glyph: m.enabled ? '✓' : '○',
          glyphFg: m.enabled ? t.success : t.faint,
          title: m.name,
          sub: `${inst.name}`,
          right: m.update ? [[`${m.version} → ${m.update}`, { fg: t.warning }]] : [[m.version, { fg: t.dim }]],
          detail: [[[m.enabled ? 'Enabled' : 'Disabled', { fg: m.enabled ? t.success : t.dim }], ['  ·  ', { fg: t.faint }], [`in ${inst.name}`, { fg: t.dim }]]],
          acts: [
            { key: '↵', label: 'open in mods', primary: true, region: 'screen:mods' },
            { key: 'space', label: m.enabled ? 'disable' : 'enable', region: 'toggle' },
          ],
        }),
      )
      .filter((x): x is Item => !!x)
    out.push(mods)
    for (const group of out) group.sort((a, b) => (b.score ?? 0) - (a.score ?? 0))
    // Sections follow their best hit: the top row is the top hit.
    out.sort((a, b) => (b[0]?.score ?? -99) - (a[0]?.score ?? -99))
    out.push([
      {
        section: 'Modrinth',
        glyph: '↗',
        title: `Search Modrinth for “${q}”`,
        right: [['m', { fg: t.muted, bold: true }]],
        detail: [[[`5,056 mods for Fabric ${sel0.mc}`, { fg: t.dim }]]],
        acts: [{ key: '↵', label: 'search', primary: true, region: 'screen:mods' }],
      },
    ])
  }
  return out.flat()
}

function actionDetail(c: Ctx, title: string): Line[] {
  const { t, data } = c
  const d = (s: string): [string, { fg: string }] => [s, { fg: t.dim }]
  const v = (s: string): [string, { fg: string }] => [s, { fg: t.text }]
  switch (title) {
    case 'New instance':
      return [[d('Latest release '), v(data.versions.find((x) => x.latest)?.id ?? ''), d('  ·  any version back to '), v('1.8.9')]]
    case 'Browse mods':
      return [[v('5,056'), d(' mods for Fabric 1.21.4  ·  '), ['1 update waiting', { fg: t.warning }]]]
    case 'Accounts':
      return [
        facts(c, [
          [['● ', { fg: t.success }], v('Steve')],
          [['▲ ', { fg: t.warning }], d('jeb_ needs to sign in')],
          [d('Notch offline')],
        ]),
      ]
    case 'Settings':
      return [facts(c, data.settings.slice(0, 3).map((s) => [d(`${s.label} `), v(s.value.toLowerCase())]))]
    default:
      return []
  }
}

function home(c: Ctx) {
  const { t, ui } = c
  c.panes(1)
  const { col, list } = page(c)
  const items = homeItems(c)
  const sel = c.sel(Math.max(1, items.length))
  const q = ui.query.trim()
  const local = items.filter((it) => it.section !== 'Modrinth').length
  prompt(c, col, {
    placeholder: 'Search instances, actions, mods…',
    right: q ? [[local ? `${local} ${local === 1 ? 'result' : 'results'}` : 'no matches here', { fg: t.muted }]] : [['mctui', { fg: t.faint, bold: true }]],
  })
  results(c, list, items, sel, {
    counts: { Modrinth: [['for more mods', { fg: t.faint }]] },
    empty: [['Nothing matches “', {}], [q, { fg: t.text }], ['”.', {}]],
  })
  const it = items[sel]
  status(c, {
    mode: 'HOME',
    context: it ? [[it.title, { fg: t.text }], [`  ${sel + 1}/${items.length}`, { fg: t.muted }]] : [],
    hints: ui.typing
      ? [
          { key: '↑↓', label: 'pick' },
          { key: '↵', label: 'done' },
          { key: 'esc', label: 'clear' },
        ]
      : [
          { key: '/', label: 'search', region: 'query' },
          { key: ':', label: 'commands', region: 'overlay:palette' },
          { key: '?', label: 'help', region: 'overlay:help' },
        ],
  })
  overlays(c, items[sel])
}

// ─── Mods ───────────────────────────────────────────────────────────────────

function mods(c: Ctx) {
  const { t, data, ui } = c
  c.panes(1)
  const { col, list } = page(c)
  const inst = data.instances[0]
  const q = ui.query.trim()
  const all = [...data.modHits, ...extraHits].sort((a, b) => downloads(b.downloads) - downloads(a.downloads))
  const found = all
    .map((h) => ({ h, m: fuzzy(q, h.name) }))
    .filter((x) => x.m)
    .sort((a, b) => (q ? b.m!.score - a.m!.score : 0))
  const items: Item[] = found.map(({ h, m }) => {
    const mine = inst.mods.find((x) => x.name === h.name)
    const upd = mine?.update
    const desc = clamp(h.description, list.w - 3, 2)
    const acts: Act[] = !mine
      ? [
          { key: '↵', label: 'install', primary: true, region: 'install' },
          { key: 'o', label: 'Modrinth page', region: 'page' },
          { key: 'v', label: 'versions', region: 'versions' },
        ]
      : [
          upd
            ? { key: '↵', label: `update to ${upd}`, primary: true, region: 'update' }
            : { key: '↵', label: 'Modrinth page', primary: true, region: 'page' },
          { key: 'space', label: 'disable', region: 'toggle' },
          ...(upd ? [{ key: 'o', label: 'Modrinth page', region: 'page' }] : [{ key: 'v', label: 'versions', region: 'versions' }]),
          { key: 'd', label: 'remove', region: 'overlay:confirm' },
        ]
    return {
      section: 'Modrinth',
      glyph: mine ? '✓' : '↓',
      glyphFg: mine ? t.success : t.faint,
      title: h.name,
      hits: m!.hits,
      sub: h.author,
      right: upd ? [[`${upd} available  `, { fg: t.warning }], [h.downloads, { fg: t.dim }]] : [[h.downloads, { fg: t.dim }]],
      detail: [
        ...desc.map((l): Line => [[l, { fg: t.text }]]),
        facts(c, [
          ...(mine ? [[[`✓ installed ${mine.version}`, { fg: t.success }]] as Line] : []),
          [[h.category, { fg: t.dim }]],
          [[h.side, { fg: t.dim }]],
          [[`updated ${h.updated}`, { fg: t.dim }]],
        ], list.w - 3),
      ],
      acts,
    }
  })
  const sel = c.sel(Math.max(1, items.length))
  prompt(c, col, {
    crumbs: [{ label: 'mods', current: true, region: 'screen:home' }],
    placeholder: 'Search Modrinth…',
    right: [[q ? `${items.length} of ${data.modTotal}` : data.modTotal, { fg: t.muted }], [q ? '' : ' mods', { fg: t.faint }]],
  })
  results(c, list, items, sel, {
    counts: { Modrinth: [[q ? 'best match first' : 'most downloaded first', { fg: t.faint }]] },
    empty: [['No mods match “', {}], [q, { fg: t.text }], ['” for Fabric 1.21.4.', {}]],
  })
  status(c, {
    mode: 'MODS',
    context: [
      [inst.name, { fg: t.text }],
      ['  ', {}],
      [`Fabric ${inst.mc}`, { fg: t.dim }],
    ],
    hints: ui.typing
      ? [
          { key: '↑↓', label: 'pick' },
          { key: '↵', label: 'done' },
          { key: 'esc', label: 'clear' },
        ]
      : [
          { key: '/', label: 'search', region: 'query' },
          { key: 'i', label: 'installed only', region: 'installed' },
          { key: 'esc', label: 'home', region: 'screen:home' },
        ],
  })
}

// ─── Game log ───────────────────────────────────────────────────────────────

function play(c: Ctx) {
  const { buf, t, data, ui } = c
  c.panes(1)
  const { col, list } = page(c, true)
  const inst = data.instances[0]
  const q = ui.query.trim()
  const log = data.log
  const found = log.map((l, i) => ({ l, i, hits: substring(q, l.text) })).filter((x) => x.hits)
  const sel = c.sel(Math.max(1, q ? found.length : 1))

  prompt(c, col, {
    crumbs: [{ label: 'log', current: true }],
    placeholder: 'Filter the log…',
    right: q ? [[`${found.length} of ${log.length}`, { fg: t.muted }]] : [[`${log.length} lines`, { fg: t.muted }]],
  })

  const levelMark = (lv: string): [string, { fg: string }] =>
    lv === 'ERROR' ? ['✗', { fg: t.error }] : lv === 'WARN' ? ['▲', { fg: t.warning }] : [' ', { fg: t.faint }]
  const textFg = (lv: string) => (lv === 'ERROR' ? t.error : lv === 'WARN' ? t.warning : t.subtle)
  const tx = list.x + 12

  if (!q) {
    // Following the tail: the newest line sits just above the status line.
    const clipped = log.length > list.h
    const shown = log.slice(-(clipped ? list.h - 1 : list.h))
    const y0 = list.y + list.h - shown.length
    if (clipped) buf.text(tx, list.y, `↑ ${log.length - shown.length} earlier lines`, { fg: t.faint })
    shown.forEach((l, j) => {
      const y = y0 + j
      buf.text(list.x, y, l.time, { fg: t.faint })
      buf.text(list.x + 10, y, ...levelMark(l.level))
      buf.text(tx, y, l.text, { fg: textFg(l.level) }, right(list) - tx)
    })
  } else if (!found.length) {
    buf.line(tx, list.y, [['No lines contain “', {}], [q, { fg: t.text }], ['”.', {}]], { fg: t.faint })
  } else {
    // Filtered: one row per hit, the selected hit opens with its thread and
    // the full line.
    type R = { k: 'hit'; n: number } | { k: 'more'; line: Line } | { k: 'acts' }
    const rows: R[] = []
    found.forEach((f, n) => {
      rows.push({ k: 'hit', n })
      if (n === sel) {
        const full = wrap(f.l.text, list.w - 12)
        const meta: Line = facts(c, [[[f.l.thread, { fg: t.text }]], [[f.l.level.toLowerCase(), { fg: textFg(f.l.level) }]], [[`line ${f.i + 1}`, { fg: t.dim }]]])
        if (full.length > 1) full.forEach((l) => rows.push({ k: 'more', line: [[l, { fg: t.text }]] }))
        rows.push({ k: 'more', line: meta })
        rows.push({ k: 'acts' })
      }
    })
    const start = Math.max(0, Math.min(rows.findIndex((r) => r.k === 'hit' && r.n === sel), rows.length - list.h))
    rows.slice(start, start + list.h).forEach((r, j) => {
      const y = list.y + j
      if (r.k === 'hit') {
        const f = found[r.n]
        const on = r.n === sel
        const band = rect(list.x - 2, y, list.w + 4, 1)
        const hov = c.hovered(`sel:${r.n}`)
        if (on) buf.fill(band, { bg: t.selection })
        else if (hov) buf.fill(band, { bg: t.surface })
        buf.text(list.x, y, f.l.time, { fg: on ? t.subtle : t.faint })
        buf.text(list.x + 10, y, ...levelMark(f.l.level))
        hl(c, tx, y, f.l.text, f.hits!, { fg: on ? t.strong : textFg(f.l.level), bold: on }, right(list) - tx)
        buf.region(band, `sel:${r.n}`)
      } else if (r.k === 'more') buf.line(tx, y, r.line, { fg: t.dim }, list.w - 12)
      else
        acts(c, tx, y, [
          { key: '↵', label: 'show in the full log', primary: true, region: 'key:esc' },
          { key: 'c', label: 'copy line', region: 'copy' },
        ], list.w - 12)
    })
  }

  const spin = c.spinner()
  const secs = 18 * 60 + 4 + Math.floor((ui.tick * 80) / 1000)
  const clock = `${Math.floor(secs / 60)}m ${String(secs % 60).padStart(2, '0')}s`
  status(c, {
    mode: `${spin} RUNNING`,
    tone: 'success',
    context: [
      [inst.name, { fg: t.text }],
      ['  ', {}],
      [clock, { fg: t.dim }],
    ],
    hints: ui.typing
      ? [
          { key: '↑↓', label: 'pick' },
          { key: '↵', label: 'done' },
          { key: 'esc', label: 'clear' },
        ]
      : [
          { key: 's', label: 'stop', region: 'stop' },
          { key: '/', label: 'filter', region: 'query' },
          { key: 'c', label: 'copy', region: 'copy' },
          { key: 'esc', label: 'home', region: 'screen:home' },
        ],
  })
}

// ─── New instance ───────────────────────────────────────────────────────────

function wizard(c: Ctx) {
  const { t, data, ui } = c
  c.panes(1)
  const step = c.tabs(3)
  const { col, list } = page(c)
  const q = ui.query.trim()
  const version = '1.21.4'
  const loader = 'Fabric'
  const name = q && step === 2 ? q : `Fabric ${version}`

  const crumbs = [{ label: 'new', current: true, region: 'tab:0' }] as { label: string; current?: boolean; region?: string }[]
  if (step > 0) crumbs.push({ label: version, region: 'tab:0' })
  if (step > 1) crumbs.push({ label: loader, region: 'tab:1' })
  const question = ['Which Minecraft version?', 'Which loader?', `Name it (${name})`][step]

  let items: Item[] = []
  if (step === 0) {
    const versions = data.versions.filter((v) => !v.snapshot)
    items = versions
      .map((v) => ({ v, m: fuzzy(q, v.id) }))
      .filter((x) => x.m)
      .map(({ v, m }) => ({
        section: 'Releases',
        glyph: v.latest ? '●' : '·',
        glyphFg: v.latest ? t.success : t.faint,
        title: v.id,
        hits: m!.hits,
        sub: v.latest ? 'latest' : undefined,
        right: [[v.date, { fg: t.dim }]],
        detail: [
          facts(c, [
            [[javaFor(v.id), { fg: t.text }], [', fetched for you', { fg: t.dim }]],
            [['Fabric', { fg: t.text }], [' or Vanilla', { fg: t.dim }]],
            ...data.instances.filter((i) => i.mc === v.id).map((i): Line => [['used by ', { fg: t.dim }], [i.name, { fg: t.text }]]),
          ], list.w - 3),
        ],
        acts: [{ key: '↵', label: 'next: loader', primary: true, region: 'tab:1' }],
      }))
  } else if (step === 1) {
    items = [
      {
        section: `Loaders for ${version}`,
        glyph: '◆',
        title: 'Fabric',
        sub: 'recommended',
        right: [['0.16.10', { fg: t.dim }]],
        detail: [[['Light and fast; Sodium, Lithium and Iris need it.', { fg: t.text }]], [['Fabric API comes along.', { fg: t.dim }]]],
        acts: [
          { key: '↵', label: 'next: name', primary: true, region: 'tab:2' },
          { key: '←', label: 'back', region: 'tab:0' },
        ],
      },
      {
        section: `Loaders for ${version}`,
        glyph: '◇',
        title: 'Vanilla',
        sub: 'just the game',
        detail: [[['No mods; resource packs still work.', { fg: t.text }]]],
        acts: [
          { key: '↵', label: 'next: name', primary: true, region: 'tab:2' },
          { key: '←', label: 'back', region: 'tab:0' },
        ],
      },
    ].map((it) => ({ ...it, hits: fuzzy(q, it.title)?.hits }))
      .filter((it) => it.hits) as Item[]
  } else {
    const summary: Line[] = [
      facts(c, [[['Minecraft ', { fg: t.dim }], [version, { fg: t.text }]], [[`${loader} `, { fg: t.dim }], ['0.16.10', { fg: t.text }]], [[javaFor(version), { fg: t.text }]]]),
      facts(c, [[['4 GB', { fg: t.text }], [' memory', { fg: t.dim }]], [['about 180 MB to download', { fg: t.dim }]]]),
    ]
    items = [
      {
        section: 'Ready',
        glyph: '+',
        title: `Create “${name}”`,
        detail: summary,
        acts: [
          { key: '↵', label: 'create', primary: true, region: 'create' },
          { key: 'e', label: 'memory, Java…', region: 'edit' },
          { key: '←', label: 'back', region: 'tab:1' },
        ],
      },
      {
        section: 'Ready',
        glyph: '+',
        title: `Create “${name}”, then add mods`,
        detail: [[['Opens Modrinth scoped to it.', { fg: t.dim }]]],
        acts: [{ key: '↵', label: 'create', primary: true, region: 'screen:mods' }],
      },
    ]
  }
  const sel = c.sel(Math.max(1, items.length))
  prompt(c, col, {
    crumbs,
    placeholder: question,
    right: [[`${step + 1} of 3`, { fg: t.muted }]],
  })
  results(c, list, items, sel, {
    counts: step === 0 ? { Releases: [[`${items.length}`, { fg: t.faint }], ['   snapshots hidden', { fg: t.faint }]] } : {},
    empty: [['No release matches “', {}], [q, { fg: t.text }], ['”.', {}]],
  })
  status(c, {
    mode: 'NEW',
    hints: ui.typing
      ? [
          { key: '↑↓', label: 'pick' },
          { key: '↵', label: 'done' },
          { key: 'esc', label: 'clear' },
        ]
      : [
          { key: '↵', label: step === 2 ? 'create' : 'next', region: step === 2 ? 'create' : `tab:${step + 1}` },
          ...(step ? [{ key: '←', label: 'back', region: `tab:${step - 1}` }] : [{ key: 'space', label: 'snapshots', region: 'snapshots' }]),
          { key: '/', label: step === 2 ? 'rename' : 'filter', region: 'query' },
          { key: 'esc', label: 'cancel', region: 'screen:home' },
        ],
  })
}

// ─── Overlays (home) ────────────────────────────────────────────────────────

function overlays(c: Ctx, it: Item | undefined) {
  const { ui } = c
  if (!ui.overlay) return
  backdrop(c)
  if (ui.overlay === 'help') help(c)
  else if (ui.overlay === 'palette') palette(c)
  else if (ui.overlay === 'menu') menu(c, it)
  else if (ui.overlay === 'confirm') confirm(c, it)
}

/** Help is a which-key drawer: every key on home, grouped, in four columns. */
function help(c: Ctx) {
  const { buf, t } = c
  const { col } = page(c)
  const groups: [string, [string, string][]][] = [
    ['Move', [['↑↓ jk', 'pick'], ['pgup dn', 'page'], ['g G', 'ends'], ['esc', 'clear']]],
    ['Act', [['↵', 'play, open'], ['o', 'offline'], ['e', 'edit'], ['x', 'more'], ['d', 'delete']]],
    ['Go', [['m', 'mods'], ['n', 'new'], ['a', 'accounts'], ['s', 'settings']]],
    ['Always', [['/', 'search'], [':', 'command'], ['?', 'this help'], ['q', 'quit']]],
  ]
  const rowsN = Math.max(...groups.map(([, k]) => k.length))
  const r = drawer(c, rowsN + 2, col.x, col.w)
  const colW = Math.floor(r.w / groups.length)
  groups.forEach(([title, keys], gi) => {
    const x = r.x + gi * colW
    const keyW = Math.max(...keys.map(([k]) => width(k))) + 2
    buf.text(x, r.y, title.toUpperCase(), { fg: t.muted, bold: true })
    keys.forEach(([k, d], i) => {
      buf.text(x, r.y + 2 + i, k, { fg: t.accent, bold: true }, keyW)
      buf.text(x + keyW, r.y + 2 + i, d, { fg: t.dim }, colW - keyW - 1)
    })
  })
  status(c, { mode: 'HELP', hints: [{ key: 'esc', label: 'close', region: 'close' }] })
}

/** The command line lives in the status row, vim-style; completions rise above it. */
function palette(c: Ctx) {
  const { buf, t, ui, data } = c
  const { col } = page(c)
  const inst = data.instances[0].name
  const cmds: [string, string, string][] = [
    ['play', `Play ${inst}`, '↵'],
    ['offline', `Play ${inst} offline`, 'o'],
    ['new', 'New instance', 'n'],
    ['mods', `Browse mods for ${inst}`, 'm'],
    ['accounts', 'Switch or add accounts', 'a'],
    ['settings', 'Theme, Java, snapshots', 's'],
    ['theme', 'Try another palette', ''],
    ['quit', 'Leave mctui', 'q'],
  ]
  const q = ui.oquery.trim()
  const found = cmds
    .map((cmd) => {
      const byName = fuzzy(q, cmd[0])
      const byDesc = byName ? null : fuzzy(q, cmd[1])
      return { cmd, m: byName ?? (byDesc && { ...byDesc, score: byDesc.score - 6 }) }
    })
    .filter((x) => x.m)
    .sort((a, b) => (q ? b.m!.score - a.m!.score : 0))
  const sel = c.osel(Math.max(1, found.length))

  const h = Math.min(Math.max(1, found.length), buf.rows - 6)
  const r = drawer(c, h, col.x, col.w)
  if (!found.length) buf.text(r.x + 3, r.y, 'No command by that name.', { fg: t.faint })
  found.slice(0, h).forEach(({ cmd: [name, desc, key] }, i) => {
    const row = rect(r.x - 2, r.y + i, r.w + 4, 1)
    const on = i === sel
    const hov = c.hovered(`osel:${i}`)
    if (on) buf.fill(row, { bg: t.selection })
    else if (hov) buf.fill(row, { bg: t.surface })
    buf.text(r.x, row.y, ':', { fg: on ? t.accent : t.faint, bold: on })
    const m = fuzzy(q, name)
    hl(c, r.x + 3, row.y, name, m?.hits, on ? { fg: t.strong, bold: true } : { fg: t.text }, 11)
    buf.text(r.x + 15, row.y, desc, { fg: on ? t.subtle : t.dim }, r.w - 20)
    if (key) buf.lineRight(rect(r.x, row.y, r.w, 1), [[key, { fg: t.muted, bold: true }]])
    buf.region(row, `osel:${i}`)
  })

  const st = status(c, { mode: 'COMMAND', right: [[`${found.length} of ${cmds.length}  `, { fg: t.muted }]] })
  const y = buf.rows - 1
  const cx = buf.text(st.x, y, ':', { fg: t.accent, bold: true })
  const end = buf.text(cx, y, q, { fg: t.strong }, st.rx - cx - 2)
  buf.cursor = { x: end, y }
}

/** `x`: every action for the selected result, keys flush right, over the status line's corner. */
function menu(c: Ctx, it: Item | undefined) {
  const { buf, t, data } = c
  const { col } = page(c)
  const name = it?.section === 'Instances' ? it.title : data.instances[0].name
  const items: [string, string, boolean?][] = [
    ['Play', '↵'],
    ['Play offline', 'o'],
    ['Edit', 'e'],
    ['Mods', 'm'],
    ['Resource packs', 'r'],
    ['Open folder', 'f'],
    ['Duplicate', 'c'],
    ['Delete', 'd', true],
  ]
  const sel = c.osel(items.length)
  const r = popover(c, items.length + 3, 36, right(col))
  head(c, r, [[name, { fg: t.strong, bold: true }]], [['actions', { fg: t.muted }]])
  items.forEach(([label, key, danger], i) => {
    const row = rect(r.x - 2, r.y + 3 + i, r.w + 4, 1)
    const on = i === sel
    const hov = c.hovered(`osel:${i}`)
    if (on) buf.fill(row, { bg: danger ? t.errorBg : t.selection })
    else if (hov) buf.fill(row, { bg: t.surface })
    buf.text(r.x, row.y, label, danger ? { fg: t.error, bold: on } : on ? { fg: t.strong, bold: true } : { fg: t.text })
    buf.lineRight(rect(r.x, row.y, r.w, 1), [[key, { fg: danger ? t.error : on ? t.accent : t.muted, bold: true }]])
    buf.region(row, `osel:${i}`)
  })
  status(c, {
    mode: 'ACTIONS',
    hints: [
      { key: '↵', label: 'run' },
      { key: 'esc', label: 'close', region: 'close' },
    ],
  })
}

/** Delete asks at the bottom, where the app always talks, and opens on Keep. */
function confirm(c: Ctx, it: Item | undefined) {
  const { buf, t, data } = c
  const { col } = page(c)
  const inst = data.instances.find((i) => i.name === it?.title) ?? data.instances[0]
  const r = drawer(c, 4, col.x, col.w)
  buf.line(r.x, r.y, [['✗  ', { fg: t.error }], [`Delete “${inst.name}”?`, { fg: t.error, bold: true }]])
  const worlds = inst.worlds.length
  const what = [worlds ? `${worlds} ${worlds === 1 ? 'world' : 'worlds'}` : '', inst.mods.length ? `${inst.mods.length} mods` : '']
    .filter(Boolean)
    .join(' and ')
  buf.text(r.x + 3, r.y + 1, `${what ? `Its ${what} go too. ` : ''}This can’t be undone.`, { fg: t.text }, r.w - 3)
  const k = w.button(c, r.x + 3, r.y + 3, 'Keep', '↵', 'secondary', 'close')
  w.button(c, right(k) + 2, r.y + 3, 'Delete', 'y', 'danger', 'delete')
  status(c, { mode: 'CONFIRM', tone: 'error', hints: [{ key: 'esc', label: 'keep', region: 'close' }] })
}

export default defineVariation({
  id: 'spotlight',
  name: 'Spotlight',
  order: 20,
  blurb:
    'Command-first and paneless: one prompt, one ranked list mixing instances, actions and worlds, the selected result opening in place, and a helix-style status line. Trades the always-visible detail pane and side-by-side context for speed and one place to look.',
  notes: [
    'Top is where you ask, bottom is where the app answers: the prompt and its rule sit on the first row; the status line (mode chip, context, live keys, account) owns the last. Help, the action menu and delete rise from the status line; `:` turns the status row into a vim command line.',
    'The selected result opens in place: a one-row tint on its title, then indented detail lines and its own keys (`↵ play  o offline  e edit  m mods`). No second pane, no stacked tints.',
    'Typing fuzzy-filters everything at once; matched letters light up in the accent, sections reorder so the top row is the top hit, and “Search Modrinth for …” is always the last resort.',
    'Mods, the game log and the new-instance flow reuse the same prompt with a scope chip (`mods`, `log`) or the answers so far as crumbs (`new › 1.21.4 › Fabric`).',
    'Trade-offs: less at a glance (one instance’s details at a time, no worlds tab), and it leans on the keyboard; mouse users get clickable rows, crumbs and hints but no visible panes to aim at.',
  ],
  screens: { home, mods, play, wizard },
})
