// Content only this direction needs: more Modrinth hits (so a short query
// finds several), the actions the home prompt can reach, and small helpers.

import type { Instance, ModHit } from '../../data/mock'

export const extraHits: ModHit[] = [
  {
    name: 'Sodium Extra',
    author: 'FlashyReese',
    downloads: '34.5M',
    category: 'Optimization',
    side: 'Client-side',
    description: 'Features that should be in Sodium: fog, particle and animation toggles, a better FPS counter.',
    updated: '1w ago',
  },
  {
    name: "Reese's Sodium Options",
    author: 'FlashyReese',
    downloads: '28.9M',
    category: 'Utility',
    side: 'Client-side',
    description: 'An alternative, tabbed video settings menu for Sodium.',
    updated: '3w ago',
  },
  {
    name: 'Sound Physics Remastered',
    author: 'henkelmax',
    downloads: '12.1M',
    category: 'Decoration',
    side: 'Client-side',
    description: 'Realistic sound attenuation, reverberation and absorption through blocks.',
    updated: '2mo ago',
  },
]

/** `2h ago` / `yesterday` / `3d ago` as minutes, for ranking by recency. */
export function ago(s: string | null): number {
  if (!s) return Infinity
  if (s === 'yesterday') return 1440
  const m = /(\d+)\s*(mo|m|h|d|w)/.exec(s)
  if (!m) return Infinity
  const unit: Record<string, number> = { m: 1, h: 60, d: 1440, w: 10080, mo: 43200 }
  return Number(m[1]) * unit[m[2]]
}

/** `262.7M` as a number, for sorting. */
export function downloads(s: string): number {
  const n = parseFloat(s)
  return s.endsWith('M') ? n * 1e6 : s.endsWith('K') ? n * 1e3 : n
}

/** The Java a Minecraft version wants. */
export function javaFor(mc: string): string {
  const [a, b = 0, c = 0] = mc.split(/[.-]/).map((p) => parseInt(p, 10) || 0)
  if (a >= 26) return 'Java 25'
  if (a === 1 && (b > 20 || (b === 20 && c >= 5))) return 'Java 21'
  if (a === 1 && b >= 18) return 'Java 17'
  if (a === 1 && b === 17) return 'Java 16'
  return 'Java 8'
}

export const loaderLabel = (i: Instance) => (i.loader === 'Vanilla' ? 'Vanilla' : `${i.loader} ${i.loaderVersion}`)

export interface HomeAction {
  title: string
  sub: string
  key?: string
  /** Where ↵ goes. */
  region: string
  glyph: string
  /** Only listed when the query finds it. */
  hidden?: boolean
}

export const homeActions: HomeAction[] = [
  { title: 'New instance', sub: 'version, loader, name', key: 'n', region: 'screen:wizard', glyph: '+' },
  { title: 'Browse mods', sub: 'Modrinth', key: 'm', region: 'screen:mods', glyph: '→' },
  { title: 'Accounts', sub: 'switch or sign in', key: 'a', region: 'screen:accounts', glyph: '→' },
  { title: 'Settings', sub: 'theme, Java, snapshots', key: 's', region: 'screen:settings', glyph: '→' },
  { title: 'Resource packs', sub: 'for the selected instance', region: 'screen:mods', glyph: '→', hidden: true },
  { title: 'Open instance folder', sub: 'in Finder', key: 'f', region: 'folder', glyph: '→', hidden: true },
  { title: 'Check for updates', sub: 'mods and loaders', key: 'u', region: 'updates', glyph: '↻', hidden: true },
  { title: 'Quit', sub: 'leave mctui', key: 'q', region: 'quit', glyph: '×', hidden: true },
]
