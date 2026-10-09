// What the columns show, derived from the shared mock: an instance's state,
// the entries of its contents column, mod states, the game log.

import type { Ctx } from '../../kit'
import type { Instance, InstalledMod, LogLine, ModHit, Task, Version } from '../../data/mock'
import type { Line } from '../../term/buffer'
export { fuzzy } from './fuzzy'

export type Kind = 'log' | 'download' | 'worlds' | 'mods' | 'packs' | 'settings'

export interface Entry {
  kind: Kind
  label: string
  trail: Line
  count?: number
}

export type Phase = 'running' | 'crashed' | 'downloading' | 'queued' | 'new' | 'idle'

export interface InstState {
  phase: Phase
  task?: Task
  /** The crash happened this session (the loud one), not some time ago. */
  fresh?: boolean
}

export function stateOf(c: Ctx, inst: Instance): InstState {
  const { data } = c
  if (data.running?.instance === inst.name) return { phase: 'running' }
  if (data.crash?.instance === inst.name) return { phase: 'crashed', fresh: true }
  if (inst.trouble) return { phase: 'crashed' }
  const task = data.tasks.find((k) => k.instance === inst.name)
  if (task) return { phase: task.ratio > 0 ? 'downloading' : 'queued', task }
  if (inst.fresh) return { phase: 'new' }
  return { phase: 'idle' }
}

/** The dim trailing word in the instances column: state, else when it was last played. */
export function stateWord(c: Ctx, inst: Instance): Line {
  const { t } = c
  const st = stateOf(c, inst)
  switch (st.phase) {
    case 'running':
      return [['running', { fg: t.success }]]
    case 'crashed':
      return [['crashed', { fg: t.warning }]]
    case 'downloading':
      return [[`${c.spinner()} `, { fg: t.accent }], [`${Math.round(st.task!.ratio * 100)}%`, { fg: t.text }]]
    case 'queued':
      return [['queued', { fg: t.dim }]]
    case 'new':
      return [['new', { fg: t.dim }]]
    default:
      return [[inst.lastPlayed ?? '', { fg: t.muted }]]
  }
}

export const loaderName = (inst: Instance) => (inst.loader === 'Vanilla' ? 'Vanilla' : `${inst.loader} ${inst.loaderVersion}`)

// ── Mods ──────────────────────────────────────────────────────────────────

export interface ModRow extends InstalledMod {
  /** Why it won't load (the crash names it). */
  broken?: string
  queued?: boolean
  /** Updating this one fixes that broken mod. */
  fixes?: string
  hit?: ModHit
}

/** An instance's mods, A→Z, with what the crash and the task queue say about them. */
export function modRows(c: Ctx, inst: Instance): ModRow[] {
  const { data } = c
  const crash = data.crash?.instance === inst.name ? data.crash : undefined
  const need = crash?.reason.match(/needs (.+?) ([\d.]+) or newer/)
  return [...inst.mods]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((m) => {
      const r: ModRow = { ...m, hit: data.modHits.find((h) => h.name === m.name) }
      if (crash && need && crash.reason.includes(`${m.name} ${m.version}`)) r.broken = `needs ${need[1]} ${need[2]}`
      if (need && need[1] === m.name) {
        r.update = need[2]
        r.fixes = inst.mods.find((x) => crash!.reason.includes(`${x.name} ${x.version}`))?.name
      }
      r.queued = data.tasks.some((k) => k.instance === inst.name && k.label.includes(m.name))
      return r
    })
}

/** `0.6.0 ↑` / `0.6.0 ✗` / `off`: a mod row's right-hand side, the state in its own two-cell column. */
export function modTrail(c: Ctx, m: ModRow): Line {
  const { t } = c
  if (!m.enabled) return [['off', { fg: t.faint }], '  ']
  const out: Line = [[m.version, { fg: t.dim }]]
  if (m.broken) out.push([' ✗', { fg: t.error }])
  else if (m.queued) out.push([' ↑', { fg: t.muted }])
  else if (m.update) out.push([' ↑', { fg: t.accent }])
  else out.push('  ')
  return out
}

// ── The contents column ───────────────────────────────────────────────────

/**
 * An instance's contents. The game log is pinned to the top while it
 * matters (live, crashed), so it's the row the preview follows by default;
 * otherwise it sits with the rest.
 */
export function entries(c: Ctx, inst: Instance): Entry[] {
  const { t } = c
  const st = stateOf(c, inst)
  const mods = modRows(c, inst)
  const ups = mods.filter((m) => m.update && !m.broken).length
  const broken = mods.filter((m) => m.broken).length
  const out: Entry[] = []
  const pinned = st.phase === 'running' || st.phase === 'crashed'
  if (st.phase === 'running') out.push({ kind: 'log', label: 'Game log', trail: [['● live', { fg: t.success }]] })
  if (st.phase === 'crashed') out.push({ kind: 'log', label: 'Game log', trail: [['crashed', { fg: t.warning }]] })
  if (st.phase === 'downloading')
    out.push({ kind: 'download', label: 'Download', trail: [[`${Math.round(st.task!.ratio * 100)}%`, { fg: t.text }]] })
  out.push({ kind: 'worlds', label: 'Worlds', trail: [[String(inst.worlds.length), { fg: t.dim }]], count: inst.worlds.length })
  const modTrail: Line =
    inst.loader === 'Vanilla'
      ? [['vanilla', { fg: t.faint }]]
      : [
          ...(broken ? ([[`✗${broken}  `, { fg: t.error }]] as Line) : []),
          ...(ups ? ([[`↑${ups}  `, { fg: t.accent }]] as Line) : []),
          [String(mods.length), { fg: t.dim }],
        ]
  out.push({ kind: 'mods', label: 'Mods', trail: modTrail, count: mods.length })
  out.push({ kind: 'packs', label: 'Resource packs', trail: [[String(inst.packs.length), { fg: t.dim }]], count: inst.packs.length })
  if (!pinned) out.push({ kind: 'log', label: 'Game log', trail: inst.lastPlayed ? [] : [['empty', { fg: t.faint }]] })
  out.push({ kind: 'settings', label: 'Settings', trail: [[inst.mc, { fg: t.dim }]] })
  return out
}

// ── Settings ──────────────────────────────────────────────────────────────

export interface Setting {
  label: string
  value: string
  about: string
  /** Other values, the current one first-class. */
  choices?: string[]
}

export function settingsOf(inst: Instance): Setting[] {
  const java = inst.java.replace(/^Java /, '')
  return [
    {
      label: 'Minecraft',
      value: inst.mc,
      about: 'Changing the version keeps your worlds. Mods for another version are turned off until updated.',
    },
    {
      label: 'Loader',
      value: loaderName(inst),
      about:
        inst.loader === 'Vanilla'
          ? 'Just the game. Switch to Fabric to add mods; your worlds stay as they are.'
          : 'Fabric loads the mods in Mods. mctui keeps it on the newest build for this version.',
    },
    { label: 'Memory', value: inst.memory, about: 'How much memory the game may use. Your Mac has 18 GB.', choices: ['2 GB', '3 GB', '4 GB', '6 GB', '8 GB'] },
    { label: 'Java', value: java, about: `mctui picks the Java that ${inst.mc} needs and downloads it when missing.` },
    { label: 'Name', value: inst.name, about: 'Also the folder name under ~/Library/Application Support/mctui.' },
  ]
}

// ── Game log ──────────────────────────────────────────────────────────────

/** The log of the instance's current or last run. A crash ends it. */
export function logOf(c: Ctx, inst: Instance): LogLine[] {
  const { data } = c
  const crash = data.crash?.instance === inst.name ? data.crash : undefined
  if (crash)
    return [
      ...data.log.slice(0, -4),
      { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: 'Mixin apply failed sodium.mixins.json:features.render' },
      { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: crash.reason.split(' — ').pop()! },
      { time: '14:20:32', thread: 'main', level: 'INFO', text: `Game exited with ${crash.exit}` },
    ]
  return data.log
}

export const logFg = (c: Ctx, l: LogLine) => (l.level === 'ERROR' ? c.t.error : l.level === 'WARN' ? c.t.warning : c.t.subtle)
export const logMark = (l: LogLine) => (l.level === 'ERROR' ? '✗' : l.level === 'WARN' ? '▲' : ' ')

// ── New instance ──────────────────────────────────────────────────────────

export interface LoaderChoice {
  name: 'Fabric' | 'Vanilla'
  version?: string
  about: string
}

/** Fabric runs on 1.14 and newer. */
export function fabricOk(v: Version): boolean {
  const [a, b] = v.id.split(/[.-]/).map(Number)
  return a >= 2 || (a === 1 && b >= 14)
}

export function loadersFor(v: Version): LoaderChoice[] {
  const vanilla: LoaderChoice = { name: 'Vanilla', about: 'Just the game, no mods.' }
  if (!fabricOk(v)) return [vanilla]
  return [{ name: 'Fabric', version: '0.17.2', about: 'Loads mods. Add them any time from Modrinth.' }, vanilla]
}

export const defaultName = (v: Version, l: LoaderChoice) => (l.name === 'Fabric' ? `${v.id} Fabric` : v.id)
