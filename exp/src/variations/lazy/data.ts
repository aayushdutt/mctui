// What Lazy's main view needs beyond the shared mock: how each instance's
// last session went, world and pack facts, what a mod update brings, and
// the log a crash leaves behind.

import type { Instance, InstalledMod, LogLine, Mock, ModHit } from '../../data/mock'

export const slug = (name: string) => name.toLowerCase().replace(/[^a-z0-9.]+/g, '-').replace(/^-|-$/g, '')

export const folder = (name: string) => `~/.local/share/mctui/${slug(name)}`

export interface Session {
  world: string
  length: string
  end: string
  ok: boolean
}

/** How the last session went, per instance. */
export const lastSession: Record<string, Session> = {
  'Survival 1.21': { world: 'New World', length: '1h 04m', end: 'closed normally', ok: true },
  'Creative Build': { world: 'Castle Plots', length: '2h 10m', end: 'closed normally', ok: true },
  'Skyblock Classic': { world: 'Skyblock', length: '12s', end: 'crashed on start, exit code 1', ok: false },
  '26.1.2 Fabric': { world: '11 jun 26', length: '3h 22m', end: 'closed normally', ok: true },
}

export const worldFacts: Record<string, { seed: string; difficulty: string; version: string; backups: string }> = {
  'New World': { seed: '-4172144997902289642', difficulty: 'Normal', version: '1.21.4', backups: '2, newest Sep 27' },
  'Hardcore Run': { seed: '8675309', difficulty: 'Hard', version: '1.21.4', backups: 'none' },
  'Castle Plots': { seed: '0 (superflat)', difficulty: 'Peaceful', version: '1.20.1', backups: '1, Sep 20' },
  Skyblock: { seed: 'custom map', difficulty: 'Normal', version: '1.8.9', backups: 'none' },
  '11 jun 26': { seed: '3291742011', difficulty: 'Normal', version: '26.1.2', backups: '4, newest yesterday' },
  World: { seed: '1440192837', difficulty: 'Normal', version: '—', backups: 'none' },
}

export const packFacts: Record<string, { res: string; size: string; about: string }> = {
  'Faithful 32x': { res: '32×', size: '14 MB', about: 'The vanilla look at twice the resolution.' },
  'Vanilla Tweaks': { res: '16×', size: '2 MB', about: 'Small fixes to vanilla textures: lower fire, clear glass, quieter ores.' },
  'Fresh Animations': { res: '16×', size: '6 MB', about: 'Mobs that move with more life: blinking, breathing, turning heads.' },
}

/** What an update brings, for the mod details. */
export const changes: Record<string, string[]> = {
  Sodium: ['Fixes a crash with Iris 1.8 on macOS', 'Faster chunk meshing on Apple silicon', 'Needs Fabric API 0.112 or newer'],
  'Fabric API': ['Supports Sodium 0.6 and newer', 'Fixes item tooltips flickering in creative'],
}

/** A Modrinth entry for an installed mod (or a quiet stand-in). */
export function hitFor(data: Mock, name: string): ModHit {
  return (
    data.modHits.find((h) => h.name === name) ?? {
      name,
      author: 'unknown',
      downloads: '—',
      category: 'Mod',
      side: 'Client-side',
      description: '',
      updated: '—',
    }
  )
}

export const needs: Record<string, string> = {
  'Fabric API': 'nothing',
  Sodium: 'nothing',
  Lithium: 'nothing',
  'Iris Shaders': 'Sodium',
  'Mod Menu': 'Fabric API',
  Zoomify: 'YetAnotherConfigLib',
  'Entity Culling': 'nothing',
  Continuity: 'Fabric API',
}

/** The crash in the `crashed` scenario blames Fabric API; this is the fix. */
export const crashFix = { mod: 'Fabric API', to: '0.112.1', why: 'Sodium 0.6.0 needs 0.112 or newer' }

/** The log a crash leaves: the shared log's start, then what went wrong. */
export const crashTail: LogLine[] = [
  { time: '14:31:02', thread: 'Render thread', level: 'INFO', text: 'Reloading ResourceManager: vanilla, fabric, sodium, lithium' },
  { time: '14:31:03', thread: 'Render thread', level: 'ERROR', text: 'Mixin apply failed sodium.mixins.json:features.render' },
  { time: '14:31:03', thread: 'Render thread', level: 'ERROR', text: 'java.lang.NoSuchMethodError: FabricRenderApi.getRenderer()' },
  { time: '14:31:03', thread: 'Render thread', level: 'ERROR', text: 'Game crashed. Report saved to crash-reports/crash-2026-09-29.txt' },
]

/** The instance a running game, a download or a crash belongs to. */
export function stateOf(data: Mock, inst: Instance) {
  const running = data.running?.instance === inst.name
  const task = data.tasks.find((t) => t.instance === inst.name && t.ratio > 0)
  const queued = data.tasks.find((t) => t.instance === inst.name && t.ratio === 0)
  const crashed = data.crash?.instance === inst.name
  return { running, task, queued, crashed }
}

/** A default name for a new instance. */
export const defaultName = (version: string, loader: string) => (loader === 'Vanilla' ? version : `${version} ${loader}`)

export const loaders: { name: 'Fabric' | 'Vanilla'; version?: string; note: string }[] = [
  { name: 'Fabric', version: '0.17.2', note: 'runs mods' },
  { name: 'Vanilla', note: 'no mods' },
]

export const downloadSize: Record<string, string> = {
  '26.3': '312 MB',
  '26.2': '305 MB',
  '26.1.2': '298 MB',
  '26.1': '297 MB',
}

/** More Modrinth results so the search list has something to scroll. */
export const moreHits: ModHit[] = [
  hit('Cloth Config API', 'shedaniel', '112.4M', 'Library', 'Client & server', 'Configuration library for Minecraft mods.', '2mo ago'),
  hit("Xaero's Minimap", 'xaero96', '58.5M', 'Adventure', 'Client-side', 'Displays a minimap of the explored world with waypoints.', '4d ago'),
  hit('FerriteCore', 'malte0811', '52.9M', 'Optimization', 'Client & server', 'Memory usage optimizations.', '3w ago'),
  hit('AppleSkin', 'squeek502', '47.8M', 'Utility', 'Client & server', 'Adds various food-related HUD improvements.', '5mo ago'),
  hit('Jade', 'Snownee', '44.1M', 'Utility', 'Client & server', "Shows information about what you're looking at.", '2w ago'),
  hit('Indium', 'comp500', '31.6M', 'Library', 'Client-side', 'Sodium addon providing support for the Fabric Rendering API.', '1mo ago'),
  hit('ModernFix', 'embeddedt', '28.3M', 'Optimization', 'Client & server', 'All-in-one mod that improves performance, reduces memory usage, and fixes many bugs.', '6d ago'),
  hit('Krypton', 'astei', '21.0M', 'Optimization', 'Client & server', 'A mod to optimize the Minecraft networking stack.', '7mo ago'),
  hit('Dynamic FPS', 'juliand665', '19.4M', 'Optimization', 'Client-side', 'Reduce resource usage while Minecraft is in the background, idle or on battery.', '2w ago'),
  hit('LambDynamicLights', 'LambdAurora', '18.2M', 'Decoration', 'Client-side', 'Adds dynamic lights to the game: torches in hand light up the world around you.', '1mo ago'),
  hit('Sodium Extra', 'FlashyReese', '16.9M', 'Optimization', 'Client-side', 'A Sodium addon that adds features that shouldn’t be in Sodium.', '1mo ago'),
  hit('Chat Heads', 'dzwdz', '9.8M', 'Utility', 'Client-side', 'Adds player heads next to their names in chat.', '3mo ago'),
]

function hit(name: string, author: string, downloads: string, category: string, side: string, description: string, updated: string): ModHit {
  return { name, author, downloads, category, side, description, updated }
}

needs.Indium = 'Sodium, Fabric API'
needs['Sodium Extra'] = 'Sodium'
needs["Xaero's Minimap"] = 'nothing'
needs.Jade = 'nothing'

/** `s` cut to `max` cells from the left, keeping the end (paths: the file matters). */
export function keepEnd(s: string, max: number) {
  if (s.length <= max) return s
  return max <= 1 ? '…' : '…' + s.slice(s.length - (max - 1))
}

/** An instance's mods, what needs you first: broken, then updates, then the rest; turned-off ones last. */
export function sortedMods(data: Mock, inst: Instance | null): InstalledMod[] {
  if (!inst) return []
  const crash = data.crash?.instance === inst.name
  const rank = (m: InstalledMod) => (crash && m.name === crashFix.mod ? 0 : m.update ? 1 : m.enabled ? 2 : 3)
  return inst.mods.map((m, i) => ({ m, i })).sort((a, b) => rank(a.m) - rank(b.m) || a.i - b.i).map((x) => x.m)
}
