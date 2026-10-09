// New instance: three steps, each a table (or a field) with the right row
// already selected, over a drawer that shows what ↵ ↵ ↵ will make.

import type { Ctx } from '../../kit'
import type { Version } from '../../data/mock'
import { type Line, type Style, rect } from '../../term/buffer'
import { fuzzy } from './fuzzy'
import { LOADERS } from './data'
import { type Col, type Entry, type Hint, body, fit, footer, frame, head, ledger, lit, moreTail, prompt, rule, topLine } from './ui'
import { overlay } from './overlays'

const STEPS = ['version', 'loader', 'name'] as const

interface VRow {
  v: Version
  hits?: number[]
}

export function wizard(c: Ctx) {
  const { buf, t, data, ui } = c
  const f = frame(c)
  const step = c.tabs(STEPS.length)

  // Choices so far: the lab keeps one selection, so earlier steps show their defaults.
  const versions: VRow[] = []
  data.versions.forEach((v) => {
    const m = step === 0 ? fuzzy(ui.query, v.id) : { hits: [] }
    if (m) versions.push({ v, hits: m.hits })
  })
  const n = step === 0 ? versions.length : step === 1 ? LOADERS.length : 1
  const sel = c.sel(Math.max(1, n))
  const version = step === 0 ? (versions[sel]?.v ?? data.versions[0]) : data.versions[0]
  const loader = step === 1 ? LOADERS[sel] : LOADERS[0]
  const loaderName = loader.name === 'Vanilla' ? 'Vanilla' : `Fabric ${loader.version}`
  const name = `${version.id} ${loader.name}`

  // ── Top line: the steps as crumbs ──
  const stepLine: Line = []
  STEPS.forEach((s, i) => {
    if (i) stepLine.push(['  ›  ', { fg: t.faint }])
    stepLine.push([`${i + 1} ${s}`, i === step ? { fg: t.strong, bold: true } : i < step ? { fg: t.text } : { fg: t.muted }])
  })
  topLine(c, f, ['new instance'], stepLine)
  // Clicking a step goes there.
  let sx = f.x + f.w
  for (let i = STEPS.length - 1; i >= 0; i--) {
    const w = `${i + 1} ${STEPS[i]}`.length
    sx -= w
    buf.region(rect(sx, f.top, w, 1), `tab:${i}`)
    sx -= 5
  }

  // ── Geometry ──
  const lastRow = f.foot - 2
  const dH = 5
  const tableRows = Math.max(1, Math.min(step === 2 ? 1 : n, lastRow - dH - 1 - f.body))
  const ruleY = f.body + tableRows + 1 + (step === 2 ? 1 : 0)
  const drawer = rect(f.x, ruleY + 1, f.w, Math.min(dH + 1, lastRow - ruleY))
  let tail: Line = []

  if (step === 0) {
    const inUse = (id: string) =>
      data.instances
        .filter((i) => i.mc === id)
        .map((i) => i.name)
        .join(', ')
    const cols: Col<VRow>[] = [
      {
        key: 'version',
        label: 'VERSION',
        w: 17,
        cell: (r, _w, on) => lit(c, r.v.id, r.hits, r.v.snapshot ? { fg: t.muted } : on ? { fg: t.strong, bold: true } : { fg: t.text }),
      },
      { key: 'released', label: 'RELEASED', w: 12, cell: (r, _w, on) => [[r.v.date, { fg: on ? t.subtle : t.dim }]] },
      {
        key: 'type',
        label: 'TYPE',
        w: 16,
        cell: (r, _w, on) =>
          r.v.snapshot
            ? [['snapshot', { fg: t.muted }]]
            : [['release', { fg: on ? t.subtle : t.dim }], [r.v.latest ? '  latest' : '', { fg: t.accent }]],
      },
      { key: 'yours', label: 'YOU HAVE', w: 20, drop: 1, spacer: true, cell: (r) => [[inUse(r.v.id), { fg: t.muted }]] },
    ]
    const placed = fit(cols, f.x, f.w, 3)
    head(c, f.head, placed)
    const { above, below } = body(c, f, f.body, tableRows, placed, versions, sel)
    tail = moreTail(c, above, below)
  } else if (step === 1) {
    const cols: Col<(typeof LOADERS)[number]>[] = [
      { key: 'loader', label: 'LOADER', w: 9, cell: (l, _w, on) => [[l.name, on ? { fg: t.strong, bold: true } : { fg: t.text }]] },
      { key: 'version', label: 'VERSION', w: 9, cell: (l, _w, on) => [[l.version || '–', { fg: l.version ? (on ? t.subtle : t.dim) : t.faint }]] },
      {
        key: 'note',
        label: 'NOTE',
        w: 30,
        spacer: true,
        cell: (l, _w, on) => {
          const st: Style = { fg: on ? t.subtle : t.dim }
          return l.note.startsWith('latest') ? [['latest', { fg: t.accent }], [l.note.slice(6), st]] : [[l.note, st]]
        },
      },
    ]
    const placed = fit(cols, f.x, f.w, 3)
    head(c, f.head, placed)
    body(c, f, f.body, tableRows, placed, LOADERS, sel)
  } else {
    buf.text(f.x, f.head, 'NAME', { fg: t.muted })
    const r = rect(f.x - 1, f.body, Math.min(f.w + 2, 44), 1)
    buf.fill(r, { bg: t.selection })
    const end = buf.text(f.x, f.body, name, { fg: t.strong, bold: true })
    buf.cursor = { x: end, y: f.body }
    buf.text(r.x + r.w + 2, f.body, 'type to change it', { fg: t.faint })
    buf.region(r, 'query')
  }

  // ── The drawer: what ↵ ↵ ↵ makes ──
  const val = (i: number, s: string, note = ''): Line => [
    [s, i === step ? { fg: t.strong, bold: true } : i < step ? { fg: t.text } : { fg: t.subtle }],
    [note, { fg: t.muted }],
  ]
  const entries: Entry[] = [
    { label: 'version', labelFg: step === 0 ? t.accent : undefined, line: val(0, `Minecraft ${version.id}`, step < 0 ? '' : version.latest ? '  latest release' : '') },
    { label: 'loader', labelFg: step === 1 ? t.accent : undefined, line: val(1, loaderName, step < 1 ? '  default' : '') },
    { label: 'name', labelFg: step === 2 ? t.accent : undefined, line: val(2, name, step < 2 ? '  default' : '') },
    { label: 'runs', line: [['with 4 GB memory on Java 25, picked for you  ·  ', { fg: t.dim }], ['e', { fg: t.text, bold: true }], [' changes it later', { fg: t.muted }]] },
  ]
  if (!ui.overlay) {
    rule(c, f, ruleY, [['New instance', { fg: t.strong, bold: true }], [`  step ${step + 1} of 3`, { fg: t.muted }]], tail)
    ledger(c, drawer, entries)
  }

  // ── Footer ──
  const next = step === 2 ? 'create' : `next: ${STEPS[step + 1]}`
  const hints: Hint[] = [['↵', next]]
  if (step > 0) hints.push(['←', 'back'])
  if (step === 0) hints.push(['/', 'filter'], ['space', 'hide snapshots'])
  hints.push(['esc', 'cancel'], ['?', 'help'])
  if (ui.typing && step === 0) prompt(c, f, '/', ui.query, 'filter versions', `${versions.length} of ${data.versions.length}`, [['↵', 'keep'], ['esc', 'clear']])
  else footer(c, f, hints)
  c.enter(step === 2 ? 'home' : `tab:${step + 1}`)
  if (ui.overlay) overlay(c, f, ruleY, drawer, 'wizard', null, null)
}
