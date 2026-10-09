// The states a screen has to handle, as variations of the shared mock. The lab
// switches between them ("State" in the toolbar, `--scenario` for pnpm shot),
// so a design is judged on the moments that matter, not just the idle one.

import { mock, type Mock } from './mock'

export const SCENARIOS: { id: string; label: string; note: string; make: (m: Mock) => Mock }[] = [
  {
    id: 'idle',
    label: 'Idle',
    note: 'Nothing running. The usual moment: open mctui, pick, play.',
    make: (m) => m,
  },
  {
    id: 'running',
    label: 'Game running',
    note: 'Survival 1.21 has been running 18 minutes; you came back to the terminal.',
    make: (m) => ({
      ...m,
      scenario: 'running',
      running: {
        instance: 'Survival 1.21',
        elapsed: '18m',
        since: '14:02',
        world: 'New World',
        warnings: 3,
        errors: 1,
        memory: '2.1 of 4 GB',
      },
    }),
  },
  {
    id: 'downloading',
    label: 'Downloading',
    note: 'A first launch is fetching the game, and a mod update is queued.',
    make: (m) => ({
      ...m,
      scenario: 'downloading',
      tasks: [
        { label: 'Downloading Minecraft 26.3-snapshot-4', instance: 'Snapshot Test', ratio: 0.42, detail: '118 of 281 MB · 3,412 of 4,019 files' },
        { label: 'Updating Sodium to 0.6.5', instance: 'Survival 1.21', ratio: 0, detail: 'queued' },
      ],
    }),
  },
  {
    id: 'crashed',
    label: 'Just crashed',
    note: 'The game closed with an error a moment ago, while you were in it.',
    make: (m) => ({
      ...m,
      scenario: 'crashed',
      crash: {
        instance: 'Survival 1.21',
        when: 'just now',
        exit: 'exit code -1',
        reason: 'Mixin apply failed sodium.mixins.json:features.render — Sodium 0.6.0 needs Fabric API 0.112 or newer',
      },
    }),
  },
  {
    id: 'expired',
    label: 'Sign-in expired',
    note: 'The Microsoft sign-in needs renewing before playing online.',
    make: (m) => ({
      ...m,
      scenario: 'expired',
      account: { ...m.account, status: 'expired' },
      accounts: m.accounts.map((a) => (a.name === m.account.name ? { ...a, status: 'expired' as const } : a)),
    }),
  },
  {
    id: 'empty',
    label: 'First run',
    note: 'No instances yet, just signed in.',
    make: (m) => ({ ...m, scenario: 'empty', instances: [] }),
  },
  {
    id: 'many',
    label: 'Lots of instances',
    note: '14 instances: does the design scale past one screen?',
    make: (m) => ({
      ...m,
      scenario: 'many',
      instances: [
        ...m.instances,
        ...[
          ['Create: Above & Beyond', '1.18.2', 'Fabric', '4d ago'],
          ['Hypixel PvP', '1.8.9', 'Vanilla', '1w ago'],
          ['Speedrun Practice', '1.16.1', 'Fabric', '1w ago'],
          ['Better Adventures', '1.20.1', 'Fabric', '2w ago'],
          ['Realms', '26.3', 'Vanilla', '3w ago'],
          ['Redstone Lab', '1.21.4', 'Vanilla', '1mo ago'],
          ['Shader Test', '26.1.2', 'Fabric', '1mo ago'],
          ['Old School Beta', '1.7.10', 'Vanilla', '3mo ago'],
          ['Cobblemon', '1.21.1', 'Fabric', '5mo ago'],
        ].map(([name, mc, loader, last]) => ({
          name,
          mc,
          loader: loader as 'Vanilla' | 'Fabric',
          loaderVersion: loader === 'Fabric' ? '0.16.10' : undefined,
          lastPlayed: last,
          playtime: '6h 10m',
          worlds: [{ name: 'World', lastPlayed: last, mode: 'Survival' as const, size: '90 MB' }],
          mods: [],
          packs: [],
          memory: '4 GB',
          java: 'Java 21 (automatic)',
        })),
      ],
    }),
  },
]

export function scenario(id: string): Mock {
  return (SCENARIOS.find((s) => s.id === id) ?? SCENARIOS[0]).make(mock)
}
