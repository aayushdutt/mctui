// The real app's frames, from `make screenshots` (target/screenshots/*.ansi).

const files = import.meta.glob<string>('../../target/screenshots/*.ansi', {
  query: '?raw',
  import: 'default',
  eager: true,
})

export interface LiveShot {
  name: string
  group: string
  text: string
}

const GROUPS = ['home', 'mods', 'packs', 'wizard', 'sign-in', 'accounts', 'settings']

export const live: LiveShot[] = Object.entries(files)
  .map(([path, text]) => {
    const name = path.split('/').pop()!.replace(/\.ansi$/, '')
    const group = GROUPS.find((g) => name.startsWith(g)) ?? 'other'
    return { name, group, text }
  })
  .sort((a, b) => a.name.localeCompare(b.name))

export const liveGroups = [...GROUPS, 'other'].filter((g) => live.some((s) => s.group === g))
