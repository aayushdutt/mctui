// What Ledger derives from the shared mock: sortable numbers from the human
// strings, each instance's state, and a little extra content (a few more
// installed mods for Survival, side info, loaders) so the tables have rows to
// judge. Everything else comes from `c.data` as-is.

import type { Instance, InstalledMod, LogLine, Mock, ModHit } from '../../data/mock'

// ── Time and version strings → numbers ─────────────────────────────────────

const UNIT: Record<string, number> = { m: 1, h: 60, d: 1440, w: 10080, mo: 43200, y: 525600 }

/** Minutes since `2h ago` / `yesterday` / `3mo ago`; never played is Infinity. */
export function ageMinutes(s: string | null): number {
  if (!s) return Infinity
  if (s === 'now' || s === 'just now') return 0
  if (s === 'yesterday') return 1440
  const m = s.match(/^(\d+)\s*(mo|m|h|d|w|y)/)
  return m ? Number(m[1]) * UNIT[m[2]] : Infinity
}

/** `yesterday` → `1d ago`, so the column reads as one unit. */
export function ageText(s: string | null): string {
  if (!s) return 'never'
  return s === 'yesterday' ? '1d ago' : s
}

/** `4h 12m` → 252. */
export function playMinutes(s: string): number {
  const h = s.match(/(\d+)h/)
  const m = s.match(/(\d+)m/)
  return (h ? Number(h[1]) * 60 : 0) + (m ? Number(m[1]) : 0)
}

/** Newest first when negated: compares `1.21.4`, `26.3-snapshot-4`, `1.8.9`. */
export function cmpVersion(a: string, b: string): number {
  const pa = a.split(/[.-]/).map((p) => (/^\d+$/.test(p) ? Number(p) : -1))
  const pb = b.split(/[.-]/).map((p) => (/^\d+$/.test(p) ? Number(p) : -1))
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? -2) - (pb[i] ?? -2)
    if (d) return d
  }
  return 0
}

/** `26.3-snapshot-4` squeezed into `w` cells without losing which snapshot. */
export function versionFit(v: string, w: number): string {
  if (v.length <= w) return v
  const short = v.replace('-snapshot-', '-snap-')
  if (short.length <= w) return short
  return v.replace('-snapshot-', 's')
}

// ── Mods ───────────────────────────────────────────────────────────────────

/** A few more of what Survival's log says it loads, so the mods table has rows. */
const SURVIVAL_EXTRA: InstalledMod[] = [
  { name: 'Iris Shaders', version: '1.8.1', enabled: true, update: '1.8.8' },
  { name: 'Mod Menu', version: '13.0.2', enabled: true },
  { name: 'Zoomify', version: '2.14.2', enabled: false },
]

export function modsOf(inst: Instance): InstalledMod[] {
  if (inst.name !== 'Survival 1.21') return inst.mods
  // The crash says Sodium needs Fabric API 0.112, so there is one.
  const own = inst.mods.map((m) => (m.name === 'Fabric API' && !m.update ? { ...m, update: '0.112.1' } : m))
  return [...own, ...SURVIVAL_EXTRA]
}

export const updatesOf = (inst: Instance) => modsOf(inst).filter((m) => m.update)

/** More Modrinth results than the shared eight, for a table that scrolls. */
export const MORE_HITS: ModHit[] = [
  hit('Cloth Config API', 'shedaniel', '112.4M', 'Library', 'Client & server', 'Configuration library for Minecraft mods.', '2mo ago'),
  hit("Xaero's Minimap", 'xaero96', '58.5M', 'Adventure', 'Client-side', 'Displays a minimap of the explored world, with waypoints.', '4d ago'),
  hit('FerriteCore', 'malte0811', '52.9M', 'Optimization', 'Client & server', 'Memory usage optimizations.', '3w ago'),
  hit('AppleSkin', 'squeek502', '47.8M', 'Utility', 'Client & server', 'Adds food-related HUD improvements: hunger, saturation, exhaustion.', '5mo ago'),
  hit('Jade', 'Snownee', '44.1M', 'Utility', 'Client & server', "Shows information about what you're looking at.", '2w ago'),
  hit('Indium', 'comp500', '31.6M', 'Library', 'Client-side', 'Sodium addon providing support for the Fabric Rendering API.', '1mo ago'),
  hit('ModernFix', 'embeddedt', '28.3M', 'Optimization', 'Client & server', 'Improves performance, reduces memory use and fixes many bugs.', '6d ago'),
  hit('Sodium Extra', 'FlashyReese', '16.9M', 'Optimization', 'Client-side', 'A Sodium addon that adds features that shouldn’t be in Sodium.', '1mo ago'),
]

function hit(name: string, author: string, downloads: string, category: string, side: string, description: string, updated: string): ModHit {
  return { name, author, downloads, category, side, description, updated }
}

export function hitsOf(data: Mock): ModHit[] {
  return [...data.modHits, ...MORE_HITS]
}

/** Modrinth facts for an installed mod (by name), when the mock has them. */
export function factsOf(data: Mock, name: string): ModHit | undefined {
  return hitsOf(data).find((h) => h.name === name)
}

/** `Client & server` → `client & server`, `Client-side` → `client`. */
export function sideText(side: string | undefined): string {
  if (!side) return 'client'
  return side === 'Client-side' ? 'client' : side === 'Server-side' ? 'server' : 'client & server'
}

export const UPDATE_NOTES: Record<string, string> = {
  'Fabric API': 'is what Sodium 0.6 and newer need',
  Sodium: 'fixes flickering chunks on macOS and a crash with Iris',
  'Iris Shaders': 'shader packs load 30% faster; needs Sodium 0.6.5',
}

// ── Instance state: what the STATE column says ─────────────────────────────

export type StateKind = 'running' | 'downloading' | 'queued' | 'crashed' | 'trouble' | 'new' | 'ready'

export interface State {
  kind: StateKind
  /** Downloads: 0..1. */
  ratio?: number
  /** Running: the elapsed time. Downloads: what's being fetched. */
  detail?: string
}

export function stateOf(data: Mock, inst: Instance): State {
  if (data.running?.instance === inst.name) return { kind: 'running', detail: data.running.elapsed }
  const task = data.tasks.find((t) => t.instance === inst.name)
  if (task && task.ratio > 0) return { kind: 'downloading', ratio: task.ratio, detail: task.label }
  if (data.crash?.instance === inst.name) return { kind: 'crashed' }
  if (task) return { kind: 'queued', detail: task.label }
  if (inst.trouble) return { kind: 'trouble' }
  if (inst.fresh) return { kind: 'new' }
  return { kind: 'ready' }
}

/** The session the play screen shows (the scenario's, or one just launched). */
export function sessionOf(data: Mock) {
  return (
    data.running ?? {
      instance: data.crash?.instance ?? data.instances[0]?.name ?? 'Survival 1.21',
      elapsed: '18m',
      since: '14:02',
      world: 'New World',
      warnings: 3,
      errors: 1,
      memory: '2.1 of 4 GB',
    }
  )
}

/** How a crashed session's log ends. */
export const CRASH_TAIL: LogLine[] = [
  { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: 'Mixin apply failed sodium.mixins.json:features.render' },
  { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: 'Sodium 0.6.0 needs Fabric API 0.112 or newer (found 0.110.0)' },
  { time: '14:20:31', thread: 'main', level: 'ERROR', text: 'Game crashed! Report saved to crash-reports/crash-2026-09-29_14.20.31-client.txt' },
  { time: '14:20:32', thread: 'main', level: 'INFO', text: 'Stopping! exit code -1' },
]

/** The game kept going after the shared log's last line: more tail for 140×40. */
export const LOG_TAIL: LogLine[] = [
  { time: '14:20:10', thread: 'Server thread', level: 'INFO', text: 'ThreadedAnvilChunkStorage (New World): All chunks are saved' },
  { time: '14:21:37', thread: 'Render thread', level: 'INFO', text: '[CHAT] Steve has made the advancement [Acquire Hardware]' },
  { time: '14:23:05', thread: 'Server thread', level: 'WARN', text: "Can't keep up! Is the server overloaded? Running 2051ms or 41 ticks behind" },
  { time: '14:25:10', thread: 'Server thread', level: 'INFO', text: "Saving chunks for level 'ServerLevel[New World]'/minecraft:the_nether" },
]

// ── The wizard's loader step ────────────────────────────────────────────────

export const LOADERS: { name: 'Fabric' | 'Vanilla'; version: string; note: string }[] = [
  { name: 'Fabric', version: '0.17.2', note: 'latest · mods from Modrinth' },
  { name: 'Vanilla', version: '', note: 'the game as Mojang ships it, no mods' },
  { name: 'Fabric', version: '0.17.1', note: '' },
  { name: 'Fabric', version: '0.16.14', note: '' },
  { name: 'Fabric', version: '0.16.10', note: '' },
]
