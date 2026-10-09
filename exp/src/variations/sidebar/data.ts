// A little extra story on top of the shared mock, so every state reads true:
// the session the play screen shows when no scenario is running, how the
// crash ends the log, and what the crash says about the mods.

import type { Ctx } from '../../kit'
import type { InstalledMod, LogLine, Running } from '../../data/mock'

/** The play screen is a running game; in scenarios without one, this one. */
export const SESSION: Running = {
  instance: 'Survival 1.21',
  elapsed: '18m',
  since: '14:02',
  world: 'New World',
  warnings: 3,
  errors: 1,
  memory: '2.1 of 4 GB',
}

/** How the log ends when the game crashes. */
export const CRASH_TAIL: LogLine[] = [
  { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: 'Mixin apply failed sodium.mixins.json:features.render' },
  { time: '14:20:31', thread: 'Render thread', level: 'ERROR', text: 'Sodium 0.6.0 needs Fabric API 0.112 or newer (found 0.110.0)' },
  { time: '14:20:32', thread: 'Render thread', level: 'ERROR', text: 'Game crashed! Crash report saved to crash-reports/crash-2026-09-29_14.20.32-client.txt' },
]

export interface ModState {
  mod: InstalledMod
  /** The state word and its tone, or none. */
  note?: { text: string; tone: 'update' | 'warn' | 'error' | 'quiet' }
}

/** Installed mods with what's true about each, trouble first. */
export function modStates(c: Ctx, name: string, mods: InstalledMod[]): ModState[] {
  const { data } = c
  const crashed = data.crash?.instance === name
  const task = (mod: string) => data.tasks.find((k) => k.instance === name && k.label.includes(mod))
  const out = mods.map((mod): ModState => {
    if (crashed && mod.name === 'Fabric API') return { mod, note: { text: '↑ 0.112.0 fixes it', tone: 'warn' } }
    if (crashed && mod.name === 'Sodium') return { mod, note: { text: '✗ failed to load', tone: 'error' } }
    const k = task(mod.name)
    if (k) return { mod, note: { text: k.ratio > 0 ? `↓ ${Math.round(k.ratio * 100)}%` : `↓ ${mod.update ?? ''} queued`.replace('  ', ' '), tone: 'quiet' } }
    if (!mod.enabled) return { mod, note: { text: '○ disabled', tone: 'quiet' } }
    if (mod.update) return { mod, note: { text: `↑ ${mod.update}`, tone: 'update' } }
    return { mod }
  })
  const rank = (s: ModState) => (!s.note ? 4 : { error: 0, warn: 1, update: 2, quiet: 3 }[s.note.tone])
  return out.map((s, i) => [s, i] as const).sort((a, b) => rank(a[0]) - rank(b[0]) || a[1] - b[1]).map(([s]) => s)
}
